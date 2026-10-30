//! A host's transfers, one at a time: the running one, or the last one with
//! its outcome, in the view both apps show. The host opens the listener
//! ([`listen`]) or the connection and runs the transfer in a task of its
//! own, which ends it here.

use super::{Report, SessionId, advert};
use crate::hold::Hold;
use crate::{error_code, now_ms};
use dmsg_types::{Error, Hash, Result, ensure};
use serde_json::{Value as Json, json};
use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::net::TcpListener;
use tokio::task::{AbortHandle, JoinHandle};

/// A transfer to start.
pub struct Start {
    /// `receive` or `send`.
    pub role: &'static str,
    /// The receiving session.
    pub session: SessionId,
    /// The receiving device.
    pub to: Hash,
    /// The succession whose content step it settles.
    pub succession: Option<Hash>,
    /// Where the receiver listens, to type in where Bonjour cannot help.
    pub port: Option<u16>,
    /// The receiver's addresses with the port, or the one the sender typed.
    pub addresses: Vec<String>,
    /// Why Bonjour could not advertise the session.
    pub bonjour: Option<String>,
}

/// What a started transfer's task carries.
pub struct Ticket {
    /// Tells this transfer from a later one.
    pub id: u64,
    /// The receiving session.
    pub session: SessionId,
    /// How far it got, which the view shows.
    pub report: Arc<Mutex<Report>>,
    succession: Option<Hash>,
}

impl Ticket {
    /// Await `work`, then settle the succession's content step with its
    /// report.
    ///
    /// # Errors
    /// As `work`, and as [`crate::succession::transferred`].
    pub async fn settle<H: Hold>(
        &self,
        h: &H,
        work: impl Future<Output = Result<Report>>,
    ) -> Result<Report> {
        let done = work.await?;
        if let Some(task) = self.succession {
            crate::succession::transferred(h, task, &done).await?;
        }
        Ok(done)
    }
}

struct Current {
    id: u64,
    start: Start,
    report: Arc<Mutex<Report>>,
    /// `running`, then `done`, `failed` or `cancelled`.
    state: &'static str,
    error: Option<Error>,
    started_at: u64,
    ended_at: Option<u64>,
    task: Option<AbortHandle>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Current {
    fn view(&self) -> Json {
        let report = lock(&self.report).clone();
        let state = match self.state {
            "running" if report.peer.is_none() => "waiting",
            state => state,
        };
        let start = &self.start;
        json!({
            "role": start.role,
            "session": hex::encode(start.session),
            "advert": advert(&start.session),
            "to": hex::encode(*start.to),
            "succession": start.succession.map(|id| hex::encode(*id)),
            "port": start.port,
            "addresses": start.addresses,
            "bonjour": start.bonjour,
            "state": state,
            "error": self.error.as_ref().map(|e| json!({"code": error_code(e), "message": e.to_string()})),
            "report": report,
            "started_at": self.started_at,
            "ended_at": self.ended_at,
        })
    }
}

/// A host's transfer: the running one, or the last one with its outcome.
#[derive(Default)]
pub struct Transfers {
    started: AtomicU64,
    current: Mutex<Option<Current>>,
}

impl Transfers {
    /// The running or last transfer, or null.
    pub fn view(&self) -> Json {
        lock(&self.current)
            .as_ref()
            .map_or(Json::Null, Current::view)
    }

    /// Start `start` with the task `run` makes, unless a transfer is
    /// running.
    ///
    /// # Errors
    /// Returns `Error::Pending` while a transfer runs.
    pub fn begin(&self, start: Start, run: impl FnOnce(Ticket) -> JoinHandle<()>) -> Result<Json> {
        let mut slot = lock(&self.current);
        ensure(
            slot.as_ref().is_none_or(|c| c.state != "running"),
            Error::Pending,
        )?;
        let id = self.started.fetch_add(1, Ordering::Relaxed) + 1;
        let report = Arc::<Mutex<Report>>::default();
        let ticket = Ticket {
            id,
            session: start.session,
            report: report.clone(),
            succession: start.succession,
        };
        // The task waits for the slot before it reports.
        let task = run(ticket).abort_handle();
        let current = Current {
            id,
            start,
            report,
            state: "running",
            error: None,
            started_at: now_ms(),
            ended_at: None,
            task: Some(task),
        };
        let view = current.view();
        *slot = Some(current);
        Ok(view)
    }

    /// The report of transfer `id` while it is the latest.
    pub fn report(&self, id: u64) -> Option<Report> {
        lock(&self.current)
            .as_ref()
            .filter(|c| c.id == id)
            .map(|c| lock(&c.report).clone())
    }

    /// Record how transfer `id` ended, unless it was cancelled; whether it
    /// did.
    pub fn end(&self, id: u64, result: Result<Report>) -> bool {
        let mut slot = lock(&self.current);
        let Some(current) = slot.as_mut().filter(|c| c.id == id && c.state == "running") else {
            return false;
        };
        current.state = if result.is_ok() { "done" } else { "failed" };
        current.error = result.err();
        current.ended_at = Some(now_ms());
        current.task = None;
        true
    }

    /// Stop the running transfer, whose task ends with the listener or
    /// connection it holds; whether one was running.
    pub fn cancel(&self) -> bool {
        let mut slot = lock(&self.current);
        let Some(current) = slot.as_mut().filter(|c| c.state == "running") else {
            return false;
        };
        if let Some(task) = current.task.take() {
            task.abort();
        }
        current.state = "cancelled";
        current.ended_at = Some(now_ms());
        true
    }
}

fn io(e: std::io::Error) -> Error {
    Error::Unavailable(format!("transfer: {e}"))
}

/// A listener on a random port of every interface: IPv6 and IPv4 together
/// where the system allows, IPv4 otherwise.
///
/// # Errors
/// Returns `Error::Unavailable` when no port opens.
pub async fn listen() -> Result<TcpListener> {
    match TcpListener::bind((Ipv6Addr::UNSPECIFIED, 0)).await {
        Ok(listener) => Ok(listener),
        Err(_) => TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
            .await
            .map_err(io),
    }
}

/// This device's address with `port` on the network its default route
/// leaves by, to type in on the sender.
pub fn local_address(port: u16) -> Option<String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // Connecting a UDP socket only picks the route; nothing is sent.
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then(|| SocketAddr::new(ip, port).to_string())
}
