//! How the daemon's requests share its one client. An operation takes the
//! client for each of its steps alone ([`Shared`], a [`Hold`]), so the
//! others go on while it waits on the network. What still waits on the
//! network holding the client goes through [`Daemon::under_lock`], the one
//! way to: `clippy.toml` refuses the client's guard across any other
//! `.await`. Operations that change the same thing, such as one channel's
//! upload or the backup pass, take turns on it ([`Daemon::turn`]).

use crate::daemon::Daemon;
use dmsg_client::{Client, hold::Hold};
use dmsg_types::{Error, Result, ensure};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, OwnedMutexGuard};

/// The daemon's client, taken for each step of one operation alone.
pub(crate) struct Shared<'a> {
    client: &'a Mutex<Client>,
    /// The account selected when the operation began.
    account: OnceLock<Option<String>>,
}

impl Hold for Shared<'_> {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        let mut client = self.client.lock().await;
        // An operation goes on only for the account it began with.
        let account = client.session_state().account;
        ensure(
            *self.account.get_or_init(|| account.clone()) == account,
            Error::Unavailable("the account changed".into()),
        )?;
        step(&mut client)
    }
}

/// The resources operations take turns on, by name.
#[derive(Default)]
pub(crate) struct Turns(std::sync::Mutex<BTreeMap<String, Arc<Mutex<()>>>>);

impl Daemon {
    /// The client for one operation written against [`Hold`].
    pub(crate) fn hold(&self) -> Shared<'_> {
        Shared {
            client: self.client(),
            account: OnceLock::new(),
        }
    }

    /// Run `work` holding the client throughout, its network waits
    /// included: for what does not wait without it yet. Every other request
    /// waits meanwhile.
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "the one way to wait on the network holding the client"
    )]
    pub(crate) async fn under_lock<T>(
        &self,
        work: impl AsyncFnOnce(&mut Client) -> Result<T>,
    ) -> Result<T> {
        let mut client = self.client().lock().await;
        work(&mut client).await
    }

    /// Wait for the right to run one operation on `resource`.
    pub(crate) async fn turn(&self, resource: &str) -> OwnedMutexGuard<()> {
        let lock = {
            let mut turns = self.turns().0.lock().expect("turns");
            // A resource nobody holds or waits for is forgotten.
            turns.retain(|_, lock| Arc::strong_count(lock) > 1);
            turns.entry(resource.to_string()).or_default().clone()
        };
        lock.lock_owned().await
    }
}
