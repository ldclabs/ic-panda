//! Owner-only Unix socket transport. No HTTP/MCP route can ask for signatures.
use crate::daemon::Daemon;
use dmsg_client::secrets::ssh::{self, Peer};
use dmsg_types::{Error, Result, ensure};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

pub(crate) struct Socket {
    path: PathBuf,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Socket {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) fn start(d: Arc<Daemon>, home: &Path) -> Result<Socket> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let meta = std::fs::symlink_metadata(home).map_err(|_| Error::Forbidden)?;
    ensure(
        meta.is_dir() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o077 == 0,
        Error::Forbidden,
    )?;
    let path = home.join(format!(
        "ssh-{}.sock",
        &hex::encode(dmsg_client::crypto::random_id())[..16]
    ));
    use std::os::unix::ffi::OsStrExt;
    ensure(
        path.as_os_str().as_bytes().len()
            < std::mem::size_of::<libc::sockaddr_un>()
                - std::mem::offset_of!(libc::sockaddr_un, sun_path),
        Error::Unavailable("SSH_SOCKET_PATH_TOO_LONG: choose a shorter dMsg home".into()),
    )?;
    let listener =
        UnixListener::bind(&path).map_err(|_| Error::Unavailable("SSH socket".into()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| Error::Forbidden)?;
    *d.ssh_path.lock().expect("SSH socket") = Some(path.clone());
    *d.ssh_error.lock().expect("SSH status") = None;
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted=listener.accept()=>{
                    let Ok((socket,_))=accepted else{break};
                    if connections.len()>=32 {drop(socket);continue;}
                    let daemon=d.clone();connections.spawn(async move{serve(daemon,socket).await;});
                }
                _=connections.join_next(),if !connections.is_empty()=>{}
            }
        }
    });
    Ok(Socket { path, task })
}

async fn send(socket: &mut UnixStream, data: &[u8]) -> Result<()> {
    socket
        .write_u32(data.len() as u32)
        .await
        .map_err(|_| Error::Unavailable("SSH disconnected".into()))?;
    socket
        .write_all(data)
        .await
        .map_err(|_| Error::Unavailable("SSH disconnected".into()))
}

async fn frame(socket: &mut UnixStream) -> Result<Vec<u8>> {
    let size = socket.read_u32().await.map_err(|_| Error::NotFound)? as usize;
    ensure((1..=ssh::MAX_FRAME).contains(&size), Error::QuotaExceeded)?;
    let mut bytes = vec![0; size];
    socket
        .read_exact(&mut bytes)
        .await
        .map_err(|_| Error::NotFound)?;
    Ok(bytes)
}

async fn serve(d: Arc<Daemon>, mut socket: UnixStream) {
    let Ok(peer) = socket.peer_cred() else { return };
    if peer.uid() != unsafe { libc::geteuid() } {
        return;
    }
    let mut connection = None;
    loop {
        let input = match tokio::time::timeout(Duration::from_secs(300), frame(&mut socket)).await {
            Ok(Ok(v)) => v,
            _ => break,
        };
        // Identity enumeration while locked is empty and never prompts for unlock.
        if input == [11] {
            let result = d.client().lock().await.secret_ssh_identities();
            let mut response = vec![12];
            match result {
                Ok(keys) => {
                    response.extend_from_slice(&(keys.len() as u32).to_be_bytes());
                    for (key, title) in keys {
                        ssh::put(&mut response, &key);
                        ssh::put(&mut response, title.as_bytes());
                    }
                }
                Err(_) => {
                    response = vec![5];
                }
            }
            if send(&mut socket, &response).await.is_err() {
                break;
            }
            continue;
        }
        if connection.is_none() {
            let opened = {
                d.client().lock().await.secret_ssh_open(Peer {
                    uid: peer.uid(),
                    pid: peer.pid().and_then(|v| u32::try_from(v).ok()),
                })
            };
            match opened {
                Ok(id) => connection = Some(id),
                Err(_) => {
                    let _ = send(&mut socket, &[5]).await;
                    break;
                }
            }
        }
        let id = connection.as_deref().expect("connection");
        let (outcome, close) = match input[0] {
            27 => {
                let mut name = &input[1..];
                if ssh::take(&mut name).is_ok_and(|n| n == b"session-bind@openssh.com") {
                    // An invalid, forwarded or repeated binding ends the connection.
                    let bound = d.client().lock().await.secret_ssh_bind(id, &input[1..]);
                    let close = bound.is_err();
                    (bound.map(|_| vec![6]), close)
                } else {
                    // Other extensions are unsupported, which is no reason to disconnect.
                    (Err(Error::UnsupportedProtocol), false)
                }
            }
            13 => match sign(&d, &mut socket, id, &input[1..]).await {
                Ok(Some(response)) => (Ok(response), false),
                // The peer left or sent more while waiting; its framing is gone.
                Ok(None) => break,
                Err(error) => (Err(error), false),
            },
            _ => (Err(Error::Forbidden), false),
        };
        if send(&mut socket, &outcome.unwrap_or_else(|_| vec![5]))
            .await
            .is_err()
        {
            break;
        }
        if close {
            break;
        }
    }
    if let Some(id) = connection {
        d.client().lock().await.secret_ssh_close(&id);
        d.emit("approval/changed", Value::Null);
    }
}

/// A signature response, or `None` once the peer has gone or broken framing.
async fn sign(
    d: &Daemon,
    socket: &mut UnixStream,
    connection: &str,
    frame: &[u8],
) -> Result<Option<Vec<u8>>> {
    let mut input = frame;
    let public = ssh::take(&mut input)?;
    let bytes = ssh::take(&mut input)?;
    ensure(input == [0, 0, 0, 0], Error::Forbidden)?; // Ed25519 has no flags.
    // Subscribed before the request exists, so its decision cannot be missed.
    let mut events = d.subscribe_events();
    let request = d
        .client()
        .lock()
        .await
        .secret_ssh_request(connection, public, bytes)?;
    let id = request["id"].as_str().ok_or(Error::IntegrityFailed)?;
    d.emit("approval/changed", Value::Null);
    loop {
        let state = d.client().lock().await.approval(id)?;
        match state["state"].as_str() {
            Some("executing") => {
                let signature = d
                    .client()
                    .lock()
                    .await
                    .secret_ssh_sign(id, connection, bytes)?;
                let mut response = vec![14];
                ssh::put(&mut response, &signature);
                d.emit("approval/changed", Value::Null);
                return Ok(Some(response));
            }
            Some("awaiting_user") => {}
            _ => return Err(Error::Forbidden),
        }
        // A decision is announced; expiry is not, so recheck now and then.
        // EOF or pipelined input invalidates the connection before a late approval.
        let mut byte = [0];
        tokio::select! {
            _ = events.recv() => {}
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            _ = socket.read(&mut byte) => {
                d.client().lock().await.secret_ssh_close(connection);
                d.emit("approval/changed", Value::Null);
                return Ok(None);
            }
        }
    }
}
