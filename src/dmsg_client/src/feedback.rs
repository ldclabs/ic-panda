//! Device-local encrypted feedback drafts, screenshot links and original-ID retries.
use crate::{
    Client,
    hold::{Hold, Unlocked},
    library::link::FEEDBACK_SHARE,
    upload::{hex_id, random_hex},
};
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};

/// The stable support account, independent of transferable handles.
pub const RECIPIENT: &str = "dbrt466u97gecq800000";
/// Leave space in the letter for its identifier, context and three links.
pub const MAX_TEXT: usize = 4500;
/// Screenshots per report.
pub const MAX_SCREENSHOTS: usize = 3;
/// Each screenshot may occupy at most 10 MiB before encryption.
pub const MAX_SCREENSHOT_BYTES: u64 = 10 * 1024 * 1024;
const DRAFT: &str = "feedback-draft";

/// A pinned local screenshot; links are created only after Send.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Screenshot {
    /// Library identity.
    pub file: String,
    /// Immutable library version.
    pub version: String,
    /// Display name.
    pub name: String,
    /// Plaintext length.
    pub size: u64,
    /// Verified library MIME type.
    pub mime: String,
    /// Durable sharing operation identity.
    pub operation: String,
    /// Complete encrypted share link, sealed at rest with the draft.
    pub url: Option<String>,
    /// Link expiry in Unix milliseconds.
    pub expires_at: Option<u64>,
}

/// The latest report on this device, retained after acceptance until New feedback.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feedback {
    /// Also the eventual letter ID.
    pub id: String,
    /// Local optimistic concurrency revision.
    pub revision: u64,
    /// Fixed support account.
    pub recipient: String,
    /// User-authored description.
    pub text: String,
    /// Previewed app version and platform, supplied by the native host.
    pub context: String,
    /// Selected immutable screenshots.
    pub screenshots: Vec<Screenshot>,
    /// `draft`, `pending`, or `sent`; pending is never reported as success.
    pub state: String,
    /// Safe error code from the last attempt.
    pub error: Option<String>,
    /// Whether no immutable contact request has been prepared yet.
    pub editable: bool,
}

impl Client {
    /// Read the account's encrypted feedback draft or most recent sent report.
    pub fn feedback(&self) -> Result<Option<Feedback>> {
        let mut value = self.workspace.private::<Feedback>(self.keys()?, DRAFT)?;
        if let Some(value) = &mut value {
            value.editable = value.state != "sent" && !self.inbox_has_outgoing(&value.id)?;
        }
        Ok(value)
    }

    /// Save only the exact draft revision the editor loaded. No network work.
    pub fn save_feedback(
        &mut self,
        id: &str,
        revision: u64,
        text: &str,
        context: &str,
        files: &[String],
    ) -> Result<Feedback> {
        hex_id(id)?;
        ensure_valid(
            text.len() <= 32 * 1024 && context.len() <= 300,
            "feedback text",
        )?;
        ensure_valid(files.len() <= MAX_SCREENSHOTS, "feedback screenshots")?;
        let old = self.feedback()?;
        // A local write may have committed before its IPC reply was lost.
        if let Some(value) = &old
            && value.id == id
            && value.state == "draft"
            && revision.checked_add(1) == Some(value.revision)
            && value.text == text
            && value.context == context
            && value.screenshots.iter().map(|s| &s.file).eq(files.iter())
        {
            return Ok(value.clone());
        }
        ensure(
            old.as_ref().map_or(revision == 0, |v| {
                v.id == id && v.revision == revision && v.state == "draft"
            }),
            Error::VersionConflict,
        )?;
        let mut screenshots = Vec::new();
        for file in files {
            ensure_valid(
                !screenshots.iter().any(|s: &Screenshot| s.file == *file),
                "duplicate screenshot",
            )?;
            if let Some(previous) = old
                .as_ref()
                .and_then(|v| v.screenshots.iter().find(|s| s.file == *file))
            {
                screenshots.push(previous.clone());
                continue;
            }
            let (_, local, manifest) = self.library_manifest(file, None)?;
            ensure_valid(
                local.local
                    && manifest.size <= MAX_SCREENSHOT_BYTES
                    && matches!(
                        manifest.mime.as_str(),
                        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
                    ),
                "feedback screenshot",
            )?;
            screenshots.push(Screenshot {
                file: file.clone(),
                version: manifest.version,
                name: manifest.name,
                size: manifest.size,
                mime: manifest.mime,
                operation: random_hex(),
                url: None,
                expires_at: None,
            });
        }
        let value = Feedback {
            id: id.into(),
            revision: revision + 1,
            recipient: RECIPIENT.into(),
            text: text.into(),
            context: context.into(),
            screenshots,
            state: "draft".into(),
            error: None,
            editable: true,
        };
        self.workspace.put_private(self.keys()?, DRAFT, &value)?;
        Ok(value)
    }

    /// Return to editing only before any contact request could have reached the relay.
    pub fn edit_feedback(&self, id: &str) -> Result<Feedback> {
        let mut value = self.feedback()?.ok_or(Error::NotFound)?;
        ensure(value.id == id && value.editable, Error::Pending)?;
        if value.state == "draft" {
            return Ok(value);
        }
        // A queued letter whose contact was never prepared returns to the report.
        match self.edit_unsent_letter(id) {
            Ok(()) => self.discard_letter_draft(id)?,
            Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
        let replan = value.error.as_deref() == Some("RESULT_EXPIRED");
        if replan {
            for screenshot in &mut value.screenshots {
                screenshot.operation = random_hex();
                screenshot.url = None;
                screenshot.expires_at = None;
            }
        }
        value.state = "draft".into();
        value.error = None;
        value.revision += 1;
        self.workspace.store.atomic(|_| {
            if replan {
                self.forget_feedback_shares()?;
            }
            self.workspace.put_private(self.keys()?, DRAFT, &value)
        })?;
        Ok(value)
    }

    /// Discard an unsent editor or start another report after confirmed acceptance.
    pub fn clear_feedback(&self, id: &str, revision: u64) -> Result<()> {
        let Some(value) = self.feedback()? else {
            return Ok(());
        };
        ensure(
            value.id == id && value.revision == revision && value.state != "pending",
            Error::VersionConflict,
        )?;
        self.workspace.store.atomic(|_| {
            self.forget_feedback_shares()?;
            self.workspace.delete_private(DRAFT)
        })
    }

    /// Screenshot share journals only serve retries of the current operations.
    fn forget_feedback_shares(&self) -> Result<()> {
        for key in self.workspace.store.sealed_keys(FEEDBACK_SHARE)? {
            self.workspace.delete_private(&key)?;
        }
        Ok(())
    }
}

fn letter(value: &Feedback) -> Result<String> {
    let mut text = format!(
        "dMsg feedback\nID: {}\n{}\n\n{}",
        value.id, value.context, value.text
    );
    for (index, screenshot) in value.screenshots.iter().enumerate() {
        text.push_str(&format!(
            "\n\nScreenshot {}: {}",
            index + 1,
            screenshot.url.as_deref().ok_or(Error::RecoveryIncomplete)?
        ));
    }
    ensure_valid(text.len() <= crate::inbox::MAX_LETTER, "feedback letter")?;
    Ok(text)
}

/// Freeze, share and send a free letter. Hosts serialize this account's feedback mutations.
pub async fn send<H: Hold>(h: &H, id: &str, revision: u64) -> Result<Feedback> {
    let h = Unlocked::new(h).await?;
    let mut value = h
        .with(|c| {
            let mut value = c.feedback()?.ok_or(Error::NotFound)?;
            ensure(
                value.id == id && value.revision == revision,
                Error::VersionConflict,
            )?;
            ensure_valid(
                value.text.len() <= MAX_TEXT
                    && (!value.text.trim().is_empty() || !value.screenshots.is_empty()),
                "feedback text",
            )?;
            if value.state != "sent" {
                value.state = "pending".into();
                value.error = None;
                c.workspace.put_private(c.keys()?, DRAFT, &value)?;
            }
            Ok(value)
        })
        .await?;
    if value.state == "sent" {
        return Ok(value);
    }
    let result = submit(&h, &mut value).await;
    if let Err(error) = &result {
        value.error = Some(crate::error_code(error));
    }
    // A session switch leaves the previously sealed operation available to retry.
    let saved = h
        .with(|c| {
            value.editable = value.state != "sent" && !c.inbox_has_outgoing(id)?;
            c.workspace.put_private(c.keys()?, DRAFT, &value)?;
            Ok(value.clone())
        })
        .await;
    result?;
    saved
}

async fn submit<H: Hold>(h: &H, value: &mut Feedback) -> Result<()> {
    // Once the contact is prepared, reconcile its original bytes even if a link expired.
    let prepared = h.with(|c| c.inbox_has_outgoing(&value.id)).await?;
    if !prepared {
        for index in 0..value.screenshots.len() {
            let screenshot = &value.screenshots[index];
            let link = crate::library::link::feedback_link(
                h,
                &screenshot.file,
                &screenshot.version,
                &screenshot.operation,
            )
            .await?;
            ensure(link.state == "active", Error::ResultExpired)?;
            value.screenshots[index].url = Some(link.url.ok_or(Error::IntegrityFailed)?);
            value.screenshots[index].expires_at = Some(link.expires_at);
            h.with(|c| c.workspace.put_private(c.keys()?, DRAFT, value))
                .await?;
        }
        let text = letter(value)?;
        h.with(|c| c.queue_free_letter(&value.id, RECIPIENT, &text, None))
            .await?;
    }
    let sent = crate::letters::send_free_letter(h, &value.id).await?;
    ensure(sent["state"] == "visible", Error::ExecutionUnknown)?;
    value.state = "sent".into();
    value.error = None;
    value.editable = false;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryStorage, TempDir, unlocked, warm};
    use std::sync::Arc;

    #[tokio::test]
    async fn encrypted_draft_survives_restart_and_rejects_stale_edits() {
        let dir = TempDir::new();
        let storage = Arc::new(MemoryStorage::default());
        let mut c = unlocked(&dir.0, false, Some(storage.clone()));
        c.enable_fast_unlock().unwrap();
        let id = random_hex();
        let saved = c
            .save_feedback(&id, 0, "Private feedback 私密反馈", "desktop", &[])
            .unwrap();
        assert_eq!(saved.recipient, RECIPIENT);
        let repeated = c
            .save_feedback(&id, 0, "Private feedback 私密反馈", "desktop", &[])
            .unwrap();
        assert_eq!(
            repeated.revision, saved.revision,
            "a lost save reply is idempotent"
        );
        assert!(matches!(
            c.save_feedback(&id, 0, "stale", "", &[]),
            Err(Error::VersionConflict)
        ));
        c.lock();
        assert!(matches!(c.feedback(), Err(Error::Locked)));
        drop(c);
        let mut c = Client::open(crate::testing::config(false), &dir.0, Some(storage)).unwrap();
        c.unlock_fast().await.unwrap();
        let saved = c.feedback().unwrap().unwrap();
        assert_eq!(saved.text, "Private feedback 私密反馈");
        assert_eq!(saved.id, id);
        c.clear_feedback(&id, saved.revision).unwrap();
        assert!(c.feedback().unwrap().is_none());
    }

    #[tokio::test]
    async fn offline_retry_keeps_id_and_text_and_can_return_to_editing_before_contact() {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        let id = random_hex();
        let draft = c
            .save_feedback(&id, 0, "A repeatable failure", "iOS", &[])
            .unwrap();
        for _ in 0..2 {
            assert!(send(&c.hold(), &id, draft.revision).await.is_err());
            let current = c.feedback().unwrap().unwrap();
            assert_eq!(current.state, "pending");
            assert_eq!(current.id, id);
            assert_eq!(current.text, draft.text);
            assert!(current.error.is_some());
            assert!(matches!(
                c.save_feedback(&id, current.revision, "late", "", &[]),
                Err(Error::VersionConflict)
            ));
        }
        assert!(c.feedback().unwrap().unwrap().editable);
        let edited = c.edit_feedback(&id).unwrap();
        assert_eq!(edited.state, "draft");
        assert_eq!(edited.revision, draft.revision + 1);
        assert!(c.letter_drafts().unwrap().is_empty());
    }

    #[tokio::test]
    async fn screenshot_is_pinned_and_upload_retry_reuses_sealed_plan() {
        let dir = TempDir::new();
        let storage = Arc::new(MemoryStorage::default());
        let mut c = unlocked(&dir.0, false, Some(storage.clone()));
        c.enable_fast_unlock().unwrap();
        warm(&mut c);
        let path = dir.path("screen.png");
        image::RgbImage::new(2, 2).save(&path).unwrap();
        let file = c.files_import(&path, "root", true, &mut |_, _| {}).unwrap();
        let id = random_hex();
        let draft = c
            .save_feedback(&id, 0, "", "desktop", std::slice::from_ref(&file.file.id))
            .unwrap();
        assert_eq!(draft.screenshots[0].version, file.version);
        assert!(
            c.workspace
                .store
                .sealed_keys("feedback-share:")
                .unwrap()
                .is_empty()
        );
        assert!(send(&c.hold(), &id, draft.revision).await.is_err());
        let keys = c.workspace.store.sealed_keys("feedback-share:").unwrap();
        assert_eq!(keys.len(), 1, "journal precedes network upload");
        let first: serde_json::Value = c
            .workspace
            .private(c.keys().unwrap(), &keys[0])
            .unwrap()
            .unwrap();
        drop(c);
        let mut c = Client::open(crate::testing::config(false), &dir.0, Some(storage)).unwrap();
        c.unlock_fast().await.unwrap();
        warm(&mut c);
        assert!(send(&c.hold(), &id, draft.revision).await.is_err());
        let second: serde_json::Value = c
            .workspace
            .private(c.keys().unwrap(), &keys[0])
            .unwrap()
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(
            c.workspace
                .store
                .sealed_keys("feedback-share:")
                .unwrap()
                .len(),
            1
        );
        assert!(
            c.letter_history().unwrap().is_empty(),
            "never send before screenshots are published"
        );
        // Replanning an expired upload forgets the journal of the old operation.
        let mut pending = c.feedback().unwrap().unwrap();
        pending.error = Some("RESULT_EXPIRED".into());
        c.workspace
            .put_private(c.keys().unwrap(), DRAFT, &pending)
            .unwrap();
        let replanned = c.edit_feedback(&id).unwrap();
        assert_ne!(
            replanned.screenshots[0].operation,
            pending.screenshots[0].operation
        );
        assert!(
            c.workspace
                .store
                .sealed_keys(FEEDBACK_SHARE)
                .unwrap()
                .is_empty()
        );
        // An ordinary failure keeps its journal; clearing the report forgets it.
        assert!(send(&c.hold(), &id, replanned.revision).await.is_err());
        let edited = c.edit_feedback(&id).unwrap();
        assert_eq!(
            edited.screenshots[0].operation,
            replanned.screenshots[0].operation
        );
        assert_eq!(
            c.workspace.store.sealed_keys(FEEDBACK_SHARE).unwrap().len(),
            1
        );
        c.clear_feedback(&id, edited.revision).unwrap();
        assert!(
            c.workspace
                .store
                .sealed_keys(FEEDBACK_SHARE)
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn validates_images_and_utf8_before_any_network_or_state_transition() {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        let path = dir.path("secret.txt");
        std::fs::write(&path, "not a screenshot").unwrap();
        let file = c.files_import(&path, "root", true, &mut |_, _| {}).unwrap();
        let id = random_hex();
        assert!(c.save_feedback(&id, 0, "", "", &[file.file.id]).is_err());
        let text = "信".repeat(1501);
        let saved = c.save_feedback(&id, 0, &text, "", &[]).unwrap();
        assert!(matches!(
            send(&c.hold(), &id, saved.revision).await,
            Err(Error::InvalidInput(_))
        ));
        assert_eq!(c.feedback().unwrap().unwrap().state, "draft");
        assert_eq!(c.feedback().unwrap().unwrap().text, text);
        let mut complete = saved;
        complete.text = "信".repeat(1500);
        assert!(letter(&complete).unwrap().len() <= crate::inbox::MAX_LETTER);
    }

    #[tokio::test]
    async fn prepared_report_is_immutable_and_only_visible_delivery_counts_as_sent() {
        use serde_json::json;
        for state in ["quoted", "visible"] {
            let dir = TempDir::new();
            let mut c = unlocked(&dir.0, false, None);
            let id = random_hex();
            let draft = c
                .save_feedback(&id, 0, "Original report", "desktop", &[])
                .unwrap();
            let body = letter(&draft).unwrap();
            c.queue_free_letter(&id, RECIPIENT, &body, None).unwrap();
            // A prepared outgoing letter and its already reconciled receipt.
            let key = format!("letter-intent:{id}");
            let mut intent: serde_json::Value = c
                .workspace
                .private(c.keys().unwrap(), &key)
                .unwrap()
                .unwrap();
            intent["result"] = json!({"state":state});
            c.workspace
                .put_private(c.keys().unwrap(), &key, &intent)
                .unwrap();
            c.workspace.put_private(c.keys().unwrap(), &format!("inbox-job:outgoing:{id}"), &json!({
                "recipient":RECIPIENT,"text_digest":"original","text":body,"sent_at":1,
                "payload":{"recipient":RECIPIENT,"order_id":id,"inbox_key_version":1,"ciphertext":"sealed-original","invitation":null}
            })).unwrap();
            assert!(!c.feedback().unwrap().unwrap().editable);
            assert!(matches!(c.edit_feedback(&id), Err(Error::Pending)));
            let result = send(&c.hold(), &id, draft.revision).await;
            if state == "visible" {
                assert_eq!(result.unwrap().state, "sent");
                assert_eq!(
                    send(&c.hold(), &id, draft.revision).await.unwrap().state,
                    "sent"
                );
            } else {
                assert!(matches!(result, Err(Error::ExecutionUnknown)));
                assert_eq!(c.feedback().unwrap().unwrap().state, "pending");
            }
            assert_eq!(c.inbox_outgoing().unwrap()[0]["text"], body);
            assert_eq!(c.inbox_outgoing().unwrap()[0]["paid"], false);
        }
    }
}
