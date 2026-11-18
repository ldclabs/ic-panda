//! Last mobile editor input. This record stays in the owning device's encrypted workspace.
use crate::{Client, storage};
use dmsg_types::{Error, Hash, Result, ensure};
use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Write},
};

const RECORD: &str = "mobile-lock-drafts:1";
pub(crate) const FILE: &str = "mobile-lock-drafts.v1";
pub(crate) const TEMP: &str = "mobile-lock-drafts.v1.tmp";

#[derive(Default)]
pub(crate) struct LockDraftCheckpoint {
    // Keep only ciphertext if the filesystem temporarily refuses the write.
    unsaved: BTreeMap<Hash, String>,
    checked: Option<Hash>,
    editable: Option<Hash>,
}

impl Client {
    /// Seal final input for this unlocked account and device without performing I/O.
    /// # Errors
    /// Returns `Locked` if the originating workspace no longer owns the keys.
    pub fn seal_lock_drafts(&self, account: &str, device: &str, bytes: &[u8]) -> Result<String> {
        ensure(
            self.session_state().unlocked_account() == Some(account)
                && hex::encode(*self.workspace.meta.device_id) == device,
            Error::Locked,
        )?;
        Ok(self.workspace.seal_record(self.keys()?, RECORD, bytes))
    }

    /// Persist an already sealed checkpoint independently of the workspace database.
    /// # Errors
    /// On a filesystem error the same device can still recover the ciphertext in memory.
    pub fn persist_lock_drafts(&mut self, sealed: String) -> Result<()> {
        let device = self.workspace.meta.device_id;
        self.lock_drafts.checked = None;
        self.lock_drafts.editable = None;
        self.lock_drafts.unsaved.insert(device, sealed);
        let dir = self.slot_dir(&self.slot);
        let temp = dir.join(TEMP);
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(storage)?;
        file.write_all(self.lock_drafts.unsaved[&device].as_bytes())
            .map_err(storage)?;
        file.sync_all().map_err(storage)?;
        fs::rename(temp, dir.join(FILE)).map_err(storage)?;
        #[cfg(unix)]
        fs::File::open(dir)
            .and_then(|dir| dir.sync_all())
            .map_err(storage)?;
        self.lock_drafts.unsaved.remove(&device);
        Ok(())
    }

    /// Whether an unlocked editor is still correcting a recovered input.
    pub fn has_editable_lock_drafts(&self) -> bool {
        self.lock_drafts.editable == Some(self.workspace.meta.device_id)
    }

    /// Replay an interrupted checkpoint in its unlocked workspace. Keep it until
    /// every local write succeeds. A remainder keeps the unaccepted edits protected.
    /// # Errors
    /// Propagates authentication, storage and replay errors without consuming the input.
    pub fn recover_lock_drafts<T>(
        &mut self,
        replay: impl FnOnce(&mut Client, &[u8]) -> Result<(T, Option<zeroize::Zeroizing<Vec<u8>>>)>,
    ) -> Result<Option<T>> {
        let device = self.workspace.meta.device_id;
        if self.lock_drafts.checked == Some(device)
            || self.session_state().unlocked_account().is_none()
        {
            return Ok(None);
        }
        let path = self.slot_dir(&self.slot).join(FILE);
        let sealed = match self.lock_drafts.unsaved.get(&device) {
            Some(value) => Some(value.clone()),
            None => match fs::read_to_string(&path) {
                Ok(value) => Some(value),
                Err(e) if e.kind() == ErrorKind::NotFound => None,
                Err(e) => return Err(storage(e)),
            },
        };
        let result = if let Some(sealed) = sealed {
            let bytes = self.workspace.open_record(self.keys()?, RECORD, &sealed)?;
            let (restored, remainder) = replay(self, &bytes)?;
            if let Some(remainder) = remainder {
                if *bytes != *remainder {
                    let sealed = self.workspace.seal_record(self.keys()?, RECORD, &remainder);
                    self.persist_lock_drafts(sealed)?;
                }
                self.lock_drafts.editable = Some(device);
                return Ok(Some(restored));
            }
            match fs::remove_file(&path) {
                Ok(()) => {
                    #[cfg(unix)]
                    fs::File::open(self.slot_dir(&self.slot))
                        .and_then(|dir| dir.sync_all())
                        .map_err(storage)?;
                }
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(storage(e)),
            }
            self.lock_drafts.unsaved.remove(&device);
            Some(restored)
        } else {
            None
        };
        self.lock_drafts.checked = Some(device);
        self.lock_drafts.editable = None;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn failed_write_retains_only_ciphertext_until_the_same_device_unlocks() {
        let dir = testing::TempDir::new();
        let mut client = testing::unlocked(&dir.0, false, None);
        let account = testing::account().id.to_string();
        let device = client.status().unwrap().device_id.unwrap();
        let last = b"last private field";
        assert!(matches!(
            client.seal_lock_drafts("another account", &device, last),
            Err(Error::Locked)
        ));
        assert!(matches!(
            client.seal_lock_drafts(&account, "another device", last),
            Err(Error::Locked)
        ));
        let sealed = client.seal_lock_drafts(&account, &device, last).unwrap();
        let temp = client.slot_dir(&client.slot).join(TEMP);
        fs::create_dir(&temp).unwrap();
        client.lock();
        assert!(client.persist_lock_drafts(sealed).is_err());
        assert!(
            client
                .recover_lock_drafts::<()>(|_, _| panic!("locked"))
                .unwrap()
                .is_none()
        );
        fs::remove_dir(temp).unwrap();
        testing::unlock_fixture(&mut client);
        let value = client
            .recover_lock_drafts(|_, bytes| Ok((bytes.to_vec(), None)))
            .unwrap();
        assert_eq!(value.as_deref(), Some(last.as_slice()));
        assert!(
            client
                .recover_lock_drafts::<()>(|_, _| panic!("consumed"))
                .unwrap()
                .is_none()
        );
    }
}
