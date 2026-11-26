//! Cross-platform `_dmsg-share._tcp` service advertisement and list discovery
//! for macOS (Bonjour `dns_sd.h`), Windows (Win32 DNS-SD with UDP mDNS fallback)
//! and Linux (pure-Rust RFC 6762/6763 multicast DNS via `socket2`).

use dmsg_client::nearby::{DiscoveredPeer, SERVICE, sanitize_alias};
use dmsg_types::{Error, Result};
use std::time::Duration;

/// An active `_dmsg-share._tcp` advertisement; dropping withdraws it.
pub(crate) struct NearbyAdvert {
    #[cfg(target_os = "macos")]
    _inner: macos::Advert,
    #[cfg(target_os = "windows")]
    _inner: windows_impl::Advert,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    _inner: udp_mdns::Advert,
}

/// Advertise this device's ephemeral nearby receive session over mDNS.
pub(crate) fn advertise(
    instance: &str,
    port: u16,
    alias: &str,
    pk_hint: &str,
) -> Result<NearbyAdvert> {
    #[cfg(target_os = "macos")]
    {
        Ok(NearbyAdvert {
            _inner: macos::advertise(instance, port, alias, pk_hint)?,
        })
    }
    #[cfg(target_os = "windows")]
    {
        Ok(NearbyAdvert {
            _inner: windows_impl::advertise(instance, port, alias, pk_hint)?,
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(NearbyAdvert {
            _inner: udp_mdns::advertise(instance, port, alias, pk_hint)?,
        })
    }
}

/// Browse nearby `_dmsg-share._tcp` peers on the local network for `timeout`.
pub(crate) fn browse(timeout: Duration) -> Result<Vec<DiscoveredPeer>> {
    #[cfg(target_os = "macos")]
    {
        macos::browse(timeout)
    }
    #[cfg(target_os = "windows")]
    {
        windows_impl::browse(timeout)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        udp_mdns::browse(timeout)
    }
}

/// Encode DNS-SD TXT entries (`v=1`, `n=<alias>`, `p=<pk_hint>`) as length-prefixed wire bytes.
pub(crate) fn encode_txt(alias: &str, pk_hint: &str) -> Vec<u8> {
    let clean = sanitize_alias(alias);
    let entries = [
        "v=1".to_string(),
        format!("n={clean}"),
        format!("p={pk_hint}"),
    ];
    let mut out = Vec::new();
    for entry in entries {
        let bytes = entry.as_bytes();
        if bytes.len() <= 255 {
            out.push(bytes.len() as u8);
            out.extend_from_slice(bytes);
        }
    }
    out
}

/// Decode DNS-SD length-prefixed TXT bytes into `(version, alias, pk_hint)`.
pub(crate) fn decode_txt(raw: &[u8]) -> (Option<String>, String, String) {
    let mut version = None;
    let mut alias = String::new();
    let mut pk_hint = String::new();
    let mut pos = 0;
    while pos < raw.len() {
        let len = raw[pos] as usize;
        pos += 1;
        if pos + len > raw.len() {
            break;
        }
        if let Ok(text) = std::str::from_utf8(&raw[pos..pos + len])
            && let Some((k, v)) = text.split_once('=')
        {
            match k {
                "v" => version = Some(v.to_string()),
                "n" => alias = sanitize_alias(v),
                "p" => pk_hint = v.chars().take(16).collect(),
                _ => {}
            }
        }
        pos += len;
    }
    if alias.is_empty() {
        alias = "dMsg Device".into();
    }
    (version, alias, pk_hint)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::ffi::{CStr, CString, c_char, c_void};
    use std::ptr::{null, null_mut};
    use std::time::Instant;

    type Ref = *mut c_void;
    type Flags = u32;
    type Code = i32;

    const ADD: Flags = 0x2;
    const POLICY_DENIED: Code = -65570;
    const REPLY: Duration = Duration::from_secs(5);
    const MAX_BROWSED: usize = 32;

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

    fn failed(code: Code) -> Error {
        match code {
            POLICY_DENIED => Error::Unavailable("bonjour: local network access is denied".into()),
            code => Error::Unavailable(format!("bonjour: error {code}")),
        }
    }

    fn checked(code: Code) -> Result<()> {
        if code == 0 { Ok(()) } else { Err(failed(code)) }
    }

    fn cstring(text: &str) -> Result<CString> {
        CString::new(text).map_err(|_| Error::InvalidInput("bonjour name".into()))
    }

    struct Service(Ref);
    unsafe impl Send for Service {}
    impl Drop for Service {
        fn drop(&mut self) {
            unsafe { DNSServiceRefDeallocate(self.0) }
        }
    }

    impl Service {
        fn start(start: impl FnOnce(*mut Ref) -> Code) -> Result<Self> {
            let mut sd = null_mut();
            checked(start(&mut sd))?;
            Ok(Self(sd))
        }

        fn process(&self, deadline: Instant) -> Result<bool> {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(false);
            }
            let mut fd = libc::pollfd {
                fd: unsafe { DNSServiceRefSockFD(self.0) },
                events: libc::POLLIN,
                revents: 0,
            };
            let timeout = i32::try_from(left.as_millis()).unwrap_or(i32::MAX);
            if unsafe { libc::poll(&mut fd, 1, timeout) } > 0 {
                checked(unsafe { DNSServiceProcessResult(self.0) })?;
                return Ok(true);
            }
            Ok(false)
        }
    }

    pub(super) struct Advert(#[expect(dead_code)] Service);

    extern "C" fn on_registered(
        _: Ref,
        _: Flags,
        code: Code,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        ctx: *mut c_void,
    ) {
        unsafe { (*(ctx as *const Cell<Option<Code>>)).set(Some(code)) }
    }

    pub(super) fn advertise(
        instance: &str,
        port: u16,
        alias: &str,
        pk_hint: &str,
    ) -> Result<Advert> {
        let name = cstring(instance)?;
        let kind = cstring(SERVICE)?;
        let txt = encode_txt(alias, pk_hint);
        let reply = Cell::new(None);
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
                txt.len() as u16,
                txt.as_ptr().cast(),
                on_registered,
                (&reply as *const Cell<Option<Code>>).cast_mut().cast(),
            )
        })?;
        let deadline = Instant::now() + REPLY;
        while reply.get().is_none() && Instant::now() < deadline {
            service.process(deadline)?;
        }
        checked(reply.get().unwrap_or(0))?;
        Ok(Advert(service))
    }

    #[derive(Default)]
    struct BrowseAcc {
        error: Option<Code>,
        items: BTreeMap<String, (u32, CString, CString)>,
    }

    extern "C" fn on_browsed(
        _: Ref,
        flags: Flags,
        interface: u32,
        code: Code,
        name: *const c_char,
        kind: *const c_char,
        domain: *const c_char,
        ctx: *mut c_void,
    ) {
        let acc = unsafe { &*(ctx as *const RefCell<BrowseAcc>) };
        let mut st = acc.borrow_mut();
        if code != 0 {
            st.error = Some(code);
            return;
        }
        if name.is_null() || kind.is_null() || domain.is_null() {
            return;
        }
        let name_str = unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned();
        if flags & ADD != 0 {
            if st.items.len() < MAX_BROWSED {
                let kind_c = unsafe { CStr::from_ptr(kind) }.to_owned();
                let domain_c = unsafe { CStr::from_ptr(domain) }.to_owned();
                st.items.insert(name_str, (interface, kind_c, domain_c));
            }
        } else {
            st.items.remove(&name_str);
        }
    }

    #[derive(Default)]
    struct ResolveAcc {
        error: Option<Code>,
        resolved: Option<(String, u16, Vec<u8>)>,
    }

    extern "C" fn on_resolved(
        _: Ref,
        _: Flags,
        _: u32,
        code: Code,
        _: *const c_char,
        host: *const c_char,
        port: u16,
        txt_len: u16,
        txt: *const u8,
        ctx: *mut c_void,
    ) {
        let acc = unsafe { &*(ctx as *const RefCell<ResolveAcc>) };
        let mut st = acc.borrow_mut();
        if code != 0 {
            st.error = Some(code);
            return;
        }
        if host.is_null() {
            return;
        }
        let host_str = unsafe { CStr::from_ptr(host) }
            .to_string_lossy()
            .into_owned();
        let txt_bytes = if !txt.is_null() && txt_len > 0 {
            unsafe { std::slice::from_raw_parts(txt, txt_len as usize) }.to_vec()
        } else {
            Vec::new()
        };
        st.resolved = Some((host_str, u16::from_be(port), txt_bytes));
    }

    pub(super) fn browse(timeout: Duration) -> Result<Vec<DiscoveredPeer>> {
        let kind = cstring(SERVICE)?;
        let acc = RefCell::new(BrowseAcc::default());
        let browsing = Service::start(|sd| unsafe {
            DNSServiceBrowse(
                sd,
                0,
                0,
                kind.as_ptr(),
                null(),
                on_browsed,
                (&acc as *const RefCell<BrowseAcc>).cast_mut().cast(),
            )
        })?;
        let browse_end = Instant::now() + timeout.min(Duration::from_secs(3));
        while Instant::now() < browse_end {
            if let Some(code) = acc.borrow().error {
                return Err(failed(code));
            }
            if !browsing.process(browse_end)? {
                break;
            }
        }
        drop(browsing);
        let found = std::mem::take(&mut acc.borrow_mut().items);
        let mut peers = Vec::new();
        for (name, (iface, kind_c, domain_c)) in found {
            let Ok(name_c) = cstring(&name) else {
                continue;
            };
            let res_acc = RefCell::new(ResolveAcc::default());
            let Ok(resolving) = Service::start(|sd| unsafe {
                DNSServiceResolve(
                    sd,
                    0,
                    iface,
                    name_c.as_ptr(),
                    kind_c.as_ptr(),
                    domain_c.as_ptr(),
                    on_resolved,
                    (&res_acc as *const RefCell<ResolveAcc>).cast_mut().cast(),
                )
            }) else {
                continue;
            };
            let res_end = Instant::now() + Duration::from_secs(2);
            while res_acc.borrow().resolved.is_none()
                && res_acc.borrow().error.is_none()
                && Instant::now() < res_end
            {
                if resolving.process(res_end).is_err() {
                    break;
                }
            }
            if let Some((host, port, txt)) = res_acc.borrow_mut().resolved.take() {
                let (ver, alias, pk_hint) = decode_txt(&txt);
                if ver.as_deref() == Some("1") && port > 0 {
                    peers.push(DiscoveredPeer {
                        advert: name,
                        alias,
                        pk_hint,
                        host: host.trim_end_matches('.').to_string(),
                        port,
                        transport: "lan".into(),
                    });
                }
            }
        }
        Ok(peers)
    }
}

/// Pure-Rust RFC 6762/6763 UDP 5353 mDNS responder & browser used on Linux
/// (avoiding any dependency on `avahi-daemon`) and as fallback on Windows.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) mod udp_mdns {
    use super::*;
    use socket2::{Domain, Protocol, Socket, Type};
    use std::collections::BTreeMap;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::JoinHandle;
    use std::time::Instant;

    const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
    const MDNS_PORT: u16 = 5353;
    const SERVICE_FQDN: &str = "_dmsg-share._tcp.local";
    const TYPE_A: u16 = 1;
    const TYPE_PTR: u16 = 12;
    const TYPE_TXT: u16 = 16;
    const TYPE_SRV: u16 = 33;
    const CLASS_IN: u16 = 1;

    /// Active UDP mDNS responder; dropping stops the responder thread and sends a goodbye.
    pub(crate) struct Advert {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for Advert {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.thread.take() {
                let _ = handle.join();
            }
        }
    }

    fn local_ipv4() -> Option<Ipv4Addr> {
        let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
        s.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
        match s.local_addr().ok()?.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
            _ => None,
        }
    }

    fn multicast_socket() -> Result<UdpSocket> {
        let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| Error::Unavailable(format!("mdns socket: {e}")))?;
        sock.set_reuse_address(true)
            .map_err(|e| Error::Unavailable(format!("mdns reuse: {e}")))?;
        #[cfg(unix)]
        let _ = sock.set_reuse_port(true);
        sock.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MDNS_PORT).into())
            .or_else(|_| sock.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into()))
            .map_err(|e| Error::Unavailable(format!("mdns bind: {e}")))?;
        let _ = sock.join_multicast_v4(&MDNS_ADDR, &Ipv4Addr::UNSPECIFIED);
        let _ = sock.set_multicast_loop_v4(true);
        let udp: UdpSocket = sock.into();
        let _ = udp.set_read_timeout(Some(Duration::from_millis(200)));
        Ok(udp)
    }

    fn write_name(out: &mut Vec<u8>, fqdn: &str) {
        for label in fqdn.trim_end_matches('.').split('.') {
            let b = label.as_bytes();
            if b.is_empty() || b.len() > 63 {
                continue;
            }
            out.push(b.len() as u8);
            out.extend_from_slice(b);
        }
        out.push(0);
    }

    fn read_name(pkt: &[u8], mut pos: usize, depth: u8) -> Option<(String, usize)> {
        if depth > 8 {
            return None;
        }
        let mut labels = Vec::new();
        let end_pos;
        loop {
            let len = *pkt.get(pos)? as usize;
            if len == 0 {
                end_pos = pos + 1;
                break;
            }
            if len & 0xc0 == 0xc0 {
                let b2 = *pkt.get(pos + 1)? as usize;
                let ptr = ((len & 0x3f) << 8) | b2;
                end_pos = pos + 2;
                let (tail, _) = read_name(pkt, ptr, depth + 1)?;
                if !tail.is_empty() {
                    labels.push(tail);
                }
                break;
            }
            pos += 1;
            let slice = pkt.get(pos..pos + len)?;
            labels.push(std::str::from_utf8(slice).ok()?.to_string());
            pos += len;
        }
        Some((labels.join("."), end_pos))
    }

    fn write_rr(out: &mut Vec<u8>, name: &str, rtype: u16, ttl: u32, rdata: &[u8]) {
        write_name(out, name);
        out.extend_from_slice(&rtype.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        out.extend_from_slice(&ttl.to_be_bytes());
        out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        out.extend_from_slice(rdata);
    }

    /// Encode a PTR query packet for `_dmsg-share._tcp.local.`.
    pub(crate) fn encode_ptr_query() -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        // Header: id=0, flags=0, qdcount=1, an=0, ns=0, ar=0
        out.extend_from_slice(&[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        write_name(&mut out, SERVICE_FQDN);
        out.extend_from_slice(&TYPE_PTR.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        out
    }

    /// Encode a complete PTR + SRV + TXT + A mDNS announcement (or TTL=0 goodbye).
    pub(crate) fn encode_announcement(
        instance: &str,
        ip: Ipv4Addr,
        port: u16,
        alias: &str,
        pk_hint: &str,
        ttl: u32,
    ) -> Vec<u8> {
        let inst_fqdn = format!("{instance}.{SERVICE_FQDN}");
        let host_fqdn = format!("dmsg-{instance}.local");
        let mut out = Vec::with_capacity(256);
        // Response header: flags=0x8400 (QR+AA), qd=0, an=4, ns=0, ar=0
        out.extend_from_slice(&[0, 0, 0x84, 0x00, 0, 0, 0, 4, 0, 0, 0, 0]);

        let mut ptr_rdata = Vec::new();
        write_name(&mut ptr_rdata, &inst_fqdn);
        write_rr(&mut out, SERVICE_FQDN, TYPE_PTR, ttl, &ptr_rdata);

        let mut srv_rdata = Vec::new();
        srv_rdata.extend_from_slice(&0u16.to_be_bytes()); // priority
        srv_rdata.extend_from_slice(&0u16.to_be_bytes()); // weight
        srv_rdata.extend_from_slice(&port.to_be_bytes());
        write_name(&mut srv_rdata, &host_fqdn);
        write_rr(&mut out, &inst_fqdn, TYPE_SRV, ttl, &srv_rdata);

        let txt_rdata = encode_txt(alias, pk_hint);
        write_rr(&mut out, &inst_fqdn, TYPE_TXT, ttl, &txt_rdata);

        write_rr(&mut out, &host_fqdn, TYPE_A, ttl, &ip.octets());
        out
    }

    /// Check whether a received DNS packet asks for `_dmsg-share._tcp.local.`.
    pub(crate) fn is_service_query(pkt: &[u8]) -> bool {
        if pkt.len() < 12 || (pkt[2] & 0x80) != 0 {
            return false;
        }
        let qd = u16::from_be_bytes([pkt[4], pkt[5]]);
        let mut pos = 12;
        for _ in 0..qd {
            let Some((name, next)) = read_name(pkt, pos, 0) else {
                return false;
            };
            if next + 4 > pkt.len() {
                return false;
            }
            if name.eq_ignore_ascii_case(SERVICE_FQDN) {
                return true;
            }
            pos = next + 4;
        }
        false
    }

    /// Parse PTR/SRV/TXT/A records from one mDNS response packet into discovered peers.
    pub(crate) fn parse_response(pkt: &[u8]) -> Vec<DiscoveredPeer> {
        if pkt.len() < 12 || (pkt[2] & 0x80) == 0 {
            return Vec::new();
        }
        let qd = u16::from_be_bytes([pkt[4], pkt[5]]);
        let an = u16::from_be_bytes([pkt[6], pkt[7]]);
        let ns = u16::from_be_bytes([pkt[8], pkt[9]]);
        let ar = u16::from_be_bytes([pkt[10], pkt[11]]);
        let mut pos = 12;
        for _ in 0..qd {
            let Some((_, next)) = read_name(pkt, pos, 0) else {
                return Vec::new();
            };
            if next + 4 > pkt.len() {
                return Vec::new();
            }
            pos = next + 4;
        }

        let mut srvs: BTreeMap<String, (String, u16)> = BTreeMap::new();
        let mut txts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut addrs: BTreeMap<String, Ipv4Addr> = BTreeMap::new();

        for _ in 0..(an as usize + ns as usize + ar as usize) {
            let Some((name, next)) = read_name(pkt, pos, 0) else {
                break;
            };
            if next + 10 > pkt.len() {
                break;
            }
            let rtype = u16::from_be_bytes([pkt[next], pkt[next + 1]]);
            let ttl =
                u32::from_be_bytes([pkt[next + 4], pkt[next + 5], pkt[next + 6], pkt[next + 7]]);
            let rdlen = u16::from_be_bytes([pkt[next + 8], pkt[next + 9]]) as usize;
            let rstart = next + 10;
            if rstart + rdlen > pkt.len() {
                break;
            }
            let rdata = &pkt[rstart..rstart + rdlen];
            pos = rstart + rdlen;
            if ttl == 0 {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            match rtype {
                TYPE_SRV if rdlen >= 6 => {
                    let port = u16::from_be_bytes([rdata[4], rdata[5]]);
                    if let Some((target, _)) = read_name(pkt, rstart + 6, 0) {
                        srvs.insert(lower, (target.to_ascii_lowercase(), port));
                    }
                }
                TYPE_TXT => {
                    txts.insert(lower, rdata.to_vec());
                }
                TYPE_A if rdlen == 4 => {
                    addrs.insert(lower, Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]));
                }
                _ => {}
            }
        }

        let suffix = format!(".{SERVICE_FQDN}");
        let mut peers = Vec::new();
        for (inst_fqdn, (target, port)) in srvs {
            let Some(instance) = inst_fqdn.strip_suffix(&suffix) else {
                continue;
            };
            let Some(ip) = addrs.get(&target) else {
                continue;
            };
            let txt = txts.get(&inst_fqdn).map(Vec::as_slice).unwrap_or(&[]);
            let (ver, alias, pk_hint) = decode_txt(txt);
            if ver.as_deref() == Some("1") && port > 0 {
                peers.push(DiscoveredPeer {
                    advert: instance.to_string(),
                    alias,
                    pk_hint,
                    host: ip.to_string(),
                    port,
                    transport: "lan".into(),
                });
            }
        }
        peers
    }

    /// Advertise via UDP multicast DNS until the returned handle is dropped.
    pub(crate) fn advertise(
        instance: &str,
        port: u16,
        alias: &str,
        pk_hint: &str,
    ) -> Result<Advert> {
        let ip = local_ipv4().unwrap_or(Ipv4Addr::LOCALHOST);
        let sock = multicast_socket()?;
        let announce = encode_announcement(instance, ip, port, alias, pk_hint, 120);
        let goodbye = encode_announcement(instance, ip, port, alias, pk_hint, 0);
        let dest = SocketAddr::V4(SocketAddrV4::new(MDNS_ADDR, MDNS_PORT));
        let _ = sock.send_to(&announce, dest);

        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            let mut last_beacon = Instant::now();
            while !flag.load(Ordering::Relaxed) {
                if let Ok((n, _)) = sock.recv_from(&mut buf)
                    && is_service_query(&buf[..n])
                {
                    let _ = sock.send_to(&announce, dest);
                }
                if last_beacon.elapsed() >= Duration::from_secs(15) {
                    let _ = sock.send_to(&announce, dest);
                    last_beacon = Instant::now();
                }
            }
            let _ = sock.send_to(&goodbye, dest);
        });
        Ok(Advert {
            stop,
            thread: Some(thread),
        })
    }

    /// Browse nearby `_dmsg-share._tcp` peers over UDP multicast DNS for `timeout`.
    pub(crate) fn browse(timeout: Duration) -> Result<Vec<DiscoveredPeer>> {
        let sock = multicast_socket()?;
        let dest = SocketAddr::V4(SocketAddrV4::new(MDNS_ADDR, MDNS_PORT));
        let query = encode_ptr_query();
        let _ = sock.send_to(&query, dest);

        let deadline = Instant::now() + timeout.min(Duration::from_secs(3));
        let mut found: BTreeMap<String, DiscoveredPeer> = BTreeMap::new();
        let mut buf = [0u8; 4096];
        let mut resent = false;
        while Instant::now() < deadline {
            if !resent && deadline.saturating_duration_since(Instant::now()) < timeout / 2 {
                let _ = sock.send_to(&query, dest);
                resent = true;
            }
            if let Ok((n, _)) = sock.recv_from(&mut buf) {
                for peer in parse_response(&buf[..n]) {
                    found.insert(peer.advert.clone(), peer);
                }
            }
        }
        Ok(found.into_values().collect())
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::sync::mpsc;
    use windows_sys::Win32::Foundation::DNS_REQUEST_PENDING;
    use windows_sys::Win32::NetworkManagement::Dns::{
        DNS_QUERY_REQUEST_VERSION1, DNS_SERVICE_BROWSE_REQUEST, DNS_SERVICE_CANCEL,
        DNS_SERVICE_INSTANCE, DNS_SERVICE_REGISTER_REQUEST, DnsServiceBrowse,
        DnsServiceBrowseCancel, DnsServiceDeRegister, DnsServiceFreeInstance, DnsServiceRegister,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub(super) enum Advert {
        Native(NativeAdvert),
        Fallback(udp_mdns::Advert),
    }

    pub(super) struct NativeAdvert {
        instance_name: Vec<u16>,
        host_name: Vec<u16>,
        keys: Vec<Vec<u16>>,
        vals: Vec<Vec<u16>>,
        key_ptrs: Vec<*const u16>,
        val_ptrs: Vec<*const u16>,
        port: u16,
    }

    unsafe impl Send for NativeAdvert {}

    unsafe extern "system" fn register_cb(
        status: u32,
        ctx: *const c_void,
        instance: *const DNS_SERVICE_INSTANCE,
    ) {
        if !instance.is_null() {
            unsafe { DnsServiceFreeInstance(instance) };
        }
        if !ctx.is_null() {
            let tx = unsafe { Box::from_raw(ctx as *mut mpsc::Sender<u32>) };
            let _ = tx.send(status);
        }
    }

    unsafe extern "system" fn deregister_cb(
        _status: u32,
        _ctx: *const c_void,
        instance: *const DNS_SERVICE_INSTANCE,
    ) {
        if !instance.is_null() {
            unsafe { DnsServiceFreeInstance(instance) };
        }
    }

    impl Drop for NativeAdvert {
        fn drop(&mut self) {
            let mut inst = DNS_SERVICE_INSTANCE {
                pszInstanceName: self.instance_name.as_mut_ptr(),
                pszHostName: self.host_name.as_mut_ptr(),
                ip4Address: null_mut(),
                ip6Address: null_mut(),
                wPort: self.port,
                wPriority: 0,
                wWeight: 0,
                dwPropertyCount: self.key_ptrs.len() as u32,
                keys: self.key_ptrs.as_mut_ptr() as *mut *mut u16,
                values: self.val_ptrs.as_mut_ptr() as *mut *mut u16,
                dwInterfaceIndex: 0,
            };
            let req = DNS_SERVICE_REGISTER_REQUEST {
                Version: DNS_QUERY_REQUEST_VERSION1,
                InterfaceIndex: 0,
                pServiceInstance: &mut inst,
                pRegisterCompletionCallback: Some(deregister_cb),
                pQueryContext: null_mut(),
                hCredentials: null_mut(),
                unicastEnabled: 0,
            };
            unsafe {
                let _ = DnsServiceDeRegister(&req, null_mut());
            }
        }
    }

    fn try_native_advertise(
        instance: &str,
        port: u16,
        alias: &str,
        pk_hint: &str,
    ) -> Result<NativeAdvert> {
        let mut instance_name = wide(&format!("{instance}._dmsg-share._tcp.local"));
        let mut host_name = wide(&format!("dmsg-{instance}.local"));
        let keys = vec![wide("v"), wide("n"), wide("p")];
        let vals = vec![wide("1"), wide(&sanitize_alias(alias)), wide(pk_hint)];
        let mut key_ptrs: Vec<*const u16> = keys.iter().map(|k| k.as_ptr()).collect();
        let mut val_ptrs: Vec<*const u16> = vals.iter().map(|v| v.as_ptr()).collect();
        let mut inst = DNS_SERVICE_INSTANCE {
            pszInstanceName: instance_name.as_mut_ptr(),
            pszHostName: host_name.as_mut_ptr(),
            ip4Address: null_mut(),
            ip6Address: null_mut(),
            wPort: port,
            wPriority: 0,
            wWeight: 0,
            dwPropertyCount: key_ptrs.len() as u32,
            keys: key_ptrs.as_mut_ptr() as *mut *mut u16,
            values: val_ptrs.as_mut_ptr() as *mut *mut u16,
            dwInterfaceIndex: 0,
        };
        let (tx, rx) = mpsc::channel();
        let ctx = Box::into_raw(Box::new(tx)) as *mut c_void;
        let req = DNS_SERVICE_REGISTER_REQUEST {
            Version: DNS_QUERY_REQUEST_VERSION1,
            InterfaceIndex: 0,
            pServiceInstance: &mut inst,
            pRegisterCompletionCallback: Some(register_cb),
            pQueryContext: ctx,
            hCredentials: null_mut(),
            unicastEnabled: 0,
        };
        let mut cancel = DNS_SERVICE_CANCEL {
            reserved: null_mut(),
        };
        let code = unsafe { DnsServiceRegister(&req, &mut cancel) };
        if code != DNS_REQUEST_PENDING as u32 {
            let _ = unsafe { Box::from_raw(ctx as *mut mpsc::Sender<u32>) };
            return Err(Error::Unavailable(format!("win dns-sd register: {code}")));
        }
        match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(0) => Ok(NativeAdvert {
                instance_name,
                host_name,
                keys,
                vals,
                key_ptrs,
                val_ptrs,
                port,
            }),
            _ => Err(Error::Unavailable("win dns-sd register failed".into())),
        }
    }

    pub(super) fn advertise(
        instance: &str,
        port: u16,
        alias: &str,
        pk_hint: &str,
    ) -> Result<Advert> {
        match try_native_advertise(instance, port, alias, pk_hint) {
            Ok(native) => Ok(Advert::Native(native)),
            Err(_) => Ok(Advert::Fallback(udp_mdns::advertise(
                instance, port, alias, pk_hint,
            )?)),
        }
    }

    pub(super) fn browse(timeout: Duration) -> Result<Vec<DiscoveredPeer>> {
        // Use UDP multicast browse directly (captures both native Windows DNS-SD
        // responders and Apple/Android/Linux mDNS peers on UDP 5353 cleanly).
        let _ = (
            DNS_SERVICE_BROWSE_REQUEST {
                Version: DNS_QUERY_REQUEST_VERSION1,
                InterfaceIndex: 0,
                QueryName: null(),
                Anonymous: unsafe { std::mem::zeroed() },
                pQueryContext: null_mut(),
            },
            DnsServiceBrowse as usize,
            DnsServiceBrowseCancel as usize,
        );
        udp_mdns::browse(timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn txt_round_trip_preserves_alias_and_pk_hint() {
        let raw = encode_txt("Alice's Mac", "AbCdEf");
        let (v, alias, hint) = decode_txt(&raw);
        assert_eq!(v.as_deref(), Some("1"));
        assert_eq!(alias, "Alice's Mac");
        assert_eq!(hint, "AbCdEf");
    }

    #[test]
    fn udp_mdns_codec_encodes_and_parses_service_announcements() {
        let query = udp_mdns::encode_ptr_query();
        assert!(udp_mdns::is_service_query(&query));

        let pkt = udp_mdns::encode_announcement(
            "0123456789abcdef",
            Ipv4Addr::new(192, 168, 1, 77),
            45678,
            "Linux Box",
            "XyZ123",
            120,
        );
        assert!(!udp_mdns::is_service_query(&pkt));
        let peers = udp_mdns::parse_response(&pkt);
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].advert, "0123456789abcdef");
        assert_eq!(peers[0].alias, "Linux Box");
        assert_eq!(peers[0].pk_hint, "XyZ123");
        assert_eq!(peers[0].host, "192.168.1.77");
        assert_eq!(peers[0].port, 45678);
    }
}
