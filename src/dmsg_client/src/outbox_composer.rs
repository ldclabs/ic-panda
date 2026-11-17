//! Full encrypted composer snapshots and atomic multi-attachment queueing.
use super::*;

const COMPOSER: &str = "composer-checkpoint:";
const RECEIPT: &str = "composer-receipt:";
const MAX_ATTACHMENTS: usize = 10;

/// A composer snapshot contains only immutable local references, never provider URIs.
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComposerDraft {
    /// Text accompanies the first attachment, or is sent on its own.
    pub text: String,
    /// Reviewed immutable file versions, in the person's selected order.
    pub attachments: Vec<Attachment>,
    /// The verified original message; also accompanies only the first attachment.
    pub reply_to: Option<channel::MessageRef>,
}

#[cfg(test)]
#[path = "outbox_composer_tests.rs"]
mod tests;

impl ComposerDraft {
    fn check(&self) -> Result<()> {
        ensure_valid(
            self.text.len() <= 16 * 1024 && self.attachments.len() <= MAX_ATTACHMENTS,
            "composer",
        )?;
        let mut seen = std::collections::BTreeSet::new();
        for file in &self.attachments {
            crate::upload::hex_id(&file.file)?;
            crate::upload::hex_id(&file.version)?;
            ensure_valid(
                file.name.len() <= 1024 && seen.insert((&file.file, &file.version)),
                "attachment",
            )?;
        }
        if let Some(reply) = &self.reply_to {
            ensure_valid(reply.seq > 0, "reply")?;
            crate::upload::hex_id(&reply.digest)?;
        }
        Ok(())
    }

    fn digest(&self) -> dmsg_types::Hash {
        dmsg_protocol::digest("dmsg/composer-intent/v2", self)
    }
}

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    stamp: DraftStamp,
    digest: dmsg_types::Hash,
    draft: Option<ComposerDraft>,
    queued: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct QueueReceipt {
    digest: dmsg_types::Hash,
    ids: Vec<String>,
    created_at: u64,
}

fn receipt_key(channel: &str, stamp: &DraftStamp) -> String {
    format!("{RECEIPT}{channel}:{}:{}", stamp.writer, stamp.revision)
}

impl Client {
    fn composer_checkpoint(&self, channel: &str) -> Result<Option<Checkpoint>> {
        self.workspace
            .private(self.keys()?, &format!("{COMPOSER}{channel}"))
    }

    fn composer_receipt(
        &self,
        channel: &str,
        stamp: &DraftStamp,
        digest: dmsg_types::Hash,
    ) -> Result<Option<Vec<String>>> {
        let receipt: Option<QueueReceipt> = self
            .workspace
            .private(self.keys()?, &receipt_key(channel, stamp))?;
        receipt
            .map(|receipt| {
                ensure(receipt.digest == digest, Error::IdempotencyConflict)?;
                Ok(receipt.ids)
            })
            .transpose()
    }

    /// Complete local drafts, including attachment/reply selections after restart.
    /// # Errors
    /// Refuses locked workspaces and unreadable snapshots.
    pub fn message_composers(&self) -> Result<Vec<serde_json::Value>> {
        let mut drafts = Vec::new();
        for key in self.workspace.store.sealed_keys(COMPOSER)? {
            let channel = &key[COMPOSER.len()..];
            if let Some(checkpoint) = self.composer_checkpoint(channel)?
                && let Some(draft) = checkpoint.draft
            {
                drafts.push(serde_json::json!({"channel":channel,"draft":draft}));
            }
        }
        Ok(drafts)
    }

    fn save_composer_inner(
        &self,
        channel: &str,
        draft: &ComposerDraft,
        stamp: &DraftStamp,
    ) -> Result<bool> {
        stamp.validate()?;
        draft.check()?;
        let digest = draft.digest();
        if self.composer_receipt(channel, stamp, digest)?.is_some() {
            return Ok(false);
        }
        if let Some(old) = self.composer_checkpoint(channel)?
            && old.stamp.writer == stamp.writer
        {
            if old.stamp.revision > stamp.revision {
                return Ok(false);
            }
            if old.stamp.revision == stamp.revision {
                ensure(old.digest == digest, Error::IdempotencyConflict)?;
                if !old.queued.is_empty() {
                    return Ok(false);
                }
            }
        }
        self.channel_draft(channel, &draft.text)?;
        self.workspace.put_private(
            self.keys()?,
            &format!("{COMPOSER}{channel}"),
            &Checkpoint {
                stamp: stamp.clone(),
                digest,
                draft: Some(draft.clone()),
                queued: vec![],
            },
        )?;
        Ok(true)
    }

    /// Save a complete authored revision; a consumed revision cannot revive a draft.
    /// # Errors
    /// Invalid/replayed revisions fail without partially replacing the previous draft.
    pub fn save_stamped_composer(
        &self,
        channel: &str,
        draft: &ComposerDraft,
        stamp: &DraftStamp,
    ) -> Result<bool> {
        self.workspace
            .store
            .atomic(|_| self.save_composer_inner(channel, draft, stamp))
    }

    /// Persist full authored drafts atomically. Unstamped legacy text writers still
    /// use the channel's text slot; they cannot overwrite a stamped revision.
    /// # Errors
    /// Refuses malformed drafts or a locked workspace.
    pub fn save_composer_drafts(
        &self,
        drafts: &[(&str, &ComposerDraft, Option<&DraftStamp>)],
    ) -> Result<()> {
        self.workspace.store.atomic(|_| {
            for (channel, draft, stamp) in drafts {
                if let Some(stamp) = stamp {
                    self.save_composer_inner(channel, draft, stamp)?;
                } else {
                    ensure_valid(
                        draft.attachments.is_empty() && draft.reply_to.is_none(),
                        "draft stamp",
                    )?;
                    self.channel_draft(channel, &draft.text)?;
                }
            }
            Ok(())
        })
    }

    /// Persist a batch of text-only drafts.
    /// # Errors
    /// As [`Self::save_composer_drafts`].
    pub fn save_message_drafts(&self, drafts: &[(String, String)]) -> Result<()> {
        self.save_stamped_message_drafts(
            &drafts
                .iter()
                .map(|(channel, text)| (channel.as_str(), text.as_str(), None))
                .collect::<Vec<_>>(),
        )
    }

    /// Persist legacy text writers using the same authored-revision fence.
    /// # Errors
    /// As [`Self::save_composer_drafts`].
    pub fn save_stamped_message_drafts(
        &self,
        drafts: &[(&str, &str, Option<&DraftStamp>)],
    ) -> Result<()> {
        let content: Vec<_> = drafts
            .iter()
            .map(|(_, text, _)| ComposerDraft {
                text: (*text).into(),
                ..Default::default()
            })
            .collect();
        self.save_composer_drafts(
            &drafts
                .iter()
                .zip(&content)
                .map(|((channel, _, stamp), draft)| (*channel, draft, *stamp))
                .collect::<Vec<_>>(),
        )
    }

    /// Persist one text-only authored revision.
    /// # Errors
    /// As [`Self::save_stamped_composer`].
    pub fn save_stamped_draft(
        &self,
        channel: &str,
        text: &str,
        stamp: &DraftStamp,
    ) -> Result<bool> {
        self.save_stamped_composer(
            channel,
            &ComposerDraft {
                text: text.into(),
                ..Default::default()
            },
            stamp,
        )
    }

    /// Atomically queue one message per attachment, with text/reply on the first.
    /// The complete ordered ID list is returned again after a lost queue reply,
    /// even if some messages have already been delivered and removed from outbox.
    /// # Errors
    /// Rejects stale revisions, unavailable versions/replies or insufficient queue room.
    pub fn queue_stamped_composer(
        &mut self,
        channel: &str,
        draft: &ComposerDraft,
        stamp: &DraftStamp,
    ) -> Result<Vec<String>> {
        stamp.validate()?;
        draft.check()?;
        let digest = draft.digest();
        if let Some(ids) = self.composer_receipt(channel, stamp, digest)? {
            return Ok(ids);
        }
        if let Some(old) = self.composer_checkpoint(channel)?
            && old.stamp.writer == stamp.writer
        {
            ensure(old.stamp.revision <= stamp.revision, Error::VersionConflict)?;
            if old.stamp.revision == stamp.revision {
                ensure(old.digest == digest, Error::IdempotencyConflict)?;
                if !old.queued.is_empty() {
                    return Ok(old.queued);
                }
            }
        }
        self.channel_view(channel)?;
        ensure_valid(
            !draft.text.trim().is_empty() || !draft.attachments.is_empty(),
            "message",
        )?;
        let count = draft.attachments.len().max(1);
        ensure(
            self.message_intents(channel)?.len() + count <= LIMIT,
            Error::QuotaExceeded,
        )?;
        if let Some(reference) = &draft.reply_to {
            let original =
                self.channel_messages_before(channel, reference.seq.saturating_add(1), 1)?;
            ensure(
                original
                    .first()
                    .is_some_and(|m| m.seq == reference.seq && m.digest == reference.digest),
                Error::VersionConflict,
            )?;
        }
        for file in &draft.attachments {
            let (_, version, manifest) = self.library_manifest(&file.file, Some(&file.version))?;
            ensure(version.local, Error::NotFound)?;
            ensure(
                manifest.size <= crate::files::MAX_FILE,
                Error::QuotaExceeded,
            )?;
        }
        let now = crate::now_ms();
        let batch = hex::encode(crate::crypto::random_id());
        let intents: Vec<_> = (0..count)
            .map(|index| Intent {
                channel: channel.into(),
                id: hex::encode(crate::crypto::random_id()),
                text: if index == 0 {
                    draft.text.clone()
                } else {
                    String::new()
                },
                reply_to: if index == 0 {
                    draft.reply_to.clone()
                } else {
                    None
                },
                edit_of: None,
                retract: None,
                attachment: draft.attachments.get(index).cloned(),
                created_at: now,
                batch: Some((batch.clone(), index as u32)),
                attempted: false,
                error: None,
            })
            .collect();
        let ids: Vec<_> = intents.iter().map(|intent| intent.id.clone()).collect();
        self.workspace.store.atomic(|_| {
            // A newer editor must not erase an older queue's acknowledgement.
            // Keep receipts while any original outbox entry still exists.
            for key in self
                .workspace
                .store
                .sealed_keys(&format!("{RECEIPT}{channel}:"))?
            {
                if let Some(old) = self.workspace.private::<QueueReceipt>(self.keys()?, &key)?
                    && now.saturating_sub(old.created_at) > crate::approval::RETAIN_MS
                {
                    let mut pending = false;
                    for id in &old.ids {
                        if self
                            .workspace
                            .private::<Intent>(self.keys()?, &format!("{PREFIX}{channel}:{id}"))?
                            .is_some()
                        {
                            pending = true;
                            break;
                        }
                    }
                    if !pending {
                        self.workspace.delete_private(&key)?;
                    }
                }
            }
            for intent in &intents {
                self.workspace
                    .put_private(self.keys()?, &intent.key(), intent)?;
            }
            self.channel_draft(channel, "")?;
            self.workspace.put_private(
                self.keys()?,
                &receipt_key(channel, stamp),
                &QueueReceipt {
                    digest,
                    ids: ids.clone(),
                    created_at: now,
                },
            )?;
            self.workspace.put_private(
                self.keys()?,
                &format!("{COMPOSER}{channel}"),
                &Checkpoint {
                    stamp: stamp.clone(),
                    digest,
                    draft: None,
                    queued: ids.clone(),
                },
            )
        })?;
        Ok(ids)
    }

    /// Queue a single legacy stamped send through the same atomic batch path.
    /// # Errors
    /// As [`Self::queue_stamped_composer`].
    pub fn queue_stamped_message(
        &mut self,
        channel: &str,
        text: &str,
        stamp: &DraftStamp,
        file: Option<(&str, &str)>,
        reply: Option<channel::MessageRef>,
    ) -> Result<String> {
        let attachments = match file {
            Some((file, version)) => vec![Attachment {
                file: file.into(),
                version: version.into(),
                name: self.files_show(file)?.file.name,
            }],
            None => vec![],
        };
        Ok(self
            .queue_stamped_composer(
                channel,
                &ComposerDraft {
                    text: text.into(),
                    attachments,
                    reply_to: reply,
                },
                stamp,
            )?
            .remove(0))
    }
}
