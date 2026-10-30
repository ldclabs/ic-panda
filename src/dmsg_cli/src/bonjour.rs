//! Bonjour through the system's DNS Service Discovery API (`dns_sd.h`): a
//! receiving device transfer advertises its session as
//! [`dmsg_client::transfer::SERVICE`], and the sender finds it by name. Each call waits
//! on the API's socket for its replies, on the calling thread. Without it,
//! elsewhere than macOS, the person types the receiver's address in.

use dmsg_types::{Error, Result};
use std::time::Duration;

#[cfg(target_os = "macos")]
pub(crate) use dns_sd::{Advert, advertise, find};

/// An advertised session; dropping it withdraws it.
#[cfg(not(target_os = "macos"))]
pub(crate) struct Advert;

#[cfg(not(target_os = "macos"))]
fn unavailable() -> Error {
    Error::Unavailable("bonjour: not available here; type the address in".into())
}

/// Advertise `name` at `port`.
#[cfg(not(target_os = "macos"))]
pub(crate) fn advertise(_name: &str, _port: u16) -> Result<Advert> {
    Err(unavailable())
}

/// The host and port advertised as `name`.
#[cfg(not(target_os = "macos"))]
pub(crate) fn find(_name: &str, _timeout: Duration) -> Result<(String, u16)> {
    Err(unavailable())
}

#[cfg(target_os = "macos")]
mod dns_sd {
    use super::*;
    use dmsg_client::transfer;
    use std::cell::{Cell, RefCell};
    use std::ffi::{CStr, CString, c_char, c_void};
    use std::ptr::{null, null_mut};
    use std::time::Instant;

    type Ref = *mut c_void;
    type Flags = u32;
    type Code = i32;

    /// `kDNSServiceFlagsAdd`: the service appeared.
    const ADD: Flags = 0x2;
    /// `kDNSServiceErr_PolicyDenied`: local network access is off.
    const POLICY_DENIED: Code = -65570;
    /// How long the system takes to confirm a registration.
    const REPLY: Duration = Duration::from_secs(5);
    /// The TXT record: `v=1`.
    const TXT: [u8; 4] = [3, b'v', b'=', b'1'];

    type RegisterReply =
        extern "C" fn(Ref, Flags, Code, *const c_char, *const c_char, *const c_char, *mut c_void);
    type BrowseReply = extern "C" fn(
        Ref,
        Flags,
        u32,
        Code,
        *const c_char,
        *const c_char,
        *const c_char,
        *mut c_void,
    );
    type ResolveReply = extern "C" fn(
        Ref,
        Flags,
        u32,
        Code,
        *const c_char,
        *const c_char,
        u16,
        u16,
        *const u8,
        *mut c_void,
    );

    unsafe extern "C" {
        fn DNSServiceRegister(
            sd: *mut Ref,
            flags: Flags,
            interface: u32,
            name: *const c_char,
            kind: *const c_char,
            domain: *const c_char,
            host: *const c_char,
            port: u16,
            txt_len: u16,
            txt: *const c_void,
            reply: RegisterReply,
            context: *mut c_void,
        ) -> Code;
        fn DNSServiceBrowse(
            sd: *mut Ref,
            flags: Flags,
            interface: u32,
            kind: *const c_char,
            domain: *const c_char,
            reply: BrowseReply,
            context: *mut c_void,
        ) -> Code;
        fn DNSServiceResolve(
            sd: *mut Ref,
            flags: Flags,
            interface: u32,
            name: *const c_char,
            kind: *const c_char,
            domain: *const c_char,
            reply: ResolveReply,
            context: *mut c_void,
        ) -> Code;
        fn DNSServiceRefSockFD(sd: Ref) -> i32;
        fn DNSServiceProcessResult(sd: Ref) -> Code;
        fn DNSServiceRefDeallocate(sd: Ref);
    }

    /// The error a reply's nonzero code reports.
    fn failed(code: Code) -> Error {
        match code {
            POLICY_DENIED => Error::Unavailable("bonjour: local network access is denied".into()),
            code => Error::Unavailable(format!("bonjour: error {code}")),
        }
    }

    fn checked(code: Code) -> Result<()> {
        match code {
            0 => Ok(()),
            code => Err(failed(code)),
        }
    }

    fn cstring(text: &str) -> Result<CString> {
        CString::new(text).map_err(|_| Error::InvalidInput("bonjour name".into()))
    }

    /// One operation; dropping it ends it, and withdraws a registration.
    struct Service(Ref);

    // SAFETY: one thread drives an operation at a time, and only this
    // module's calls do.
    unsafe impl Send for Service {}

    impl Drop for Service {
        fn drop(&mut self) {
            // SAFETY: the reference came from a call that succeeded and is
            // released once.
            unsafe { DNSServiceRefDeallocate(self.0) }
        }
    }

    impl Service {
        /// Start an operation with `start`, which fills in the reference.
        fn start(start: impl FnOnce(*mut Ref) -> Code) -> Result<Self> {
            let mut sd = null_mut();
            checked(start(&mut sd))?;
            Ok(Self(sd))
        }

        /// Handle the next reply, its callback included, if one comes
        /// before `deadline`.
        fn process(&self, deadline: Instant) -> Result<()> {
            let left = deadline.saturating_duration_since(Instant::now());
            let mut fd = libc::pollfd {
                // SAFETY: the reference is live.
                fd: unsafe { DNSServiceRefSockFD(self.0) },
                events: libc::POLLIN,
                revents: 0,
            };
            let timeout = i32::try_from(left.as_millis()).unwrap_or(i32::MAX);
            // SAFETY: one valid pollfd. An interrupted wait is tried again.
            if unsafe { libc::poll(&mut fd, 1, timeout) } > 0 {
                // SAFETY: the reference is live and a reply is waiting.
                checked(unsafe { DNSServiceProcessResult(self.0) })?;
            }
            Ok(())
        }

        /// Handle replies until `done` has a value or `deadline` passes.
        fn until<T>(&self, deadline: Instant, done: impl Fn() -> Option<Result<T>>) -> Result<T> {
            loop {
                if let Some(result) = done() {
                    return result;
                }
                if Instant::now() >= deadline {
                    return Err(Error::Unavailable(
                        "bonjour: the other device was not found; type its address in".into(),
                    ));
                }
                self.process(deadline)?;
            }
        }
    }

    /// An advertised session; dropping it withdraws it.
    pub(crate) struct Advert(
        #[expect(dead_code, reason = "dropping it withdraws the service")] Service,
    );

    fn context<T>(state: &T) -> *mut c_void {
        (state as *const T).cast_mut().cast()
    }

    extern "C" fn registered(
        _: Ref,
        _: Flags,
        code: Code,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        context: *mut c_void,
    ) {
        // SAFETY: the context is the cell `advertise` passed, alive while
        // it handles replies; nothing handles them after it returns.
        unsafe { (*(context as *const Cell<Option<Code>>)).set(Some(code)) }
    }

    /// Advertise `name` at `port` on every interface, once the system
    /// confirms it.
    pub(crate) fn advertise(name: &str, port: u16) -> Result<Advert> {
        let (name, kind) = (cstring(name)?, cstring(transfer::SERVICE)?);
        let reply = Cell::new(None);
        // SAFETY: the strings and the cell outlive the replies handled
        // below; the TXT record is static.
        let service = Service::start(|sd| unsafe {
            DNSServiceRegister(
                sd,
                0,
                0,
                name.as_ptr(),
                kind.as_ptr(),
                null(),
                null(),
                port.to_be(),
                TXT.len() as u16,
                TXT.as_ptr().cast(),
                registered,
                context(&reply),
            )
        })?;
        service.until(Instant::now() + REPLY, || reply.get().map(checked))?;
        Ok(Advert(service))
    }

    #[derive(Default)]
    struct Browsed {
        error: Option<Code>,
        /// The interface, type and domain the service was found under.
        found: Option<(u32, CString, CString)>,
    }

    struct Browse<'a> {
        name: &'a CStr,
        state: RefCell<Browsed>,
    }

    extern "C" fn browsed(
        _: Ref,
        flags: Flags,
        interface: u32,
        code: Code,
        name: *const c_char,
        kind: *const c_char,
        domain: *const c_char,
        context: *mut c_void,
    ) {
        // SAFETY: the context is the `Browse` that `find` passed, alive
        // while it handles replies; the strings are valid in the callback.
        let browse = unsafe { &*(context as *const Browse) };
        let mut state = browse.state.borrow_mut();
        if code != 0 {
            state.error = Some(code);
        } else if flags & ADD != 0
            && state.found.is_none()
            && unsafe { CStr::from_ptr(name) } == browse.name
        {
            let (kind, domain) = unsafe { (CStr::from_ptr(kind), CStr::from_ptr(domain)) };
            state.found = Some((interface, kind.to_owned(), domain.to_owned()));
        }
    }

    #[derive(Default)]
    struct Resolved {
        error: Option<Code>,
        host: Option<(String, u16)>,
    }

    extern "C" fn resolved(
        _: Ref,
        _: Flags,
        _: u32,
        code: Code,
        _: *const c_char,
        host: *const c_char,
        port: u16,
        _: u16,
        _: *const u8,
        context: *mut c_void,
    ) {
        // SAFETY: the context is the cell `find` passed, alive while it
        // handles replies; the host name is valid in the callback.
        let state = unsafe { &*(context as *const RefCell<Resolved>) };
        let mut state = state.borrow_mut();
        match code {
            0 => {
                let host = unsafe { CStr::from_ptr(host) };
                state.host = Some((host.to_string_lossy().into_owned(), u16::from_be(port)));
            }
            code => state.error = Some(code),
        }
    }

    /// The host and port advertised as `name`, found within `timeout`.
    pub(crate) fn find(name: &str, timeout: Duration) -> Result<(String, u16)> {
        let deadline = Instant::now() + timeout;
        let (name, kind) = (cstring(name)?, cstring(transfer::SERVICE)?);
        let browse = Browse {
            name: &name,
            state: RefCell::default(),
        };
        // SAFETY: the type and the browse state outlive the replies
        // handled below.
        let browsing = Service::start(|sd| unsafe {
            DNSServiceBrowse(sd, 0, 0, kind.as_ptr(), null(), browsed, context(&browse))
        })?;
        let (interface, kind, domain) = browsing.until(deadline, || {
            let mut state = browse.state.borrow_mut();
            match state.error {
                Some(code) => Some(Err(failed(code))),
                None => state.found.take().map(Ok),
            }
        })?;
        drop(browsing);
        let state = RefCell::new(Resolved::default());
        // SAFETY: the names and the state outlive the replies handled below.
        let resolving = Service::start(|sd| unsafe {
            DNSServiceResolve(
                sd,
                0,
                interface,
                name.as_ptr(),
                kind.as_ptr(),
                domain.as_ptr(),
                resolved,
                context(&state),
            )
        })?;
        resolving.until(deadline, || {
            let mut state = state.borrow_mut();
            match state.error {
                Some(code) => Some(Err(failed(code))),
                None => state.host.take().map(Ok),
            }
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn an_advertised_session_is_found_until_withdrawn() {
            let name = transfer::advert(&transfer::session_id());
            let advert = match advertise(&name, 4321) {
                Ok(advert) => advert,
                // No mDNSResponder to talk to, as in some sandboxes.
                Err(Error::Unavailable(reason)) => return eprintln!("skipped: {reason}"),
                Err(error) => panic!("{error:?}"),
            };
            let (host, port) = find(&name, Duration::from_secs(10)).unwrap();
            assert_eq!(port, 4321);
            assert!(host.ends_with(".local."), "{host}");
            drop(advert);
            let other = transfer::advert(&transfer::session_id());
            assert!(matches!(
                find(&other, Duration::from_millis(500)),
                Err(Error::Unavailable(_))
            ));
        }
    }
}
