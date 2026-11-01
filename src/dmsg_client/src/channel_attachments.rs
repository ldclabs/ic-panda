//! Verified, reconstructible attachment material. Separate from file-library copies.
use super::*;

pub(crate) const PREFIX: &str = "channel-file:";
pub(crate) const BUDGET: u64 = 256 * 1024 * 1024;
const ENTRIES: usize = 512;

#[derive(Serialize, Deserialize)]
pub(super) struct CachedAttachment {
    pub(super) plan: UploadPlan,
    pub(super) manifest: ByteBuf,
    uploader: String,
    at: u64,
}

impl Client {
    pub(crate) fn attachment_cache_status(&self) -> Result<serde_json::Value> {
        let (records, metadata) = self.workspace.store.sealed_usage(PREFIX)?;
        let mut bytes = metadata;
        for digest in self.attachment_chunks()? {
            bytes += self.workspace.store.chunk_size(&digest)?;
        }
        Ok(
            serde_json::json!({"kind":"attachments","records":records,"sealed_bytes":bytes,"budget_bytes":BUDGET}),
        )
    }

    pub(crate) fn clear_attachment_records(&self) -> Result<()> {
        for key in self.workspace.store.sealed_keys(PREFIX)? {
            self.workspace.store.delete_sealed(&key)?;
        }
        Ok(())
    }

    fn attachment_key(&self, channel: &str, reference: &FileRef) -> Result<String> {
        Ok(format!(
            "{PREFIX}{}",
            hex::encode(self.keys()?.derive(&(
                "dmsg/channel-file-cache/1",
                channel,
                &reference.upload_id,
                &reference.manifest_digest,
            )))
        ))
    }

    pub(super) fn cached_attachment(
        &self,
        channel: &str,
        reference: &FileRef,
        uploader: &str,
    ) -> Result<Option<CachedAttachment>> {
        let key = self.attachment_key(channel, reference)?;
        let record: Option<CachedAttachment> = self.workspace.private(self.keys()?, &key)?;
        if let Some(record) = &record {
            ensure(
                record.uploader == uploader
                    && of_file(&record.plan, reference)
                    && hex_sha256(&record.manifest) == reference.manifest_digest,
                Error::IntegrityFailed,
            )?;
        }
        Ok(record)
    }

    pub(crate) fn keep_attachment(
        &self,
        channel: &str,
        reference: &FileRef,
        uploader: &str,
        plan: &UploadPlan,
        manifest: &[u8],
    ) -> Result<()> {
        let key = self.attachment_key(channel, reference)?;
        self.workspace.put_private(
            self.keys()?,
            &key,
            &CachedAttachment {
                plan: plan.clone(),
                manifest: manifest.to_vec().into(),
                uploader: uploader.into(),
                at: now_ms(),
            },
        )?;
        let mut records = Vec::new();
        let mut bytes = 0;
        for saved in self.workspace.store.sealed_keys(PREFIX)? {
            if let Some(record) = self
                .workspace
                .private::<CachedAttachment>(self.keys()?, &saved)?
            {
                let size = record.plan.chunks.iter().map(|c| c.size).sum::<u64>()
                    + record.manifest.len() as u64;
                bytes += size;
                records.push((record.at, saved, size, record.plan.chunks));
            }
        }
        records.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        let mut count = records.len();
        let mut evicted = BTreeSet::new();
        for (_, old, size, chunks) in records {
            if bytes <= BUDGET && count <= ENTRIES {
                break;
            }
            if old == key {
                continue;
            }
            self.workspace.delete_private(&old)?;
            evicted.extend(chunks.into_iter().map(|c| c.digest));
            bytes -= size;
            count -= 1;
        }
        if !evicted.is_empty() {
            let active = self.attachment_chunks()?;
            evicted.retain(|digest| !active.contains(digest));
            self.workspace
                .store
                .delete_chunks(&self.disposable_chunks(evicted)?)?;
        }
        Ok(())
    }

    /// References kept through account-sync GC, including interrupted downloads.
    pub(crate) fn attachment_chunks(&self) -> Result<BTreeSet<String>> {
        let mut keep = BTreeSet::new();
        for key in self.workspace.store.sealed_keys(PREFIX)? {
            if let Some(record) = self
                .workspace
                .private::<CachedAttachment>(self.keys()?, &key)?
            {
                keep.extend(record.plan.chunks.into_iter().map(|c| c.digest));
            }
        }
        Ok(keep)
    }
}
