//! The desktop's one active relay invitation, scoped to its unlocked session.

use crate::daemon::Daemon;
use dmsg_client::pairing::Receiver;
use dmsg_types::{Error, Result};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(crate) struct Pairing {
    slot: Mutex<Slot>,
}

#[derive(Default)]
struct Slot {
    revision: u64,
    receiver: Option<Arc<Receiver>>,
}

impl Pairing {
    pub(crate) async fn start(&self, daemon: &Daemon) -> Result<Value> {
        let (revision, previous) = {
            let mut slot = self.slot.lock().unwrap();
            slot.revision += 1;
            (slot.revision, slot.receiver.take())
        };
        if let Some(previous) = previous {
            let _ = previous.cancel(&daemon.hold()).await;
        }
        let receiver = Arc::new(Receiver::new(&daemon.hold()).await?);
        let view = receiver.view()?;
        {
            let mut slot = self.slot.lock().unwrap();
            if slot.revision == revision {
                slot.receiver = Some(receiver);
                return Ok(view);
            }
        }
        let _ = receiver.cancel(&daemon.hold()).await;
        Err(Error::VersionConflict)
    }

    pub(crate) async fn status(&self, daemon: &Daemon, id: &str) -> Result<Value> {
        let receiver = self
            .slot
            .lock()
            .unwrap()
            .receiver
            .as_ref()
            .filter(|receiver| receiver.id() == id)
            .cloned()
            .ok_or(Error::NotFound)?;
        let result = receiver.status(&daemon.hold()).await;
        // A reply must still belong to the currently displayed invitation.
        if !self
            .slot
            .lock()
            .unwrap()
            .receiver
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &receiver))
        {
            return Err(Error::NotFound);
        }
        result
    }

    pub(crate) async fn cancel(&self, daemon: &Daemon, id: &str) -> Result<()> {
        let receiver = {
            let mut slot = self.slot.lock().unwrap();
            if slot
                .receiver
                .as_ref()
                .is_some_and(|receiver| receiver.id() == id)
            {
                slot.revision += 1;
                slot.receiver.take()
            } else {
                None
            }
        };
        if let Some(receiver) = receiver {
            receiver.cancel(&daemon.hold()).await?;
        }
        Ok(())
    }
}
