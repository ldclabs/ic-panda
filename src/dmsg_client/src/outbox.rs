//! Durable user-authored send intents, saved before network preparation.
//! Existing channel send journals retain their IDs and bytes across retries.
use crate::{Client, channel, hold::Hold};
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};

pub use crate::letters::send_free_letter;

const PREFIX: &str = "send-intent:";
const LIMIT: usize = 100;
#[path = "outbox_composer.rs"]
mod composer;
pub use composer::ComposerDraft;
/// An older send waits for the person, who may no longer mean it.
const RECOVER_MS: u64 = 24 * 60 * 60 * 1000;

/// One writer process and monotonically increasing authored revision.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DraftStamp {
    /// Random 128-bit process identifier, encoded as lowercase hex.
    pub writer: String,
    /// Increased for each edit or explicit send in this writer.
    pub revision: u64,
}

impl DraftStamp {
    fn validate(&self) -> Result<()> {
        ensure_valid(
            self.writer.len() == 32
                && self
                    .writer
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && self.revision > 0,
            "draft stamp",
        )
    }
}

/// A locally sealed message awaiting relay confirmation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Intent {
    /// The stable channel and send IDs.
    pub channel: String,
    /// Reused by every retry, including after restart.
    pub id: String,
    /// User-authored text, sealed under this workspace's LDK.
    pub text: String,
    /// Exact original message references for replies and amendments.
    #[serde(default)]
    pub reply_to: Option<channel::MessageRef>,
    /// Same-author replacement, checked again when dispatched.
    #[serde(default)]
    pub edit_of: Option<channel::MessageRef>,
    /// Same-author withdrawal, checked again when dispatched.
    #[serde(default)]
    pub retract: Option<channel::MessageRef>,
    /// Immutable local file version selected by the person, when present.
    #[serde(default)]
    pub attachment: Option<Attachment>,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Local batch and position preserve attachment order after restart.
    #[serde(default)]
    pub batch: Option<(String, u32)>,
    /// An attempt began; absence of a reply never implies cancellation.
    pub attempted: bool,
    /// Stable error code only.
    pub error: Option<String>,
}

/// A library version, never a mutable external document-provider URL.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    /// Library file ID.
    pub file: String,
    /// Exact version that will be sent.
    pub version: String,
    /// Display name, kept inside the sealed intent.
    pub name: String,
}

impl Client {
    /// Settled with the verified message and send journals in one transaction.
    pub(crate) fn forget_message_intent(&self, channel: &str, id: &str) -> Result<()> {
        self.workspace
            .delete_private(&format!("{PREFIX}{channel}:{id}"))
    }

    /// Atomically seal an intent and clear the matching composer draft.
    ///
    /// # Errors
    /// Refuses a locked workspace, missing channel, oversized text or full queue.
    pub fn queue_message(&self, channel: &str, text: &str) -> Result<Intent> {
        self.queue_draft(
            channel,
            &channel::Draft {
                text: text.into(),
                ..Default::default()
            },
        )
    }

    /// Persist a reply, edit or withdrawal before its first network attempt.
    pub fn queue_draft(&self, channel: &str, draft: &channel::Draft) -> Result<Intent> {
        ensure_valid(
            draft.agent.is_none() && draft.packet.is_none(),
            "user draft",
        )?;
        ensure_valid(
            draft.edit_of.is_none() || draft.retract.is_none(),
            "amendment",
        )?;
        self.queue_intent(channel, draft, None)
    }

    /// Queue an immutable attachment before any upload or authorization wait.
    ///
    /// # Errors
    /// As [`Self::queue_message`], or `NotFound` for a missing file.
    pub fn queue_attachment(&mut self, channel: &str, file: &str, text: &str) -> Result<Intent> {
        self.queue_attachment_reply(channel, file, text, None)
    }

    /// Queue an attachment with the same verified reply reference as a text draft.
    pub fn queue_attachment_reply(
        &mut self,
        channel: &str,
        file: &str,
        text: &str,
        reply_to: Option<channel::MessageRef>,
    ) -> Result<Intent> {
        if let Some(reference) = &reply_to {
            let original =
                self.channel_messages_before(channel, reference.seq.saturating_add(1), 1)?;
            ensure(
                original
                    .first()
                    .is_some_and(|m| m.seq == reference.seq && m.digest == reference.digest),
                Error::VersionConflict,
            )?;
        }
        let details = self.files_show(file)?;
        self.queue_intent(
            channel,
            &channel::Draft {
                text: text.into(),
                reply_to,
                ..Default::default()
            },
            Some(Attachment {
                file: file.into(),
                version: details.file.current,
                name: details.file.name,
            }),
        )
    }

    fn queue_intent(
        &self,
        channel: &str,
        draft: &channel::Draft,
        attachment: Option<Attachment>,
    ) -> Result<Intent> {
        let _timing =
            crate::diagnostic::MessageTiming::start(crate::diagnostic::MessagePhase::Queue);
        self.channel_view(channel)?;
        let text = &draft.text;
        ensure_valid(
            (!text.trim().is_empty() || attachment.is_some() || draft.retract.is_some())
                && text.len() <= 16 * 1024,
            "message",
        )?;
        ensure(
            self.message_intents(channel)?.len() < LIMIT,
            Error::QuotaExceeded,
        )?;
        let intent = Intent {
            channel: channel.into(),
            id: hex::encode(crate::crypto::random_id()),
            text: text.clone(),
            reply_to: draft.reply_to.clone(),
            edit_of: draft.edit_of.clone(),
            retract: draft.retract.clone(),
            attachment,
            created_at: crate::now_ms(),
            batch: None,
            attempted: false,
            error: None,
        };
        self.workspace.store.atomic(|_| {
            self.workspace
                .put_private(self.keys()?, &intent.key(), &intent)?;
            if draft.edit_of.is_none()
                && draft.retract.is_none()
                && self.channel_view(channel)?.draft == *text
            {
                self.channel_draft(channel, "")?;
            }
            Ok(())
        })?;
        Ok(intent)
    }

    /// The account-local queue, bounded and readable only while unlocked.
    ///
    /// # Errors
    /// Refuses a locked workspace or a corrupt record.
    pub fn message_intents(&self, channel: &str) -> Result<Vec<Intent>> {
        let prefix = format!("{PREFIX}{channel}:");
        let mut intents: Vec<Intent> =
            self.workspace
                .private_after(self.keys()?, &prefix, &prefix, LIMIT)?;
        intents.sort_by(|a, b| a.order().cmp(&b.order()));
        Ok(intents)
    }

    /// Sends in any channel that never reached a verdict, oldest first, to
    /// submit again under their own IDs. A refusal stays for the person.
    ///
    /// # Errors
    /// Refuses a locked workspace or a corrupt record.
    pub fn recoverable_intents(&self, now: u64) -> Result<Vec<Intent>> {
        let mut intents: Vec<Intent> =
            self.workspace
                .private_after(self.keys()?, PREFIX, PREFIX, LIMIT)?;
        intents.retain(|i| {
            i.created_at.saturating_add(RECOVER_MS) > now
                && i.error
                    .as_deref()
                    .is_none_or(|code| code == "CANCELLED" || crate::sync::transient(code))
        });
        intents.sort_by(|a, b| a.order().cmp(&b.order()));
        Ok(intents)
    }
}

impl Intent {
    fn order(&self) -> (u64, &str, u32) {
        let (batch, position) = self
            .batch
            .as_ref()
            .map_or((self.id.as_str(), 0), |(id, position)| {
                (id.as_str(), *position)
            });
        (self.created_at, batch, position)
    }

    fn key(&self) -> String {
        format!("{PREFIX}{}:{}", self.channel, self.id)
    }
}

/// Submit the original intent, never replacing an unknown send with a new ID.
///
/// # Errors
/// Preserves the intent and stable failure on an unconfirmed result.
pub async fn send<H: Hold>(h: &H, channel: &str, id: &str) -> Result<serde_json::Value> {
    let mut intent = h
        .with(|c| {
            let intent = c
                .message_intents(channel)?
                .into_iter()
                .find(|i| i.id == id)
                .ok_or(Error::NotFound)?;
            Ok(intent)
        })
        .await?;
    intent.attempted = true;
    h.with(|c| c.workspace.put_private(c.keys()?, &intent.key(), &intent))
        .await?;
    let draft = channel::Draft {
        text: intent.text.clone(),
        reply_to: intent.reply_to.clone(),
        edit_of: intent.edit_of.clone(),
        retract: intent.retract.clone(),
        ..Default::default()
    };
    let result = match &intent.attachment {
        Some(file) => {
            channel::send_file(
                h,
                channel,
                &file.file,
                Some(&file.version),
                &draft,
                Some(id),
            )
            .await
        }
        None => channel::send(h, channel, &draft, Some(id)).await,
    };
    match &result {
        Ok(_) => {
            h.with(|c| {
                c.keys()?;
                c.workspace.delete_private(&intent.key())
            })
            .await?
        }
        Err(error) => {
            intent.error = Some(crate::error_code(error));
            // If the session ended, keep the already sealed pending intent.
            let _ = h
                .with(|c| {
                    if c.workspace
                        .private::<Intent>(c.keys()?, &intent.key())?
                        .is_some()
                    {
                        c.workspace.put_private(c.keys()?, &intent.key(), &intent)?;
                    }
                    Ok(())
                })
                .await;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryStorage, TempDir, joined_channel, unlocked};
    use std::sync::Arc;

    #[test]
    fn a_lost_queue_reply_cannot_revive_the_sent_draft_or_mint_another_intent() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        let channel = "a1".repeat(32);
        joined_channel(&mut client, &channel);
        let stamp = DraftStamp {
            writer: "ab".repeat(16),
            revision: 1,
        };
        client
            .save_stamped_draft(&channel, "last keystroke", &stamp)
            .unwrap();
        let id = client
            .queue_stamped_message(&channel, "last keystroke", &stamp, None, None)
            .unwrap();
        // The queue committed but its UI reply was lost before an explicit lock flush.
        client
            .save_stamped_message_drafts(&[(&channel, "last keystroke", Some(&stamp))])
            .unwrap();
        assert!(client.channel_view(&channel).unwrap().draft.is_empty());
        assert_eq!(
            client
                .queue_stamped_message(&channel, "last keystroke", &stamp, None, None)
                .unwrap(),
            id
        );
        assert_eq!(client.message_intents(&channel).unwrap().len(), 1);
        client.forget_message_intent(&channel, &id).unwrap();
        assert_eq!(
            client
                .queue_stamped_message(&channel, "last keystroke", &stamp, None, None)
                .unwrap(),
            id
        );
        assert!(client.message_intents(&channel).unwrap().is_empty());
    }

    #[test]
    fn an_old_save_cannot_replace_new_typing_and_equal_text_can_be_a_new_intent() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        let channel = "a1".repeat(32);
        joined_channel(&mut client, &channel);
        let first = DraftStamp {
            writer: "ab".repeat(16),
            revision: 1,
        };
        let id = client
            .queue_stamped_message(&channel, "repeat", &first, None, None)
            .unwrap();
        let next = DraftStamp {
            revision: 2,
            ..first.clone()
        };
        assert!(
            client
                .save_stamped_draft(&channel, "repeat", &next)
                .unwrap()
        );
        assert!(
            !client
                .save_stamped_draft(&channel, "repeat", &first)
                .unwrap()
        );
        assert_eq!(client.channel_view(&channel).unwrap().draft, "repeat");
        assert_eq!(
            client.queue_stamped_message(&channel, "repeat", &first, None, None),
            Ok(id.clone())
        );
        assert_ne!(
            client
                .queue_stamped_message(&channel, "repeat", &next, None, None)
                .unwrap(),
            id
        );
        assert_eq!(client.message_intents(&channel).unwrap().len(), 2);
    }

    #[test]
    fn stamped_draft_replay_cannot_change_text_and_a_new_writer_can_resume() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        let channel = "a1".repeat(32);
        joined_channel(&mut client, &channel);
        let stamp = DraftStamp {
            writer: "ab".repeat(16),
            revision: 20,
        };
        client.save_stamped_draft(&channel, "kept", &stamp).unwrap();
        assert_eq!(
            client.save_stamped_draft(&channel, "changed", &stamp),
            Err(Error::IdempotencyConflict)
        );
        assert_eq!(client.channel_view(&channel).unwrap().draft, "kept");
        let restarted = DraftStamp {
            writer: "cd".repeat(16),
            revision: 1,
        };
        client
            .save_stamped_draft(&channel, "after restart", &restarted)
            .unwrap();
        assert_eq!(
            client.channel_view(&channel).unwrap().draft,
            "after restart"
        );
    }

    #[tokio::test]
    async fn queued_text_is_sealed_survives_restart_and_retains_its_id_after_failure() {
        let dir = TempDir::new();
        let storage = Arc::new(MemoryStorage::default());
        let mut client = unlocked(&dir.0, false, Some(storage.clone()));
        let channel = "a1".repeat(32);
        joined_channel(&mut client, &channel);
        client.enable_fast_unlock().unwrap();
        client
            .channel_draft(&channel, "private queued text")
            .unwrap();
        let intent = client
            .queue_message(&channel, "private queued text")
            .unwrap();
        assert!(client.channel_view(&channel).unwrap().draft.is_empty());
        assert!(send(&client.hold(), &channel, &intent.id).await.is_err());
        drop(client);
        let mut client =
            Client::open(crate::testing::config(false), &dir.0, Some(storage)).unwrap();
        client.unlock_fast().await.unwrap();
        let pending = client.message_intents(&channel).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, intent.id);
        assert_eq!(pending[0].text, "private queued text");
        assert!(pending[0].attempted);
        client.lock();
        assert_eq!(client.message_intents(&channel), Err(Error::Locked));
    }

    #[test]
    fn only_recent_sends_without_a_verdict_are_recovered_and_oldest_first() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        let (first, second) = ("a1".repeat(32), "b2".repeat(32));
        joined_channel(&mut client, &first);
        joined_channel(&mut client, &second);
        let stored = |channel: &str, created_at: u64, error: Option<&str>| {
            let mut intent = client.queue_message(channel, "text").unwrap();
            intent.created_at = created_at;
            intent.error = error.map(str::to_string);
            let keys = client.keys().unwrap();
            client
                .workspace
                .put_private(keys, &intent.key(), &intent)
                .unwrap();
            intent.id
        };
        let now = RECOVER_MS * 2;
        let later = stored(&first, now - 1, Some("NETWORK_UNAVAILABLE"));
        let earlier = stored(&second, now - 2, None);
        let interrupted = stored(&first, now - 3, Some("CANCELLED"));
        stored(&first, now - 4, Some("DEVICE_NOT_APPROVED"));
        stored(&first, now - RECOVER_MS, None);
        let ids: Vec<String> = client
            .recoverable_intents(now)
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(ids, [interrupted, earlier, later]);
    }
}
