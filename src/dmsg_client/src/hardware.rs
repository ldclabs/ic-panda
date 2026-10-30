//! The hardware anchor: every workspace has a key the hardware running it
//! made ([`HardwareAnchor`]), and signs with it before it unlocks. A copy
//! of the workspace on other hardware, such as one a system migration or a
//! restored backup made, cannot sign: it is moved and reads only. It never
//! unlocks, so the old device's identity never acts from two places, and
//! nothing in it is erased. Setting it aside keeps the copy under
//! `moved/` for import and makes this hardware a new device of the same
//! account.
//!
//! A workspace that already held hardware keys when its anchor was made
//! may have come from other hardware before the anchor existed: the first
//! of those keys that opens settles it, and one this hardware cannot use
//! moves it.

use crate::client::{Client, Slot, Status};
use crate::crypto::random_id;
use crate::now_ms;
use crate::secure::HardwareAnchor;
use crate::storage;
use crate::store::Store;
use crate::workspace::{Anchor, META, Meta, Workspace};
use dmsg_protocol::digest;
use dmsg_types::{AccountId, Error, Hash, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Copies set aside, each in a directory of its own.
pub(crate) const MOVED: &str = "moved";
/// What a copy set aside says about itself.
const MOVED_RECORD: &str = "moved.json";

/// What the anchor says about a workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// It runs on the hardware that made its anchor, or on hardware that
    /// makes none.
    Native,
    /// It came from other hardware.
    Moved,
    /// The hardware could not answer now.
    Pending,
}

/// A workspace set aside under `moved/`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MovedCopy {
    /// Its directory under `moved/`.
    pub id: String,
    /// The account it belonged to; `None` for a login's workspace.
    pub account: Option<String>,
    /// The device it was.
    pub device_id: String,
    /// When it was set aside, in Unix milliseconds.
    pub moved_at: u64,
    /// Why it counts as moved: `anchor` or `hardware_key`.
    pub reason: String,
}

/// Check a workspace's anchor on this hardware; one without an anchor gets
/// it now, unless the hardware makes none.
fn verdict(anchor: &dyn HardwareAnchor, store: &Store, meta: &mut Meta) -> Result<Verdict> {
    let Some(held) = &meta.anchor else {
        if let Ok((handle, public)) = anchor.create() {
            meta.anchor = Some(Anchor {
                handle: handle.into(),
                public: public.into(),
                created_at: now_ms(),
                late: Workspace::holds_hardware(store, meta)?,
                foreign: false,
            });
            store.set_meta(META, meta)?;
        }
        return Ok(Verdict::Native);
    };
    if held.foreign {
        return Ok(Verdict::Moved);
    }
    let message = digest(
        "dmsg/hardware-anchor/v1",
        &(&meta.environment, meta.device_id, Hash::new(random_id())),
    );
    Ok(match anchor.sign(&held.handle, &message) {
        Ok(signature) if verified(&held.public, &message, &signature) => Verdict::Native,
        Ok(_) | Err(Error::IntegrityFailed) => Verdict::Moved,
        Err(_) => Verdict::Pending,
    })
}

/// Whether `signature` verifies; the hardware may sign with a high `s`.
fn verified(public: &[u8], message: &Hash, signature: &[u8; 64]) -> bool {
    p256::ecdsa::Signature::from_slice(signature).is_ok_and(|s| {
        dmsg_protocol::verify_p256(
            public,
            message.as_slice(),
            &s.normalize_s().to_bytes().into(),
        )
        .is_ok()
    })
}

/// Whether `id` names a directory directly under `moved/`.
pub(crate) fn plain_name(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains(['/', '\\'])
        && Path::new(id).file_name().is_some_and(|name| name == id)
}

impl Client {
    /// Check every workspace on this device against `anchor`, which makes
    /// the anchors still missing; workspaces from other hardware are moved
    /// from now on.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn set_hardware_anchor(&mut self, anchor: Arc<dyn HardwareAnchor>) -> Result<()> {
        self.anchor = Some(anchor.clone());
        self.moved.clear();
        let entries = match std::fs::read_dir(self.dir.join("accounts")) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return self.check_hardware(),
            Err(e) => return Err(storage(e)),
        };
        for entry in entries {
            let entry = entry.map_err(storage)?;
            let Ok(account) = entry.file_name().to_string_lossy().parse::<AccountId>() else {
                continue;
            };
            let path = entry.path().join(crate::client::WORKSPACE);
            if self.slot == Slot::Account(account) || !path.exists() {
                continue;
            }
            let store = Store::open(&path)?;
            let Some(mut meta) = store.meta::<Meta>(META)? else {
                continue;
            };
            if verdict(anchor.as_ref(), &store, &mut meta)? == Verdict::Moved {
                self.moved.insert(account);
            }
        }
        self.check_hardware()
    }

    /// Check the selected workspace's anchor.
    pub(crate) fn check_hardware(&mut self) -> Result<()> {
        self.hardware = match (&self.anchor, &self.slot) {
            (None, _) | (_, Slot::Idle) => Verdict::Native,
            (Some(anchor), _) => {
                let mut meta = self.workspace.meta.clone();
                let verdict = verdict(anchor.as_ref(), &self.workspace.store, &mut meta)?;
                self.workspace.meta = meta;
                verdict
            }
        };
        if let Slot::Account(account) = &self.slot {
            match self.hardware {
                Verdict::Moved => self.moved.insert(*account),
                _ => self.moved.remove(account),
            };
        }
        Ok(())
    }

    /// Before an unlock: a moved workspace never unlocks.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a moved workspace and
    /// `Error::Unavailable` while the hardware cannot answer.
    pub(crate) fn hardware_ready(&mut self) -> Result<()> {
        if self.hardware == Verdict::Pending {
            self.check_hardware()?;
        }
        match self.hardware {
            Verdict::Native => Ok(()),
            Verdict::Moved => Err(Error::Forbidden),
            Verdict::Pending => Err(Error::Unavailable("hardware: anchor".into())),
        }
    }

    /// What a hardware key of the selected workspace answered. After a late
    /// anchor the first key that opens settles the workspace as native; one
    /// this hardware cannot use moves it.
    ///
    /// # Errors
    /// The answer's own error, or `Error::Forbidden` once moved.
    pub(crate) fn hardware_key<T>(&mut self, opened: Result<T>) -> Result<T> {
        let Some(anchor) = self.workspace.meta.anchor.as_mut().filter(|a| a.late) else {
            return opened;
        };
        let foreign = match &opened {
            Ok(_) => false,
            Err(Error::IntegrityFailed) => true,
            Err(_) => return opened,
        };
        anchor.late = false;
        anchor.foreign = foreign;
        self.workspace.save_meta()?;
        if foreign {
            self.check_hardware()?;
            return Err(Error::Forbidden);
        }
        opened
    }

    /// Set the selected workspace, which came from other hardware, aside
    /// under `moved/`, whole, and start a new device in its place: the
    /// account then adds this hardware as a new device. The copy stays for
    /// import until [`Self::discard_moved`].
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the workspace is not moved and
    /// `Error::Unavailable` when its files cannot move.
    pub fn set_aside_moved(&mut self) -> Result<Status> {
        ensure(self.hardware == Verdict::Moved, Error::Forbidden)?;
        let slot = self.slot.clone();
        let meta = self.workspace.meta.clone();
        let from = self.slot_dir(&slot);
        let name = from
            .file_name()
            .ok_or(Error::IntegrityFailed)?
            .to_string_lossy()
            .into_owned();
        let device = hex::encode(*meta.device_id);
        let now = now_ms();
        let moved = self.dir.join(MOVED);
        let mut id = format!("{name}-{}", &device[..8]);
        if moved.join(&id).exists() {
            id = format!("{id}-{now}");
        }
        let record = MovedCopy {
            id: id.clone(),
            account: meta.account.as_ref().map(|a| a.id.to_string()),
            device_id: device,
            moved_at: now,
            reason: match meta.anchor.as_ref().is_some_and(|a| a.foreign) {
                true => "hardware_key",
                false => "anchor",
            }
            .into(),
        };
        self.lock();
        // Close the database before its directory moves.
        self.workspace = crate::client::idle(&self.config.environment)?;
        self.slot = Slot::Idle;
        std::fs::write(
            from.join(MOVED_RECORD),
            serde_json::to_vec(&record).map_err(|_| Error::IntegrityFailed)?,
        )
        .map_err(storage)?;
        std::fs::create_dir_all(&moved).map_err(storage)?;
        std::fs::rename(&from, moved.join(&id)).map_err(storage)?;
        if let Slot::Account(account) = &slot {
            self.moved.remove(account);
        }
        self.select(slot)?;
        self.status()
    }

    /// The workspaces set aside under `moved/`, the latest first.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn moved_copies(&self) -> Result<Vec<MovedCopy>> {
        let entries = match std::fs::read_dir(self.dir.join(MOVED)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(storage(e)),
        };
        let mut list = Vec::new();
        for entry in entries {
            let entry = entry.map_err(storage)?;
            let Ok(bytes) = std::fs::read(entry.path().join(MOVED_RECORD)) else {
                continue;
            };
            let Ok(mut copy) = serde_json::from_slice::<MovedCopy>(&bytes) else {
                continue;
            };
            copy.id = entry.file_name().to_string_lossy().into_owned();
            list.push(copy);
        }
        list.sort_by_key(|c| std::cmp::Reverse(c.moved_at));
        Ok(list)
    }

    /// Delete a copy set aside, with what the credential store kept for
    /// it; the person confirmed it.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown copy and
    /// `Error::Unavailable` when its files cannot be removed.
    pub fn discard_moved(&mut self, id: &str) -> Result<()> {
        ensure(plain_name(id), Error::NotFound)?;
        let copy = self
            .moved_copies()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(Error::NotFound)?;
        if let (Some(secure), Some(account)) = (self.secure(), &copy.account) {
            let name = format!(
                "dmsg:{}:{account}:{}",
                crate::config::environment_name(&self.config.environment),
                copy.device_id
            );
            secure.delete(&format!("{name}:fast-unlock"))?;
            secure.delete(&format!("{name}:device-bound"))?;
        }
        std::fs::remove_dir_all(self.dir.join(MOVED).join(id)).map_err(storage)
    }
}

#[cfg(test)]
#[path = "hardware_tests.rs"]
mod tests;
