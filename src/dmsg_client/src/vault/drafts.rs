//! Device-local editor checkpoints, encrypted with the workspace LDK.
use super::*;

const PREFIX: &str = "vault-draft:";
const ACTIVE: &str = "vault-draft-active";

/// Complete, possibly invalid input. Native editors own their versioned form schema.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Draft {
    /// Random editor identity; a new edit after save/discard gets a new ID.
    pub id: String,
    /// Existing item, or a new item not yet created.
    pub item_id: Option<String>,
    /// Immutable version the person started editing.
    pub base_revision: Option<String>,
    /// Monotonic authored revision, retained when reopening a checkpoint.
    pub revision: u64,
    /// Bounded JSON containing raw fields, incomplete tags and the original form.
    pub form: String,
    /// Whether returning to this account should reopen this editor.
    pub active: bool,
}

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    revision: u64,
    draft: Option<Draft>,
    receipt: Option<Receipt>,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    digest: dmsg_types::Hash,
    item: String,
    revision: String,
}

impl Draft {
    fn check(&self) -> Result<()> {
        crate::upload::hex_id(&self.id)?;
        ensure_valid(
            self.revision > 0 && self.form.len() <= 512 * 1024,
            "vault draft",
        )?;
        ensure_valid(
            self.item_id.is_some() == self.base_revision.is_some(),
            "draft target",
        )?;
        if let Some(id) = &self.item_id {
            crate::upload::hex_id(id)?;
        }
        if let Some(base) = &self.base_revision {
            crate::upload::hex_id(base)?;
        }
        ensure_valid(
            serde_json::from_str::<serde_json::Value>(&self.form).is_ok(),
            "draft form",
        )
    }
}

impl Client {
    fn vault_checkpoint(&self, id: &str) -> Result<Option<Checkpoint>> {
        crate::upload::hex_id(id)?;
        self.workspace
            .private(self.keys()?, &format!("{PREFIX}{id}"))
    }

    /// Read unfinished forms only in their unlocked account workspace.
    /// # Errors
    /// Returns lock, storage or integrity errors without inventing empty drafts.
    pub fn vault_drafts(&self) -> Result<Vec<Draft>> {
        let keys = self.keys()?;
        let active: Option<String> = self.workspace.private(keys, ACTIVE)?;
        let mut drafts = Vec::new();
        for key in self.workspace.store.sealed_keys(PREFIX)? {
            if let Some(checkpoint) = self.workspace.private::<Checkpoint>(keys, &key)?
                && let Some(mut draft) = checkpoint.draft
            {
                draft.active = active.as_deref() == Some(draft.id.as_str());
                drafts.push(draft);
            }
        }
        Ok(drafts)
    }

    /// Save incomplete input without creating an item or a sync job.
    /// # Errors
    /// Rejects invalid identities, changed targets and reused revisions with different input.
    /// Older writes and writes to a finished editor return false.
    pub fn checkpoint_vault_draft(&self, draft: &Draft) -> Result<bool> {
        draft.check()?;
        let keys = self.keys()?;
        if let Some(old) = self.vault_checkpoint(&draft.id)? {
            let Some(previous) = old.draft else {
                return Ok(false);
            };
            ensure(
                previous.item_id == draft.item_id && previous.base_revision == draft.base_revision,
                Error::VersionConflict,
            )?;
            if old.revision > draft.revision {
                return Ok(false);
            }
            if old.revision == draft.revision {
                ensure(previous == *draft, Error::IdempotencyConflict)?;
                return Ok(true);
            }
        }
        self.workspace.store.atomic(|_| {
            self.workspace.put_private(
                keys,
                &format!("{PREFIX}{}", draft.id),
                &Checkpoint {
                    revision: draft.revision,
                    draft: Some(draft.clone()),
                    receipt: None,
                },
            )?;
            if draft.active {
                self.workspace.put_private(keys, ACTIVE, &draft.id)?;
            } else {
                self.clear_active_vault_draft(&draft.id)?;
            }
            Ok(true)
        })
    }

    fn clear_active_vault_draft(&self, id: &str) -> Result<()> {
        let active: Option<String> = self.workspace.private(self.keys()?, ACTIVE)?;
        if active.as_deref() == Some(id) {
            self.workspace.delete_private(ACTIVE)?;
        }
        Ok(())
    }

    /// Explicit discard leaves a small fence so a delayed write cannot revive it.
    /// # Errors
    /// Returns lock, malformed identity or storage errors.
    pub fn discard_vault_draft(&self, id: &str) -> Result<()> {
        let old = self.vault_checkpoint(id)?;
        if old.as_ref().is_some_and(|value| value.draft.is_none()) {
            return Ok(());
        }
        self.workspace.store.atomic(|_| {
            self.workspace.put_private(
                self.keys()?,
                &format!("{PREFIX}{id}"),
                &Checkpoint {
                    revision: old.map_or(0, |value| value.revision),
                    draft: None,
                    receipt: None,
                },
            )?;
            self.clear_active_vault_draft(id)
        })
    }

    /// Save the final input, then atomically commit its item and consume its draft.
    /// A retry of the same save returns the original version, including after restart.
    /// # Errors
    /// As vault creation/editing. Validation failures retain the editable checkpoint.
    pub fn commit_vault_draft(
        &self,
        draft: &Draft,
        kind: &str,
        changes: &Changes,
    ) -> Result<VaultEntry> {
        draft.check()?;
        let digest = dmsg_protocol::digest("dmsg/vault-draft-save/v1", &(draft, kind, changes));
        if let Some(receipt) = self
            .vault_checkpoint(&draft.id)?
            .and_then(|value| value.receipt)
        {
            ensure(receipt.digest == digest, Error::IdempotencyConflict)?;
            return self.entry(&self.version_row(&receipt.item, &receipt.revision)?);
        }
        ensure(self.checkpoint_vault_draft(draft)?, Error::VersionConflict)?;
        let now = crate::now_ms();
        let (id, mut item) = if let Some(id) = &draft.item_id {
            let (_, head) = self.current(id)?;
            ensure(!head.tombstone, Error::NotFound)?;
            let (base, item) = self.open_row(
                &self.version_row(id, draft.base_revision.as_deref().ok_or(Error::NotFound)?)?,
            )?;
            ensure(!base.tombstone, Error::NotFound)?;
            ensure(!item.reserved(), Error::Forbidden)?;
            (id.clone(), item)
        } else {
            ensure_valid(TYPES.contains(&kind), "type")?;
            (random_hex(), Item::new(kind, "", now))
        };
        let original = item.clone();
        changes.apply(&mut item);
        item.normalize();
        ensure(!item.reserved(), Error::Forbidden)?;
        item.validate()?;
        if let Some(base) = &draft.base_revision
            && item.same(&original)
        {
            let entry = self.entry(&self.version_row(&id, base)?)?;
            self.workspace
                .store
                .atomic(|_| self.consume_vault_draft(draft, digest, &entry.id, &entry.revision))?;
            return Ok(entry);
        }
        item.updated_at = now;
        let payload = Zeroizing::new(canonical(&item));
        ensure(payload.len() <= MAX_OBJECT, Error::QuotaExceeded)?;
        let roots = self.roots()?;
        let generation = roots.generation;
        let record = EncryptedObject::seal(
            self.environment(),
            (
                generation,
                roots.key(generation).ok_or(Error::RecoveryIncomplete)?,
            ),
            &self.account()?.id.to_string(),
            &hex::encode(*self.workspace.meta.device_id),
            VAULT,
            id,
            draft.base_revision.clone(),
            false,
            &payload,
            now,
        );
        let conflict = self.write_record(&record, None, || {
            self.consume_vault_draft(draft, digest, &record.id, &record.revision)
        })?;
        Ok(VaultEntry::new(&record, item, false, conflict))
    }

    fn consume_vault_draft(
        &self,
        draft: &Draft,
        digest: dmsg_types::Hash,
        item: &str,
        revision: &str,
    ) -> Result<()> {
        self.workspace.put_private(
            self.keys()?,
            &format!("{PREFIX}{}", draft.id),
            &Checkpoint {
                revision: draft.revision,
                draft: None,
                receipt: Some(Receipt {
                    digest,
                    item: item.into(),
                    revision: revision.into(),
                }),
            },
        )?;
        self.clear_active_vault_draft(&draft.id)
    }
}

#[cfg(test)]
mod tests;
