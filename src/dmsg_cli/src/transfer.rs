//! Device transfers from and to this computer (dmsg-device-transfer/1).
//! Receiving opens a listener on a random port for its session alone,
//! advertised with Bonjour; sending connects to the session Bonjour finds or
//! to the address the person typed. One transfer runs at a time: its
//! progress goes out as `transfer/changed`, it counts as activity while it
//! moves, and it stops once the device locks. A transfer for a succession
//! settles the content step on the device it ran on.

use crate::bonjour;
use crate::daemon::Daemon;
use crate::params::{hash, optional, parse, text};
use dmsg_client::hold::{Hold, Unlocked};
use dmsg_client::transfer::{self, Category, Certified, Report, SessionId};
use dmsg_client::{error_code, now_ms, succession};
use dmsg_types::{Error, Hash, Result, ensure};
use serde_json::{Value, json};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};

/// The methods this module serves.
pub(crate) const METHODS: [&str; 4] = [
    "transfer/receive_start",
    "transfer/send_start",
    "transfer/status",
    "transfer/cancel",
];

/// How long a sender looks for the session and connects.
const CONNECT: Duration = Duration::from_secs(15);
/// How often a running transfer reports and checks the lock.
const TICK: Duration = Duration::from_secs(1);

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "transfer/status" => &[],
        _ => &["transfer/changed"],
    }
}

/// This daemon's transfer: the running one, or the last one with its
/// outcome.
#[derive(Default)]
pub(crate) struct Transfers {
    host: OnceLock<Weak<Daemon>>,
    started: AtomicU64,
    current: Mutex<Option<Current>>,
}

struct Current {
    /// Tells this transfer's task from a later one's.
    id: u64,
    /// `receive` or `send`.
    role: &'static str,
    session: SessionId,
    /// The receiving device, in hex.
    to: String,
    succession: Option<Hash>,
    /// Where the receiver listens, to type in where Bonjour cannot help.
    port: Option<u16>,
    addresses: Vec<String>,
    /// Why Bonjour could not advertise the session.
    bonjour: Option<String>,
    report: Arc<Mutex<Report>>,
    /// `running`, then `done`, `failed` or `cancelled`.
    state: &'static str,
    error: Option<Error>,
    started_at: u64,
    ended_at: Option<u64>,
    task: Option<tokio::task::AbortHandle>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Current {
    fn view(&self) -> Value {
        let report = lock(&self.report).clone();
        let state = match self.state {
            "running" if report.peer.is_none() => "waiting",
            state => state,
        };
        json!({
            "role": self.role,
            "session": hex::encode(self.session),
            "advert": transfer::advert(&self.session),
            "to": self.to,
            "succession": self.succession.map(|id| hex::encode(*id)),
            "port": self.port,
            "addresses": self.addresses,
            "bonjour": self.bonjour,
            "state": state,
            "error": self.error.as_ref().map(|e| json!({"code": error_code(e), "message": e.to_string()})),
            "report": report,
            "started_at": self.started_at,
            "ended_at": self.ended_at,
        })
    }
}

/// A session ID in hex.
fn session(params: &Value, name: &str) -> Result<SessionId> {
    hex::decode(text(params, name)?)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| Error::InvalidInput(format!("{name}: 16 bytes in hex")))
}

fn io(e: std::io::Error) -> Error {
    Error::Unavailable(format!("transfer: {e}"))
}

/// A listener on a random port of every interface: IPv6 and IPv4 together
/// where the system allows, IPv4 otherwise.
async fn listen() -> Result<TcpListener> {
    match TcpListener::bind((Ipv6Addr::UNSPECIFIED, 0)).await {
        Ok(listener) => Ok(listener),
        Err(_) => TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
            .await
            .map_err(io),
    }
}

/// This computer's address on the network its default route leaves by,
/// to type in on the sender.
fn local_address(port: u16) -> Option<String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // Connecting a UDP socket only picks the route; nothing is sent.
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then(|| SocketAddr::new(ip, port).to_string())
}

/// The receiver of session `id`, at `address` or found with Bonjour.
async fn connect(id: &SessionId, address: Option<String>) -> Result<TcpStream> {
    let connecting = async {
        match address {
            Some(address) => TcpStream::connect(address.as_str()).await.map_err(io),
            None => {
                let name = transfer::advert(id);
                let (host, port) =
                    tokio::task::spawn_blocking(move || bonjour::find(&name, CONNECT))
                        .await
                        .map_err(|e| Error::Unavailable(format!("bonjour: {e}")))??;
                TcpStream::connect((host.as_str(), port)).await.map_err(io)
            }
        }
    };
    tokio::time::timeout(CONNECT * 2, connecting)
        .await
        .map_err(|_| Error::Unavailable("transfer: the other device did not answer".into()))?
}

impl Transfers {
    /// Let transfers run tasks of their own on `host`.
    pub(crate) fn bind(&self, host: Weak<Daemon>) {
        let _ = self.host.set(host);
    }

    fn view(&self) -> Value {
        lock(&self.current)
            .as_ref()
            .map_or(Value::Null, Current::view)
    }

    /// Start `current` with the task `run` makes, unless a transfer is
    /// running.
    fn begin(
        &self,
        mut current: Current,
        run: impl FnOnce(Job) -> tokio::task::JoinHandle<()>,
    ) -> Result<Value> {
        let daemon = self
            .host
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| Error::Unavailable("the daemon is stopping".into()))?;
        let mut slot = lock(&self.current);
        ensure(
            slot.as_ref().is_none_or(|c| c.state != "running"),
            Error::Pending,
        )?;
        current.id = self.started.fetch_add(1, Ordering::Relaxed) + 1;
        let job = Job {
            daemon,
            id: current.id,
            session: current.session,
            task: current.succession,
            report: current.report.clone(),
        };
        // The task waits for the slot before it reports.
        current.task = Some(run(job).abort_handle());
        crate::logs::write(
            "transfer/start",
            &[
                ("role", &current.role),
                ("advert", &transfer::advert(&current.session)),
            ],
        );
        let view = current.view();
        *slot = Some(current);
        Ok(view)
    }

    fn report(&self, id: u64) -> Option<Report> {
        lock(&self.current)
            .as_ref()
            .filter(|c| c.id == id)
            .map(|c| lock(&c.report).clone())
    }

    /// Run `work`, announcing its progress each tick and noting it as
    /// activity while it moves; it stops once the device locks.
    async fn drive<H: Hold>(
        &self,
        daemon: &Daemon,
        id: u64,
        h: &H,
        work: impl Future<Output = Result<Report>>,
    ) -> Result<Report> {
        tokio::pin!(work);
        let mut tick = tokio::time::interval(TICK);
        let mut last = None;
        loop {
            tokio::select! {
                result = &mut work => return result,
                _ = tick.tick() => {
                    h.with(|_| Ok(())).await?;
                    let now = self.report(id);
                    if now != last {
                        if last.is_some() {
                            daemon.note_activity();
                        }
                        last = now;
                        daemon.emit("transfer/changed", self.view());
                    }
                }
            }
        }
    }

    /// Record how transfer `id` ended, unless it was cancelled.
    fn end(&self, daemon: &Daemon, id: u64, result: Result<Report>) {
        {
            let mut slot = lock(&self.current);
            let Some(current) = slot.as_mut().filter(|c| c.id == id && c.state == "running") else {
                return;
            };
            current.state = if result.is_ok() { "done" } else { "failed" };
            current.error = result.err();
            current.ended_at = Some(now_ms());
            current.task = None;
            let code = current.error.as_ref().map(error_code).unwrap_or_default();
            crate::logs::write(
                "transfer/end",
                &[("state", &current.state), ("error", &code)],
            );
        }
        daemon.emit("transfer/changed", self.view());
    }

    /// Stop the running transfer; its listener and advertisement close.
    fn cancel(&self) -> Value {
        let mut slot = lock(&self.current);
        if let Some(current) = slot.as_mut().filter(|c| c.state == "running") {
            if let Some(task) = current.task.take() {
                task.abort();
            }
            current.state = "cancelled";
            current.ended_at = Some(now_ms());
            crate::logs::write("transfer/end", &[("state", &current.state)]);
        }
        slot.as_ref().map_or(Value::Null, Current::view)
    }
}

/// What a transfer's task carries.
struct Job {
    daemon: Arc<Daemon>,
    /// The transfer's ID among this daemon's.
    id: u64,
    session: SessionId,
    /// The succession whose content step it settles.
    task: Option<Hash>,
    report: Arc<Mutex<Report>>,
}

impl Job {
    /// Drive `work` and settle the succession's content step after it.
    async fn settle<H: Hold>(
        &self,
        h: &H,
        work: impl Future<Output = Result<Report>>,
    ) -> Result<Report> {
        let done = self
            .daemon
            .transfers
            .drive(&self.daemon, self.id, h, work)
            .await?;
        if let Some(task) = self.task {
            succession::transferred(h, task, &done).await?;
        }
        Ok(done)
    }

    fn end(&self, result: Result<Report>) {
        self.daemon.transfers.end(&self.daemon, self.id, result);
    }
}

/// The receiving side, until a sender's transfer completes.
async fn receiving(
    job: Job,
    listener: TcpListener,
    advert: Option<bonjour::Advert>,
    categories: Vec<Category>,
) {
    let shared = job.daemon.hold();
    let result = async {
        let h = Unlocked::new(&shared).await?;
        let devices = Certified(&h);
        let work = transfer::serve(
            &h,
            &devices,
            &listener,
            job.session,
            &categories,
            &job.report,
        );
        job.settle(&h, work).await
    }
    .await;
    drop((advert, listener));
    job.end(result);
}

/// The sending side.
async fn sending(job: Job, to: Hash, address: Option<String>) {
    let shared = job.daemon.hold();
    let result = async {
        let h = Unlocked::new(&shared).await?;
        let stream = connect(&job.session, address).await?;
        let devices = Certified(&h);
        let work = transfer::send(&h, &devices, stream, to, job.session, &job.report);
        job.settle(&h, work).await
    }
    .await;
    job.end(result);
}

impl Daemon {
    /// Run one of [`METHODS`].
    pub(crate) async fn call_transfer(&self, method: &str, params: &Value) -> Result<Value> {
        match method {
            "transfer/receive_start" => self.receive_start(params).await,
            "transfer/send_start" => self.send_start(params).await,
            "transfer/status" => Ok(self.transfers.view()),
            "transfer/cancel" => Ok(self.transfers.cancel()),
            _ => Err(Error::NotFound),
        }
    }

    /// Open a receiving session, or reopen `session` to resume it.
    async fn receive_start(&self, params: &Value) -> Result<Value> {
        let session = optional(params, "session", session)?.unwrap_or_else(transfer::session_id);
        let categories: Vec<Category> =
            optional(params, "categories", parse)?.unwrap_or(Category::ALL.to_vec());
        ensure(
            !categories.is_empty(),
            Error::InvalidInput("categories".into()),
        )?;
        let task = optional(params, "succession", hash)?;
        let shared = self.hold();
        let h = Unlocked::new(&shared).await?;
        let me = h
            .with(|c| c.status()?.device_id.ok_or(Error::Locked))
            .await?;
        if let Some(task) = task {
            succession::peer(&h, task).await?;
        }
        let listener = listen().await?;
        let port = listener.local_addr().map_err(io)?.port();
        let name = transfer::advert(&session);
        let advertised = tokio::task::spawn_blocking(move || bonjour::advertise(&name, port))
            .await
            .map_err(|e| Error::Unavailable(format!("bonjour: {e}")))?;
        let (advert, bonjour) = match advertised {
            Ok(advert) => (Some(advert), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let current = Current {
            id: 0,
            role: "receive",
            session,
            to: me,
            succession: task,
            port: Some(port),
            addresses: local_address(port).into_iter().collect(),
            bonjour,
            report: Arc::default(),
            state: "running",
            error: None,
            started_at: now_ms(),
            ended_at: None,
            task: None,
        };
        self.transfers.begin(current, |job| {
            tokio::spawn(receiving(job, listener, advert, categories))
        })
    }

    /// Send this device's content to the receiving session `session`.
    async fn send_start(&self, params: &Value) -> Result<Value> {
        let session = session(params, "session")?;
        let task = optional(params, "succession", hash)?;
        let named = optional(params, "to", hash)?;
        let address = optional(params, "address", text)?;
        let shared = self.hold();
        let h = Unlocked::new(&shared).await?;
        let to = match (named, task) {
            (named, Some(task)) => {
                let peer = succession::peer(&h, task).await?;
                ensure(
                    named.is_none_or(|to| to == peer),
                    Error::InvalidInput("to: not the device of the succession".into()),
                )?;
                peer
            }
            (Some(to), None) => to,
            (None, None) => return Err(Error::InvalidInput("missing to".into())),
        };
        let current = Current {
            id: 0,
            role: "send",
            session,
            to: hex::encode(*to),
            succession: task,
            port: None,
            addresses: address.iter().cloned().collect(),
            bonjour: None,
            report: Arc::default(),
            state: "running",
            error: None,
            started_at: now_ms(),
            ended_at: None,
            task: None,
        };
        self.transfers
            .begin(current, |job| tokio::spawn(sending(job, to, address)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::Home;
    use dmsg_client::testing::{TempDir, unlocked};

    fn daemon(dir: &TempDir) -> Arc<Daemon> {
        let home = Home {
            dir: dir.0.join("home"),
        };
        let daemon = Arc::new(Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            unlocked(&dir.0.join("workspace"), false, None),
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        ));
        daemon.transfers.bind(Arc::downgrade(&daemon));
        daemon
    }

    /// Whether the port takes connections, within a second.
    async fn open(port: u16) -> bool {
        for _ in 0..50 {
            if TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .await
                .is_err()
            {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        true
    }

    async fn state(daemon: &Daemon, want: &str) -> Value {
        for _ in 0..100 {
            let view = daemon.transfers.view();
            if view["state"] == want {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("{want}: {}", daemon.transfers.view());
    }

    #[tokio::test]
    async fn a_receiving_session_listens_until_cancelled_or_locked() {
        let dir = TempDir::new();
        let daemon = daemon(&dir);
        assert_eq!(
            daemon.call_transfer("transfer/status", &Value::Null).await,
            Ok(Value::Null)
        );
        let view = daemon
            .call_transfer("transfer/receive_start", &Value::Null)
            .await
            .unwrap();
        assert_eq!(view["state"], "waiting");
        let port = view["port"].as_u64().unwrap() as u16;
        // A connection that says nothing fails alone; the session listens.
        let mut silent = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::write_all(&mut silent, &[0xff; 4])
            .await
            .unwrap();
        while daemon.transfers.view()["report"]["last_error"].is_null() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(open(port).await);
        // One transfer at a time.
        assert_eq!(
            daemon
                .call_transfer("transfer/receive_start", &Value::Null)
                .await,
            Err(Error::Pending)
        );
        let cancelled = daemon
            .call_transfer("transfer/cancel", &Value::Null)
            .await
            .unwrap();
        assert_eq!(cancelled["state"], "cancelled");
        assert!(!open(port).await);

        // The same session opens again, and locking closes it.
        let params = json!({"session": view["session"]});
        let again = daemon
            .call_transfer("transfer/receive_start", &params)
            .await
            .unwrap();
        assert_eq!(again["session"], view["session"]);
        let port = again["port"].as_u64().unwrap() as u16;
        daemon.client().lock().await.lock();
        let failed = state(&daemon, "failed").await;
        assert_eq!(failed["error"]["code"], "LOCKED");
        assert!(!open(port).await);
        assert_eq!(
            daemon
                .call_transfer("transfer/receive_start", &Value::Null)
                .await,
            Err(Error::Locked)
        );
    }

    #[tokio::test]
    async fn a_sender_needs_the_receiving_device() {
        let dir = TempDir::new();
        let daemon = daemon(&dir);
        let params = json!({"session": "00".repeat(16)});
        assert!(matches!(
            daemon.call_transfer("transfer/send_start", &params).await,
            Err(Error::InvalidInput(_))
        ));
        let params = json!({"session": "00".repeat(8), "to": "11".repeat(32)});
        assert!(matches!(
            daemon.call_transfer("transfer/send_start", &params).await,
            Err(Error::InvalidInput(_))
        ));
    }
}
