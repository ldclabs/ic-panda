//! Device transfers on the phone (dmsg-device-transfer/1). The core runs the
//! listener or the connection; the app advertises the session with Bonjour
//! or finds it, and passes the receiver's host and port. A transfer stops
//! once the app leaves the foreground or locks, and the app starts it again
//! with the same session, which resumes.

use crate::error::Result;
use crate::session::ScopedHost;
use crate::{MobileEngine, ObjectSnapshot, runtime};
use dmsg_client::hold::Hold;
use dmsg_client::succession;
use dmsg_client::transfer::{self, Category, Certified, SessionId, Start, Ticket, Transfers};
use dmsg_types::{Error, Hash, ensure};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};

/// How long a sender waits for the receiver to answer.
const CONNECT: Duration = Duration::from_secs(15);

fn io(e: std::io::Error) -> Error {
    Error::Unavailable(format!("transfer: {e}"))
}

fn session_id(text: &str) -> Result<SessionId> {
    Ok(hex::decode(text)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| dmsg_types::invalid("session"))?)
}

fn hash(text: &str, name: &str) -> Result<Hash> {
    Ok(hex::decode(text)
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b).ok())
        .map(Hash::new)
        .ok_or_else(|| dmsg_types::invalid(name))?)
}

/// The Bonjour name receiving session `session` is advertised under, which
/// a sender looks for.
#[uniffi::export]
pub fn transfer_advert(session: String) -> Result<String> {
    Ok(transfer::advert(&session_id(&session)?))
}

/// The receiving side, while the session it started in lasts.
async fn receiving(
    host: ScopedHost,
    transfers: Arc<Transfers>,
    ticket: Ticket,
    listener: TcpListener,
) {
    let work = async {
        let devices = Certified(&host);
        let work = transfer::serve(
            &host,
            &devices,
            &listener,
            ticket.session,
            &Category::ALL,
            &ticket.report,
        );
        ticket.settle(&host, work).await
    };
    let result = host.inner.while_current(host.generation(), work).await;
    drop(listener);
    transfers.end(ticket.id, result);
}

/// The sending side, while the session it started in lasts.
async fn sending(
    host: ScopedHost,
    transfers: Arc<Transfers>,
    ticket: Ticket,
    to: Hash,
    address: (String, u16),
) {
    let work = async {
        let connecting = TcpStream::connect((address.0.as_str(), address.1));
        let stream = tokio::time::timeout(CONNECT, connecting)
            .await
            .map_err(|_| Error::Unavailable("transfer: the other device did not answer".into()))?
            .map_err(io)?;
        let devices = Certified(&host);
        let work = transfer::send(&host, &devices, stream, to, ticket.session, &ticket.report);
        ticket.settle(&host, work).await
    };
    let result = host.inner.while_current(host.generation(), work).await;
    transfers.end(ticket.id, result);
}

#[uniffi::export]
impl MobileEngine {
    /// Open a session that receives the file library, or reopen `session`
    /// to resume it, for `succession` when given. The view names the port
    /// and the Bonjour name (`advert`) the app advertises.
    pub async fn transfer_receive_start(
        &self,
        session: Option<String>,
        succession: Option<String>,
    ) -> Result<ObjectSnapshot> {
        let session = match session {
            Some(session) => session_id(&session)?,
            None => transfer::session_id(),
        };
        let task = succession.map(|id| hash(&id, "succession")).transpose()?;
        let transfers = self.transfers.clone();
        let generation = self.host.generation();
        let view = self
            .held(move |h| {
                Box::pin(async move {
                    let me = h.with(|c| c.device_id()).await?;
                    if let Some(task) = task {
                        succession::peer(h, task).await?;
                    }
                    let listener = transfer::listen().await?;
                    let port = listener.local_addr().map_err(io)?.port();
                    let start = Start {
                        role: "receive",
                        session,
                        to: me,
                        succession: task,
                        port: Some(port),
                        addresses: transfer::local_address(port).into_iter().collect(),
                        bonjour: None,
                    };
                    let host = h.clone();
                    let slot = transfers.clone();
                    slot.begin(start, |ticket| {
                        runtime().spawn(receiving(host, transfers, ticket, listener))
                    })
                })
            })
            .await?;
        Ok(ObjectSnapshot {
            generation,
            json: view.to_string(),
        })
    }

    /// Send the file library to receiving session `session` at `host` and
    /// `port`: to device `to`, or the other device of `succession`.
    pub async fn transfer_send_start(
        &self,
        session: String,
        to: Option<String>,
        host: String,
        port: u16,
        succession: Option<String>,
    ) -> Result<ObjectSnapshot> {
        let session = session_id(&session)?;
        let named = to.map(|to| hash(&to, "to")).transpose()?;
        let task = succession.map(|id| hash(&id, "succession")).transpose()?;
        let transfers = self.transfers.clone();
        let generation = self.host.generation();
        let view = self
            .held(move |h| {
                Box::pin(async move {
                    let to = match (named, task) {
                        (named, Some(task)) => {
                            let peer = succession::peer(h, task).await?;
                            ensure(
                                named.is_none_or(|to| to == peer),
                                Error::InvalidInput("to: not the device of the succession".into()),
                            )?;
                            peer
                        }
                        (Some(to), None) => to,
                        (None, None) => return Err(Error::InvalidInput("missing to".into())),
                    };
                    let start = Start {
                        role: "send",
                        session,
                        to,
                        succession: task,
                        port: None,
                        addresses: vec![format!("{host}:{port}")],
                        bonjour: None,
                    };
                    let scoped = h.clone();
                    let slot = transfers.clone();
                    slot.begin(start, |ticket| {
                        let address = (host, port);
                        runtime().spawn(sending(scoped, transfers, ticket, to, address))
                    })
                })
            })
            .await?;
        Ok(ObjectSnapshot {
            generation,
            json: view.to_string(),
        })
    }

    /// The running transfer, or the last one with its outcome, or null.
    pub fn transfer_status(&self) -> ObjectSnapshot {
        ObjectSnapshot {
            generation: self.host.generation(),
            json: self.transfers.view().to_string(),
        }
    }

    /// Stop the running transfer: its listener or connection closes.
    pub fn transfer_cancel(&self) -> ObjectSnapshot {
        self.transfers.cancel();
        self.transfer_status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_client::host::Host;
    use std::net::Ipv4Addr;

    async fn open(port: u16) -> bool {
        TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn a_receiving_session_ends_once_the_app_leaves_the_foreground() {
        let dir = dmsg_client::testing::TempDir::new();
        let client = dmsg_client::testing::unlocked(&dir.0, false, None);
        let host = Host::start(client, &tokio::runtime::Handle::current());
        let scoped = ScopedHost::new(host.clone(), host.generation());
        let transfers = Arc::new(Transfers::default());
        let listener = transfer::listen().await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let start = Start {
            role: "receive",
            session: transfer::session_id(),
            to: Hash::new([1; 32]),
            succession: None,
            port: Some(port),
            addresses: vec![],
            bonjour: None,
        };
        let slot = transfers.clone();
        let view = slot
            .begin(start, |ticket| {
                tokio::spawn(receiving(scoped, transfers, ticket, listener))
            })
            .unwrap();
        assert_eq!(view["state"], "waiting");
        assert!(open(port).await);
        host.pause();
        for _ in 0..100 {
            if slot.view()["state"] == "failed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(slot.view()["error"]["code"], "LOCKED");
        assert!(!open(port).await);
        host.close();
        host.closed().await.unwrap();
    }
}
