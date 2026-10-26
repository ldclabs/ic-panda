//! Durable user-authored send intents, saved before network preparation.
//! Existing channel send journals retain their IDs and bytes across retries.
use crate::{Client, channel, hold::Hold};
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};

const PREFIX: &str = "send-intent:";
const LIMIT: usize = 100;

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
    /// Persist a batch of already authored drafts in one local transaction.
    pub fn save_message_drafts(&self, drafts: &[(String, String)]) -> Result<()> {
        self.workspace.store.atomic(|_| {
            for (channel, text) in drafts {
                self.channel_draft(channel, text)?;
            }
            Ok(())
        })
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
        let details = self.files_show(file)?;
        self.queue_intent(
            channel,
            &channel::Draft {
                text: text.into(),
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
        self.workspace
            .private_after(self.keys()?, &prefix, &prefix, LIMIT)
    }
}

impl Intent {
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
                .with(|c| c.workspace.put_private(c.keys()?, &intent.key(), &intent))
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
}

#[derive(Serialize, Deserialize)]
struct LetterIntent {
    recipient: String,
    text: String,
    invitation: Option<String>,
    result: Option<serde_json::Value>,
}
const LETTER_INTENT: &str = "letter-intent:";

impl Client {
    /// Persist the exact free letter before its first relay request.
    pub fn queue_free_letter(
        &self,
        id: &str,
        recipient: &str,
        text: &str,
        invitation: Option<String>,
    ) -> Result<()> {
        crate::upload::hex_id(id)?;
        recipient
            .parse::<dmsg_types::AccountId>()
            .map_err(|_| dmsg_types::invalid("recipient"))?;
        ensure_valid(!text.trim().is_empty() && text.len() <= 16 * 1024, "letter")?;
        let key = format!("{LETTER_INTENT}{id}");
        if let Some(old) = self.workspace.private::<LetterIntent>(self.keys()?, &key)? {
            return ensure(
                old.recipient == recipient && old.text == text && old.invitation == invitation,
                Error::IdempotencyConflict,
            );
        }
        self.workspace.put_private(
            self.keys()?,
            &key,
            &LetterIntent {
                recipient: recipient.into(),
                text: text.into(),
                invitation,
                result: None,
            },
        )
    }

    /// Pending letters, with only the metadata needed to resume them.
    pub fn pending_letters(&self) -> Result<Vec<serde_json::Value>> {
        let mut result = Vec::new();
        for key in self.workspace.store.sealed_keys(LETTER_INTENT)? {
            if let Some(letter) = self.workspace.private::<LetterIntent>(self.keys()?, &key)?
                && letter.result.is_none()
            {
                result.push(serde_json::json!({"id":key.trim_start_matches(LETTER_INTENT),"recipient":letter.recipient}));
            }
        }
        Ok(result)
    }
}

/// Submit a free letter with its original order ID. A paid quote is not approved.
pub async fn send_free_letter<H: Hold>(h: &H, id: &str) -> Result<serde_json::Value> {
    let h = crate::hold::Unlocked::new(h).await?;
    let key = format!("{LETTER_INTENT}{id}");
    let mut letter = h
        .with(|c| {
            c.workspace
                .private::<LetterIntent>(c.keys()?, &key)?
                .ok_or(Error::NotFound)
        })
        .await?;
    if let Some(result) = letter.result {
        return Ok(result);
    }
    let recipient = letter
        .recipient
        .parse()
        .map_err(|_| Error::IntegrityFailed)?;
    let result = crate::inbox::send(
        &h,
        &recipient,
        &letter.text,
        Some(id),
        letter.invitation.as_deref(),
        false,
    )
    .await?;
    letter.result = Some(result.clone());
    h.with(|c| c.workspace.put_private(c.keys()?, &key, &letter))
        .await?;
    Ok(result)
}
