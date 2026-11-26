//! Rust FFI bridge to `swift/NearbyAWDL.swift` (`Network.framework` with
//! `includePeerToPeer = true`) on macOS. Other platforms compile as a no-op.

use dmsg_client::nearby::{BridgeTransport, DiscoveredPeer};
use dmsg_types::{Error, Result};
use std::time::Duration;

#[cfg(target_os = "macos")]
pub(crate) use macos::{AwdlListener, browse, connect, listen};

#[cfg(not(target_os = "macos"))]
pub(crate) struct AwdlListener;

#[cfg(not(target_os = "macos"))]
pub(crate) fn listen(
    _instance: &str,
    _alias: &str,
    _pk_hint: &str,
    _on_accept: std::sync::Arc<dyn Fn(BridgeTransport) + Send + Sync>,
) -> Result<AwdlListener> {
    Err(Error::Unavailable("awdl unavailable on this OS".into()))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn connect(_instance: &str) -> Result<BridgeTransport> {
    Err(Error::Unavailable("awdl unavailable on this OS".into()))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn browse(_timeout: Duration) -> Result<Vec<DiscoveredPeer>> {
    Ok(Vec::new())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::{CStr, CString, c_char, c_void};
    use std::sync::{Arc, Mutex, OnceLock};
    use tokio::sync::mpsc;

    unsafe extern "C" {
        fn dmsg_awdl_listen_start(
            instance: *const c_char,
            alias: *const c_char,
            pk_hint: *const c_char,
            on_accept: *const c_void,
            on_frame: *const c_void,
            ctx: *mut c_void,
        ) -> u64;
        fn dmsg_awdl_listen_stop(listener_id: u64);
        fn dmsg_awdl_connect(
            instance: *const c_char,
            on_frame: *const c_void,
            ctx: *mut c_void,
        ) -> u64;
        fn dmsg_awdl_conn_send(conn_id: u64, data: *const u8, length: isize) -> i32;
        fn dmsg_awdl_conn_close(conn_id: u64);
        fn dmsg_awdl_browse(timeout_ms: u32, on_peer: *const c_void, ctx: *mut c_void) -> i32;
    }

    type AcceptHook = Arc<dyn Fn(BridgeTransport) + Send + Sync>;

    struct AcceptEntry {
        hook: AcceptHook,
        runtime: tokio::runtime::Handle,
    }

    struct Registry {
        inbound: HashMap<u64, mpsc::Sender<Vec<u8>>>,
        accept_hooks: HashMap<usize, AcceptEntry>,
    }

    fn registry() -> &'static Mutex<Registry> {
        static REG: OnceLock<Mutex<Registry>> = OnceLock::new();
        REG.get_or_init(|| {
            Mutex::new(Registry {
                inbound: HashMap::new(),
                accept_hooks: HashMap::new(),
            })
        })
    }

    fn wire_connection(conn_id: u64, runtime: &tokio::runtime::Handle) -> BridgeTransport {
        let (transport, mut handle) = BridgeTransport::pair(16);
        {
            let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
            reg.inbound.insert(conn_id, handle.inbound);
        }
        runtime.spawn(async move {
            while let Some(frame) = handle.outbound.recv().await {
                let rc =
                    unsafe { dmsg_awdl_conn_send(conn_id, frame.as_ptr(), frame.len() as isize) };
                if rc != 0 {
                    break;
                }
            }
            unsafe { dmsg_awdl_conn_close(conn_id) };
            let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
            reg.inbound.remove(&conn_id);
        });
        transport
    }

    extern "C" fn on_accept(conn_id: u64, ctx: *mut c_void) {
        let key = ctx as usize;
        let entry = {
            let reg = registry().lock().unwrap_or_else(|e| e.into_inner());
            reg.accept_hooks
                .get(&key)
                .map(|e| (e.hook.clone(), e.runtime.clone()))
        };
        if let Some((hook, runtime)) = entry {
            let transport = wire_connection(conn_id, &runtime);
            runtime.spawn(async move {
                hook(transport);
            });
        } else {
            unsafe { dmsg_awdl_conn_close(conn_id) };
        }
    }

    extern "C" fn on_frame(conn_id: u64, ptr: *const u8, len: isize, _ctx: *mut c_void) {
        if ptr.is_null() || len <= 0 {
            let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
            reg.inbound.remove(&conn_id);
            return;
        }
        let data = unsafe { std::slice::from_raw_parts(ptr, len as usize) }.to_vec();
        let tx = {
            let reg = registry().lock().unwrap_or_else(|e| e.into_inner());
            reg.inbound.get(&conn_id).cloned()
        };
        if let Some(tx) = tx {
            let _ = tx.try_send(data);
        }
    }

    /// Active macOS AWDL listener; dropping stops listening and releases its callback slot.
    pub(crate) struct AwdlListener {
        id: u64,
        hook_key: usize,
    }

    impl Drop for AwdlListener {
        fn drop(&mut self) {
            unsafe { dmsg_awdl_listen_stop(self.id) };
            let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
            reg.accept_hooks.remove(&self.hook_key);
        }
    }

    pub(crate) fn listen(
        instance: &str,
        alias: &str,
        pk_hint: &str,
        on_accept_cb: AcceptHook,
    ) -> Result<AwdlListener> {
        let inst_c = CString::new(instance).map_err(|_| Error::InvalidInput("awdl name".into()))?;
        let alias_c = CString::new(alias).map_err(|_| Error::InvalidInput("awdl alias".into()))?;
        let hint_c = CString::new(pk_hint).map_err(|_| Error::InvalidInput("awdl hint".into()))?;
        let runtime = tokio::runtime::Handle::current();
        let hook_key = Arc::as_ptr(&on_accept_cb) as *const () as usize;
        {
            let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
            reg.accept_hooks.insert(
                hook_key,
                AcceptEntry {
                    hook: on_accept_cb,
                    runtime,
                },
            );
        }
        let id = unsafe {
            dmsg_awdl_listen_start(
                inst_c.as_ptr(),
                alias_c.as_ptr(),
                hint_c.as_ptr(),
                on_accept as *const c_void,
                on_frame as *const c_void,
                hook_key as *mut c_void,
            )
        };
        if id == 0 {
            let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
            reg.accept_hooks.remove(&hook_key);
            return Err(Error::Unavailable("awdl listener failed".into()));
        }
        Ok(AwdlListener { id, hook_key })
    }

    pub(crate) fn connect(instance: &str) -> Result<BridgeTransport> {
        let inst_c = CString::new(instance).map_err(|_| Error::InvalidInput("awdl name".into()))?;
        let conn_id = unsafe {
            dmsg_awdl_connect(
                inst_c.as_ptr(),
                on_frame as *const c_void,
                std::ptr::null_mut(),
            )
        };
        if conn_id == 0 {
            return Err(Error::Unavailable("awdl connect failed".into()));
        }
        Ok(wire_connection(conn_id, &tokio::runtime::Handle::current()))
    }

    extern "C" fn on_peer(
        name: *const c_char,
        alias: *const c_char,
        hint: *const c_char,
        ctx: *mut c_void,
    ) {
        if name.is_null() || ctx.is_null() {
            return;
        }
        let peers = unsafe { &mut *(ctx as *mut Vec<DiscoveredPeer>) };
        let advert = unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned();
        let alias = if alias.is_null() {
            "dMsg Device".into()
        } else {
            unsafe { CStr::from_ptr(alias) }
                .to_string_lossy()
                .into_owned()
        };
        let pk_hint = if hint.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(hint) }
                .to_string_lossy()
                .into_owned()
        };
        peers.push(DiscoveredPeer {
            advert,
            alias,
            pk_hint,
            host: String::new(),
            port: 0,
            transport: "awdl".into(),
        });
    }

    pub(crate) fn browse(timeout: Duration) -> Result<Vec<DiscoveredPeer>> {
        let mut peers = Vec::<DiscoveredPeer>::new();
        let ms = timeout.as_millis().clamp(200, 3000) as u32;
        unsafe {
            dmsg_awdl_browse(
                ms,
                on_peer as *const c_void,
                (&mut peers as *mut Vec<DiscoveredPeer>).cast(),
            );
        }
        Ok(peers)
    }
}
