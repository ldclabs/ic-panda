//! Account-local, encrypted letter drafts and send history. These records are
//! retained on this device; they are not part of account content replication.
use crate::{Client, hold::Hold};
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const INTENT: &str = "letter-intent:";
const DRAFT: &str = "letter-draft:";
const STATUS: &str = "letter-status:";

/// An unfinished letter, readable only in its unlocked account workspace.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LetterDraft {
    /// Stable local draft and eventual send ID.
    pub id: String,
    /// May be incomplete until the person sends.
    pub recipient: String,
    /// User-authored plaintext, sealed at rest.
    pub text: String,
    /// An optional contact invitation, not a channel invitation.
    pub invitation: Option<String>,
    /// Unix milliseconds.
    pub updated_at: u64,
}

#[derive(Serialize, Deserialize)]
struct LetterIntent {
    recipient: String,
    text: String,
    invitation: Option<String>,
    result: Option<Value>,
    #[serde(default)]
    created_at: u64,
    #[serde(default)]
    attempted: bool,
    #[serde(default)]
    error: Option<String>,
}

impl Client {
    /// Save partial input without preparing a network request.
    pub fn save_letter_draft(
        &self,
        id: &str,
        recipient: &str,
        text: &str,
        invitation: Option<String>,
    ) -> Result<()> {
        self.save_letter_draft_at(id, recipient, text, invitation, crate::now_ms())
    }

    /// Save a draft using the host's single timestamp for a local batch.
    pub fn save_letter_draft_at(
        &self,
        id: &str,
        recipient: &str,
        text: &str,
        invitation: Option<String>,
        now: u64,
    ) -> Result<()> {
        crate::upload::hex_id(id)?;
        ensure_valid(
            recipient.len() <= 200
                && text.len() <= 32 * 1024
                && invitation.as_ref().is_none_or(|s| s.len() <= 256),
            "letter draft",
        )?;
        // A stale editor cannot recreate a draft already moved to the outbox.
        ensure(
            self.workspace
                .private::<LetterIntent>(self.keys()?, &format!("{INTENT}{id}"))?
                .is_none(),
            Error::VersionConflict,
        )?;
        let key = format!("{DRAFT}{id}");
        if recipient.is_empty()
            && text.is_empty()
            && invitation.as_ref().is_none_or(String::is_empty)
        {
            return self.workspace.delete_private(&key);
        }
        let draft = LetterDraft {
            id: id.into(),
            recipient: recipient.into(),
            text: text.into(),
            invitation,
            updated_at: now,
        };
        self.workspace.put_private(self.keys()?, &key, &draft)
    }

    /// Read encrypted drafts on this device, newest first.
    pub fn letter_drafts(&self) -> Result<Vec<LetterDraft>> {
        let keys = self.keys()?;
        let mut drafts = Vec::new();
        for key in self.workspace.store.sealed_keys(DRAFT)? {
            if let Some(draft) = self.workspace.private::<LetterDraft>(keys, &key)? {
                drafts.push(draft);
            }
        }
        drafts.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
        Ok(drafts)
    }

    /// Explicitly discard only an unfinished local draft.
    pub fn discard_letter_draft(&self, id: &str) -> Result<()> {
        self.keys()?;
        crate::upload::hex_id(id)?;
        self.workspace.delete_private(&format!("{DRAFT}{id}"))
    }

    /// Validate and seal the exact original letter before its first request.
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
        ensure_valid(
            !text.trim().is_empty() && text.len() <= crate::inbox::MAX_LETTER,
            "letter",
        )?;
        ensure_valid(
            invitation
                .as_deref()
                .is_none_or(crate::channel_log::is_hash),
            "invitation",
        )?;
        let key = format!("{INTENT}{id}");
        if let Some(old) = self.workspace.private::<LetterIntent>(self.keys()?, &key)? {
            return ensure(
                old.recipient == recipient && old.text == text && old.invitation == invitation,
                Error::IdempotencyConflict,
            );
        }
        let letter = LetterIntent {
            recipient: recipient.into(),
            text: text.into(),
            invitation,
            result: None,
            created_at: crate::now_ms(),
            attempted: false,
            error: None,
        };
        self.workspace.store.atomic(|_| {
            self.workspace.put_private(self.keys()?, &key, &letter)?;
            self.workspace.delete_private(&format!("{DRAFT}{id}"))
        })
    }

    /// Legacy pending-list API, preserving its original fields.
    pub fn pending_letters(&self) -> Result<Vec<Value>> {
        Ok(self
            .letter_history()?
            .into_iter()
            .filter(|v| v["pending"] == true)
            .collect())
    }

    /// Sent and pending letters, including older sender journals without a body.
    pub fn letter_history(&self) -> Result<Vec<Value>> {
        let keys = self.keys()?;
        let mut rows = std::collections::BTreeMap::new();
        for outgoing in self.inbox_outgoing()? {
            let id = outgoing["order_id"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?;
            let status = self
                .workspace
                .private::<Value>(keys, &format!("{STATUS}{id}"))?;
            rows.insert(id.to_string(), json!({"id":id,"recipient":outgoing["recipient"],"text":null,"created_at":status.as_ref().and_then(|v| v["created_at"].as_u64()),"state":status.as_ref().and_then(|v| v["state"].as_str()).unwrap_or("unknown"),"error":null,"pending":false,"editable":false}));
        }
        for key in self.workspace.store.sealed_keys(INTENT)? {
            let letter = self
                .workspace
                .private::<LetterIntent>(keys, &key)?
                .ok_or(Error::IntegrityFailed)?;
            let id = &key[INTENT.len()..];
            let latest = self
                .workspace
                .private::<Value>(keys, &format!("{STATUS}{id}"))?;
            let result = latest.as_ref().or(letter.result.as_ref());
            let state = result.and_then(|v| v["state"].as_str()).unwrap_or(
                if letter.error.as_deref() == Some("EXECUTION_UNKNOWN") {
                    "unconfirmed"
                } else if letter.error.is_some() {
                    "failed"
                } else if letter.attempted {
                    "unconfirmed"
                } else {
                    "queued"
                },
            );
            rows.insert(id.into(), json!({"id":id,"recipient":letter.recipient,"text":letter.text,"created_at":if letter.created_at > 0 { Some(letter.created_at) } else { result.and_then(|v| v["created_at"].as_u64()) },"state":state,"error":letter.error,"pending":result.is_none(),"editable":result.is_none() && !self.inbox_has_outgoing(id)?}));
        }
        let mut rows: Vec<_> = rows.into_values().collect();
        rows.sort_by(|a, b| {
            b["created_at"]
                .as_u64()
                .cmp(&a["created_at"].as_u64())
                .then(a["id"].as_str().cmp(&b["id"].as_str()))
        });
        Ok(rows)
    }

    /// Restore an intent to editing only when no contact request was prepared.
    pub fn edit_unsent_letter(&self, id: &str) -> Result<()> {
        crate::upload::hex_id(id)?;
        ensure(!self.inbox_has_outgoing(id)?, Error::Pending)?;
        let key = format!("{INTENT}{id}");
        let letter = self
            .workspace
            .private::<LetterIntent>(self.keys()?, &key)?
            .ok_or(Error::NotFound)?;
        ensure(letter.result.is_none(), Error::Forbidden)?;
        let draft = LetterDraft {
            id: id.into(),
            recipient: letter.recipient,
            text: letter.text,
            invitation: letter.invitation,
            updated_at: crate::now_ms(),
        };
        self.workspace.store.atomic(|_| {
            self.workspace
                .put_private(self.keys()?, &format!("{DRAFT}{id}"), &draft)?;
            self.workspace.delete_private(&key)
        })
    }
}

/// Submit the same original ID and bytes. The caller serializes this order.
pub async fn send_free_letter<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let h = crate::hold::Unlocked::new(h).await?;
    let key = format!("{INTENT}{id}");
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
    letter.attempted = true;
    h.with(|c| c.workspace.put_private(c.keys()?, &key, &letter))
        .await?;
    let result = crate::inbox::send(
        &h,
        &recipient,
        &letter.text,
        Some(id),
        letter.invitation.as_deref(),
        false,
    )
    .await;
    match &result {
        Ok(value) => {
            letter.result = Some(value.clone());
            letter.error = None;
        }
        Err(error) => {
            letter.error = Some(crate::error_code(error));
        }
    }
    // A locked or switched workspace retains the previously sealed intent.
    let saved = h
        .with(|c| c.workspace.put_private(c.keys()?, &key, &letter))
        .await;
    if result.is_ok() {
        saved?;
    }
    result
}

/// Query and retain verified sender status without sending another letter.
pub async fn refresh<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let h = crate::hold::Unlocked::new(h).await?;
    let recipient = h
        .with(|c| {
            let row = c
                .letter_history()?
                .into_iter()
                .find(|v| v["id"] == id)
                .ok_or(Error::NotFound)?;
            row["recipient"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?
                .parse()
                .map_err(|_| Error::IntegrityFailed)
        })
        .await?;
    let result = crate::inbox::order(&h, &recipient, id).await?;
    h.with(|c| {
        c.workspace
            .put_private(c.keys()?, &format!("{STATUS}{id}"), &result)
    })
    .await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryStorage, TempDir, unlocked};
    use std::sync::Arc;

    const RECIPIENT: &str = "040g2082040g2082040g";

    #[tokio::test]
    async fn draft_survives_restart_and_moves_atomically_to_history() {
        let dir = TempDir::new();
        let storage = Arc::new(MemoryStorage::default());
        let mut c = unlocked(&dir.0, false, Some(storage.clone()));
        let id = "a1".repeat(32);
        c.enable_fast_unlock().unwrap();
        c.save_letter_draft(&id, RECIPIENT, "Private draft", None)
            .unwrap();
        drop(c);
        let mut c = Client::open(crate::testing::config(false), &dir.0, Some(storage)).unwrap();
        c.unlock_fast().await.unwrap();
        assert_eq!(c.letter_drafts().unwrap()[0].text, "Private draft");
        c.queue_free_letter(&id, RECIPIENT, "Private draft", None)
            .unwrap();
        assert!(c.letter_drafts().unwrap().is_empty());
        let row = c.letter_history().unwrap().remove(0);
        assert_eq!(row["state"], "queued");
        assert_eq!(row["text"], "Private draft");
        assert!(row["created_at"].as_u64().unwrap() > 0);
        assert!(matches!(
            c.save_letter_draft(&id, RECIPIENT, "late editor", None),
            Err(Error::VersionConflict)
        ));
        c.lock();
        assert!(matches!(c.letter_drafts(), Err(Error::Locked)));
        assert!(matches!(c.letter_history(), Err(Error::Locked)));
    }

    #[test]
    fn validates_utf8_bytes_and_invitation_before_consuming_a_draft() {
        let dir = TempDir::new();
        let c = unlocked(&dir.0, false, None);
        let id = "b2".repeat(32);
        let oversized = "信".repeat(2001);
        c.save_letter_draft(&id, RECIPIENT, &oversized, None)
            .unwrap();
        assert!(matches!(
            c.queue_free_letter(&id, RECIPIENT, &oversized, None),
            Err(Error::InvalidInput(_))
        ));
        assert_eq!(c.letter_drafts().unwrap()[0].text, oversized);
        assert!(c.letter_history().unwrap().is_empty());
        assert!(
            c.queue_free_letter(&id, RECIPIENT, "hello", Some("invalid".into()))
                .is_err()
        );
        c.queue_free_letter(&id, RECIPIENT, &"信".repeat(2000), None)
            .unwrap();
        assert!(matches!(
            c.queue_free_letter(&id, RECIPIENT, "different text", None),
            Err(Error::IdempotencyConflict)
        ));
    }

    #[test]
    fn sent_body_is_retained_and_only_unprepared_intents_can_be_edited() {
        let dir = TempDir::new();
        let c = unlocked(&dir.0, false, None);
        let id = "c3".repeat(32);
        c.queue_free_letter(&id, RECIPIENT, "Exact letter", None)
            .unwrap();
        c.edit_unsent_letter(&id).unwrap();
        assert!(c.letter_history().unwrap().is_empty());
        assert_eq!(c.letter_drafts().unwrap()[0].text, "Exact letter");
        c.queue_free_letter(&id, RECIPIENT, "Exact letter", None)
            .unwrap();
        let key = format!("{INTENT}{id}");
        let mut letter = c
            .workspace
            .private::<LetterIntent>(c.keys().unwrap(), &key)
            .unwrap()
            .unwrap();
        letter.attempted = true;
        letter.error = Some("EXECUTION_UNKNOWN".into());
        c.workspace
            .put_private(c.keys().unwrap(), &key, &letter)
            .unwrap();
        assert_eq!(c.letter_history().unwrap()[0]["state"], "unconfirmed");
        letter.result = Some(json!({"state":"visible"}));
        c.workspace
            .put_private(c.keys().unwrap(), &key, &letter)
            .unwrap();
        assert!(c.pending_letters().unwrap().is_empty());
        assert_eq!(c.letter_history().unwrap()[0]["text"], "Exact letter");
        assert_eq!(c.letter_history().unwrap()[0]["state"], "visible");
        assert!(c.edit_unsent_letter(&id).is_err());
    }
}
