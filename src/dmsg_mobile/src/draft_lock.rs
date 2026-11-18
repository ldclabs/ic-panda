use super::*;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use zeroize::Zeroizing;

/// Editor inputs that a normal draft store cannot yet accept, for example
/// a pasted letter over its local size limit. They stay editable on Android.
#[derive(Default, uniffi::Record, Serialize, Deserialize)]
pub struct EditorDrafts {
    pub drafts: Vec<MessageDraft>,
    pub letters: Vec<LetterDraftInput>,
    pub feedback: Option<FeedbackDraftInput>,
    pub vault: Vec<vault::VaultDraftInput>,
}

impl EditorDrafts {
    fn is_empty(&self) -> bool {
        self.drafts.is_empty()
            && self.letters.is_empty()
            && self.feedback.is_none()
            && self.vault.is_empty()
    }

    fn checkpoint(&self) -> dmsg_types::Result<Option<Zeroizing<Vec<u8>>>> {
        if self.is_empty() {
            return Ok(None);
        }
        serde_json::to_vec(self)
            .map(Zeroizing::new)
            .map(Some)
            .map_err(|_| Error::IntegrityFailed)
    }

    fn restore(mut self, c: &mut Client) -> dmsg_types::Result<Self> {
        // Return a rejected form to its editor. A storage error leaves the
        // encrypted checkpoint on disk for a retry.
        fn unsaved<T>(result: dmsg_types::Result<T>) -> dmsg_types::Result<bool> {
            match result {
                Ok(_) | Err(Error::VersionConflict | Error::IdempotencyConflict) => Ok(false),
                Err(Error::InvalidInput(_) | Error::NotFound | Error::QuotaExceeded) => Ok(true),
                Err(error) => Err(error),
            }
        }
        let mut pending = Self::default();
        for draft in self.drafts {
            let saved = draft.content().and_then(|content| match &draft.stamp {
                Some(stamp) => c
                    .save_stamped_composer(&draft.channel, &content, &stamp.clone().into())
                    .map(|_| ()),
                None => c.channel_draft(&draft.channel, &draft.text),
            });
            if unsaved(saved)? {
                pending.drafts.push(draft);
            }
        }
        for draft in self.vault {
            if unsaved(c.checkpoint_vault_draft(&draft.clone().into()))? {
                pending.vault.push(draft);
            }
        }
        let now = dmsg_client::now_ms();
        for draft in self.letters {
            if unsaved(c.save_letter_draft_at(
                &draft.id,
                &draft.recipient,
                &draft.text,
                draft.invitation.clone(),
                now,
            ))? {
                pending.letters.push(draft);
            }
        }
        if let Some(mut draft) = self.feedback.take() {
            if let Some(current) = c.feedback()? {
                // A queued or replaced report takes precedence over the old editor.
                if current.id != draft.id || current.state != "draft" {
                    return Ok(pending);
                }
                if current.text == draft.text
                    && current.context == draft.context
                    && current
                        .screenshots
                        .iter()
                        .map(|s| &s.file)
                        .eq(draft.files.iter())
                {
                    return Ok(pending);
                }
                draft.revision = current.revision;
            }
            if unsaved(draft.save(c))? {
                pending.feedback = Some(draft);
            }
        }
        Ok(pending)
    }
}

/// Completion of the off-thread checkpoint. The host owns the write even if
/// the Activity or the caller waiting for its result goes away.
#[derive(uniffi::Object)]
pub struct DraftLock {
    done: Mutex<Option<oneshot::Receiver<dmsg_types::Result<()>>>>,
}

#[uniffi::export]
impl DraftLock {
    pub async fn finish(&self) -> Result<()> {
        let done = self.done.lock().unwrap().take().ok_or(Error::Locked)?;
        Ok(done.await.map_err(|_| Error::Locked)??)
    }
}

#[uniffi::export]
impl MobileEngine {
    /// Fence ordinary work synchronously, including work already waiting on I/O.
    pub fn begin_draft_lock(
        &self,
        account: String,
        device: String,
        drafts: Vec<MessageDraft>,
        letters: Vec<LetterDraftInput>,
        feedback: Option<FeedbackDraftInput>,
        vault: Vec<vault::VaultDraftInput>,
    ) -> Arc<DraftLock> {
        let input = EditorDrafts {
            drafts,
            letters,
            feedback,
            vault,
        };
        let done = self.host.inner.lock_with_checkpoint(
            move |client| {
                let bytes =
                    Zeroizing::new(serde_json::to_vec(&input).map_err(|_| Error::IntegrityFailed)?);
                client.seal_lock_drafts(&account, &device, &bytes)
            },
            Client::persist_lock_drafts,
        );
        self.cancel_login();
        self.platform.cancel_presence();
        Arc::new(DraftLock {
            done: Mutex::new(Some(done)),
        })
    }

    /// Recover only the drafts belonging to this authenticated session.
    pub async fn restore_editor_drafts(&self) -> Result<Option<EditorDrafts>> {
        self.held(|h| Box::pin(async { h.with(recover).await }))
            .await
    }
}

/// A later successful save or discard supersedes this editor's recovery copy.
pub(super) fn resolved(
    c: &mut Client,
    remove: impl FnOnce(&mut EditorDrafts),
) -> dmsg_types::Result<()> {
    if c.has_editable_lock_drafts() {
        c.recover_lock_drafts(|_, bytes| {
            let mut pending: EditorDrafts =
                serde_json::from_slice(bytes).map_err(|_| Error::IntegrityFailed)?;
            remove(&mut pending);
            Ok(((), pending.checkpoint()?))
        })?;
    }
    Ok(())
}

pub(super) fn recover(c: &mut Client) -> dmsg_types::Result<Option<EditorDrafts>> {
    let pending = c
        .recover_lock_drafts(|client, bytes| {
            let drafts = serde_json::from_slice::<EditorDrafts>(bytes)
                .map_err(|_| Error::IntegrityFailed)?
                .restore(client)?;
            let remaining = drafts.checkpoint()?;
            Ok((drafts, remaining))
        })?
        .filter(|drafts| !drafts.is_empty());
    Ok(pending)
}
