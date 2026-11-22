//! An administrator's one active relay invitation, scoped to its unlocked session.

use super::Receiver;
use crate::hold::{Hold, Unlocked};
use dmsg_types::{Error, Result};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// One invitation per host, fenced across cancellation and replacement.
#[derive(Default)]
pub struct InvitationHost {
    slot: Mutex<Slot>,
}

#[derive(Default)]
struct Slot {
    revision: u64,
    receiver: Option<Arc<Receiver>>,
}

impl InvitationHost {
    /// Replace the active invitation, preflighting the administrator before issuing an account QR.
    /// # Errors
    /// Propagates session, protocol and relay errors.
    pub async fn start<H: Hold>(&self, h: &H, with_login: bool) -> Result<Value> {
        let (revision, previous) = {
            let mut slot = self.slot.lock().unwrap();
            slot.revision += 1;
            (slot.revision, slot.receiver.take())
        };
        if let Some(previous) = previous {
            let _ = previous.cancel(h).await;
        }
        let session = Unlocked::new(h).await?;
        if with_login {
            let state = crate::account::refreshed(&session).await?;
            let route = session.with(|c| c.invitation_route(&state)).await?;
            if route["route"] != "approve" {
                return Ok(route);
            }
        }
        let receiver = Arc::new(Receiver::register(&session, with_login).await?);
        let view = receiver.view()?;
        {
            let mut slot = self.slot.lock().unwrap();
            if slot.revision == revision {
                slot.receiver = Some(receiver);
                return Ok(view);
            }
        }
        let _ = receiver.cancel(h).await;
        Err(Error::VersionConflict)
    }

    /// Read the current invitation.
    /// # Errors
    /// Propagates session, protocol and relay errors.
    pub async fn status<H: Hold>(&self, h: &H, id: &str) -> Result<Value> {
        let receiver = self
            .slot
            .lock()
            .unwrap()
            .receiver
            .as_ref()
            .filter(|receiver| receiver.id() == id)
            .cloned()
            .ok_or(Error::NotFound)?;
        let result = receiver.status(h).await;
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

    /// Discard this invitation and cancel its mailboxes.
    /// # Errors
    /// Propagates session, protocol and relay errors.
    pub async fn cancel<H: Hold>(&self, h: &H, id: &str) -> Result<()> {
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
            receiver.cancel(h).await?;
        }
        Ok(())
    }
}
