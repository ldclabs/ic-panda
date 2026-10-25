//! Where an operation finds the client. A caller that owns the client holds
//! it for the whole operation ([`Client::hold`]); the daemon shares one
//! client between every request and takes it for each step alone, so the
//! others go on while the network answers.
//!
//! An operation written against [`Hold`] reaches the client only in
//! synchronous steps ([`Hold::with`]), which cannot wait on the network.
//! Relay requests are signed in such a step and sent after it
//! ([`Hold::send`]); the IC's and the user homes' agents leave the client
//! the same way. What a step reads may change before the next one, since
//! other requests run in between: each step checks what it builds on, as
//! the journals and cursors do.

use crate::account::Security;
use crate::client::{Client, Warm};
use crate::cloud::{Cloud, Prepared, found};
use crate::evidence::Trust;
use crate::ic::Ic;
use dmsg_types::{AccountId, Error, Hash, Result, ensure};
use futures_util::future::try_join_all;
use serde::Serialize;
use serde_json::Value as Json;
use std::future::Future;
use std::sync::{Mutex, PoisonError};

/// Chunks downloaded side by side.
const DOWNLOADS: usize = 8;

/// Where an operation finds the client; see the module.
pub trait Hold: Sync {
    /// Run `step` on the client.
    fn with<T: Send>(
        &self,
        step: impl FnOnce(&mut Client) -> Result<T> + Send,
    ) -> impl Future<Output = Result<T>> + Send;

    /// Bring the client to what `warm` asks for: the certified security and
    /// the user homes are read without the client ([`Client::warmup`]).
    /// Returns the security the relay holds, or the user home's answer.
    fn warm(&self, warm: Warm) -> impl Future<Output = Result<Option<Security>>> + Send {
        async move {
            let Some(warmup) = self.with(|c| c.warmup(warm)).await? else {
                return Ok(None);
            };
            let warmed = Box::pin(warmup.run()).await?;
            self.with(|c| c.warmed(warmed)).await
        }
    }

    /// What `prepare` builds with relay evidence fresh enough to sign with,
    /// and the relay client to send it without the client. Built again with
    /// newer evidence once, when the evidence went stale meanwhile.
    fn signed<T: Send>(
        &self,
        mut prepare: impl FnMut(&mut Client) -> Result<T> + Send,
    ) -> impl Future<Output = Result<(Cloud, T)>> + Send {
        async move {
            let mut stale = false;
            loop {
                if let Some(warmup) = self
                    .with(|c| c.warmup_for(Some(Warm::Relay), false))
                    .await?
                {
                    let warmed = Box::pin(warmup.run()).await?;
                    self.with(|c| c.warmed(warmed)).await?;
                }
                match self.with(|c| Ok((c.relay(), prepare(c)))).await? {
                    (_, Err(Error::PolicyStale)) if !stale => stale = true,
                    (relay, prepared) => return Ok((relay, prepared?)),
                }
            }
        }
    }

    /// A relay request `sign` signs under the client, answered without it.
    fn send(
        &self,
        sign: impl FnMut(&mut Client) -> Result<Prepared> + Send,
    ) -> impl Future<Output = Result<Json>> + Send {
        async move {
            let (relay, request) = self.signed(sign).await?;
            relay.send(request).await
        }
    }

    /// A device-authenticated GET; `None` for an absent resource.
    fn find(&self, path: &str) -> impl Future<Output = Result<Option<Json>>> + Send {
        async move { found(self.send(|c| c.signed_get(path)).await) }
    }

    /// A device-authenticated GET.
    fn get(&self, path: &str) -> impl Future<Output = Result<Json>> + Send {
        async move { self.send(|c| c.signed_get(path)).await }
    }

    /// A signed device command.
    fn post<P: Serialize + Sync>(
        &self,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
    ) -> impl Future<Output = Result<Json>> + Send {
        async move {
            self.send(|c| c.signed_post(path, action, payload, request_id))
                .await
        }
    }

    /// Download chunks by `(path, digest)`, a few side by side, each group
    /// signed with fresh evidence and checked against its digests; `keep`
    /// takes each group under the client.
    fn chunks(
        &self,
        parts: &[(String, String)],
        mut keep: impl FnMut(&mut Client, &[(String, String)], Vec<Vec<u8>>) -> Result<()> + Send,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            for group in parts.chunks(DOWNLOADS) {
                let (relay, requests) = self
                    .signed(|c| {
                        group
                            .iter()
                            .map(|(path, digest)| Ok((c.signed_get(path)?, digest.as_str())))
                            .collect::<Result<Vec<_>>>()
                    })
                    .await?;
                let relay = &relay;
                let fetched = try_join_all(
                    requests
                        .into_iter()
                        .map(|(request, digest)| relay.send_chunk(request, digest)),
                )
                .await?;
                self.with(|c| keep(c, group, fetched)).await?;
            }
            Ok(())
        }
    }

    /// One chunk or manifest, checked against its digest.
    fn chunk(&self, path: &str, digest: &str) -> impl Future<Output = Result<Vec<u8>>> + Send {
        async move {
            let (relay, request) = self.signed(|c| c.signed_get(path)).await?;
            relay.send_chunk(request, digest).await
        }
    }

    /// What other accounts' relay evidence is checked against, with the user
    /// homes read without the client the first time.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the user homes cannot be read.
    fn trust(&self) -> impl Future<Output = Result<Trust>> + Send {
        async move {
            if let Some(trust) = self.with(|c| c.trust_now()).await? {
                return Ok(trust);
            }
            if let Some(warmup) = self.with(|c| c.warmup_for(None, true)).await? {
                let warmed = Box::pin(warmup.run()).await?;
                self.with(|c| c.warmed(warmed)).await?;
            }
            self.with(|c| c.trust_now())
                .await?
                .ok_or_else(|| Error::Unavailable("the user homes".into()))
        }
    }

    /// An anonymous agent, for canisters outside the login's delegation
    /// targets, made without the client the first time.
    fn anonymous(&self) -> impl Future<Output = Result<Ic>> + Send {
        async move {
            let (made, config) = self
                .with(|c| Ok((c.anonymous.clone(), c.config.clone())))
                .await?;
            if let Some(ic) = made {
                return Ok(ic);
            }
            let ic = Ic::anonymous(&config).await?;
            self.with(|c| Ok(c.anonymous.get_or_insert(ic).clone()))
                .await
        }
    }

    /// The login's agent for public queries, else an anonymous one.
    fn reader(&self) -> impl Future<Output = Result<Ic>> + Send {
        async move {
            match self
                .with(|c| Ok(c.session().ok().map(|l| l.ic.clone())))
                .await?
            {
                Some(ic) => Ok(ic),
                None => self.anonymous().await,
            }
        }
    }
}

/// A shared client bound to the current unlocked account and session.
/// Locking (even followed by unlock) prevents subsequent local steps.
pub struct Unlocked<'a, H> {
    inner: &'a H,
    pub(crate) account: AccountId,
    generation: u64,
}

impl<'a, H: Hold> Unlocked<'a, H> {
    /// Bind the unlocked session before a network call or resource turn.
    /// # Errors
    /// Returns `Error::Locked` without content keys or `Error::AuthRequired` without an account.
    pub async fn new(inner: &'a H) -> Result<Self> {
        let (account, generation) = inner
            .with(|c| {
                c.keys()?;
                Ok((c.account()?.id, c.session_generation))
            })
            .await?;
        Ok(Self {
            inner,
            account,
            generation,
        })
    }
}

impl<H: Hold> Hold for Unlocked<'_, H> {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        self.inner
            .with(|c| {
                c.keys()?;
                ensure(
                    c.account()?.id == self.account && c.session_generation == self.generation,
                    Error::Locked,
                )?;
                step(c)
            })
            .await
    }
}

/// The client held by its one caller for a whole operation.
pub struct Owned<'a>(Mutex<&'a mut Client>);

/// An owned client that yields between local steps, so a host can cancel a
/// large stream even when its file reads and SQLite steps are immediately ready.
pub struct Cooperative<'a>(Owned<'a>);

impl Client {
    /// This client, held by the caller throughout an operation written
    /// against [`Hold`].
    pub fn hold(&mut self) -> Owned<'_> {
        Owned(Mutex::new(self))
    }

    /// The same ownership as [`Self::hold`], with cooperative cancellation
    /// points between synchronous steps on the host's Tokio executor.
    pub fn cooperative_hold(&mut self) -> Cooperative<'_> {
        Cooperative(self.hold())
    }
}

impl Hold for Cooperative<'_> {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        let value = self.0.with(step).await;
        tokio::task::yield_now().await;
        value
    }
}

impl Hold for Owned<'_> {
    fn with<T: Send>(
        &self,
        step: impl FnOnce(&mut Client) -> Result<T> + Send,
    ) -> impl Future<Output = Result<T>> + Send {
        let mut client = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        std::future::ready(step(&mut client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_unlocked_operation_cannot_resume_after_lock_and_unlock() {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let shared = c.hold();
        let operation = Unlocked::new(&shared).await.unwrap();
        shared
            .with(|c| {
                c.lock();
                c.keys = Some(c.workspace.unlock(&crate::testing::SECRET)?);
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(operation.with(|_| Ok(())).await, Err(Error::Locked));
    }
}
