//! Device transfers from and to this computer (dmsg-device-transfer/1).
//! Receiving opens a listener on a random port for its session alone,
//! advertised with Bonjour; sending connects to the session Bonjour finds or
//! to the address the person typed. One transfer runs at a time: its
//! progress goes out as `transfer/changed`, it counts as activity while it
//! moves, and it stops once the device locks. A transfer for a succession
//! settles the content step on the device it ran on.

use crate::bonjour;
use crate::daemon::Daemon;
use crate::params::{hash, optional, parse, source, text};
use dmsg_client::hold::{Hold, Unlocked};
use dmsg_client::transfer::{self, Category, Certified, Report, SessionId, Start, Ticket};
use dmsg_client::{error_code, succession};
use dmsg_types::{Error, Result, ensure};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

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
        "transfer/send_start" => &["transfer/changed", "approval/changed"],
        _ => &["transfer/changed"],
    }
}

/// This daemon's transfer: the running one, or the last one with its
/// outcome.
#[derive(Default)]
pub(crate) struct Transfers {
    host: OnceLock<Weak<Daemon>>,
    slot: transfer::Transfers,
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

    pub(crate) fn view(&self) -> Value {
        self.slot.view()
    }

    /// Start `start` with the task `run` makes, unless a transfer is
    /// running.
    fn begin(&self, start: Start, run: impl FnOnce(Job) -> JoinHandle<()>) -> Result<Value> {
        let daemon = self
            .host
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| Error::Unavailable("the daemon is stopping".into()))?;
        let (role, advert) = (start.role, transfer::advert(&start.session));
        let view = self
            .slot
            .begin(start, |ticket| run(Job { daemon, ticket }))?;
        crate::logs::write("transfer/start", &[("role", &role), ("advert", &advert)]);
        Ok(view)
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
                    let now = self.slot.report(id);
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
        let code = result.as_ref().err().map(error_code).unwrap_or_default();
        if !self.slot.end(id, result) {
            return;
        }
        let view = self.view();
        let state = view["state"].as_str().unwrap_or_default();
        crate::logs::write("transfer/end", &[("state", &state), ("error", &code)]);
        daemon.emit("transfer/changed", view);
    }

    /// Stop the running transfer; its listener and advertisement close.
    fn cancel(&self) -> Value {
        if self.slot.cancel() {
            crate::logs::write("transfer/end", &[("state", &"cancelled")]);
        }
        self.view()
    }
}

/// What a transfer's task carries.
struct Job {
    daemon: Arc<Daemon>,
    ticket: Ticket,
}

impl Job {
    /// Drive `work` and settle the succession's content step after it.
    async fn settle<H: Hold>(
        &self,
        h: &H,
        work: impl Future<Output = Result<Report>>,
    ) -> Result<Report> {
        let transfers = &self.daemon.transfers;
        let work = transfers.drive(&self.daemon, self.ticket.id, h, work);
        self.ticket.settle(h, work).await
    }

    fn end(&self, result: Result<Report>) {
        self.daemon
            .transfers
            .end(&self.daemon, self.ticket.id, result);
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
        let ticket = &job.ticket;
        let work = transfer::serve(
            &h,
            &devices,
            &listener,
            ticket.session,
            &categories,
            &ticket.report,
        );
        job.settle(&h, work).await
    }
    .await;
    drop((advert, listener));
    job.end(result);
}

/// The sending side.
async fn sending(job: Job, permit: transfer::Permit, address: Option<String>) {
    let shared = job.daemon.hold();
    let result = async {
        let h = Unlocked::new(&shared).await?;
        let ticket = &job.ticket;
        let stream = connect(&ticket.session, address).await?;
        let devices = Certified(&h);
        let work = transfer::send_approved(&h, &devices, stream, permit, &ticket.report);
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
        let me = h.with(|c| c.device_id()).await?;
        if let Some(task) = task {
            succession::peer(&h, task).await?;
        }
        let listener = transfer::listen().await?;
        let port = listener.local_addr().map_err(io)?.port();
        let name = transfer::advert(&session);
        let advertised = tokio::task::spawn_blocking(move || bonjour::advertise(&name, port))
            .await
            .map_err(|e| Error::Unavailable(format!("bonjour: {e}")))?;
        let (advert, bonjour) = match advertised {
            Ok(advert) => (Some(advert), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let start = Start {
            role: "receive",
            session,
            to: me,
            succession: task,
            port: Some(port),
            addresses: transfer::local_address(port).into_iter().collect(),
            bonjour,
        };
        self.transfers.begin(start, |job| {
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
        ensure(
            !matches!(
                self.transfers.view()["state"].as_str(),
                Some("waiting" | "running")
            ),
            Error::Pending,
        )?;
        let Some(approval) = optional(params, "approval", text)? else {
            return Ok(
                json!({"approval": transfer::request(&h, source(params)?, to, session).await?}),
            );
        };
        let permit = transfer::claim(&h, &approval, to, session).await?;
        let start = Start {
            role: "send",
            session,
            to,
            succession: task,
            port: None,
            addresses: address.iter().cloned().collect(),
            bonjour: None,
        };
        self.transfers
            .begin(start, |job| tokio::spawn(sending(job, permit, address)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::Home;
    use dmsg_client::testing::{TempDir, unlocked};
    use serde_json::json;
    use std::net::Ipv4Addr;

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
