//! Durable, owner-reviewed file-link creation. Retries keep the original edition,
//! upload, share ID and expiry; the password never appears in a display record.
use super::*;

const INTENT: &str = "file-link-intent:";
const PUBLICATION: &str = "file-link-publication:";
const MAX_PENDING: usize = 64;

#[derive(Clone, Serialize, Deserialize)]
struct Intent {
    id: String,
    file: String,
    version: String,
    name: String,
    mime: String,
    size: u64,
    manifest: Option<FileManifest>,
    backup: Option<String>,
    expires_in_ms: u64,
    max_downloads: u64,
    password: Option<String>,
    protected: bool,
    show_profile: bool,
    created_at: u64,
    updated_at: u64,
    error: Option<String>,
    complete: bool,
}

impl Client {
    fn file_link_intent(&self, id: &str) -> Result<Intent> {
        hex_id(id)?;
        self.workspace
            .private(self.keys()?, &format!("{INTENT}{id}"))?
            .ok_or(Error::NotFound)
    }

    fn save_file_link_intent(&self, intent: &Intent) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{INTENT}{}", intent.id), intent)
    }

    fn file_link_attempt(&self, id: &str) -> Result<Option<LinkAttempt>> {
        self.workspace
            .private(self.keys()?, &format!("{PUBLICATION}{id}"))
    }

    fn file_link_intent_view(&self, intent: &Intent, now: u64) -> Result<Value> {
        let attempt = self.file_link_attempt(&intent.id)?;
        let link = attempt
            .as_ref()
            .filter(|a| a.published)
            .map(|a| {
                let saved: Option<Value> = self
                    .workspace
                    .private(self.keys()?, &format!("{LINK}{}", a.link.share_id))?;
                let current = saved
                    .map(LinkRecord::from_value)
                    .transpose()?
                    .unwrap_or(a.link.clone());
                Ok::<_, Error>(self.link_view(&current, now))
            })
            .transpose()?;
        let stage = match attempt.as_ref() {
            Some(a) if a.published => "complete",
            Some(a) if a.publishing => "checking",
            Some(_) => "uploading",
            None => "prepared",
        };
        Ok(json!({
            "id":intent.id,"file":intent.file,"version":intent.version,
            "name":intent.name,"mime":intent.mime,"size":intent.size,
            "backup":intent.backup,"expires_in_ms":intent.expires_in_ms,"max_downloads":intent.max_downloads,
            "password":intent.protected,"show_profile":intent.show_profile,"stage":stage,
            "created_at":intent.created_at,"updated_at":intent.updated_at,"error":intent.error,
            "can_discard":attempt.is_none(),"link":link,
        }))
    }

    fn prepare_link_intent(
        &mut self,
        manifest: FileManifest,
        backup: Option<String>,
        terms: LinkTerms<'_>,
    ) -> Result<Value> {
        self.keys()?;
        let expires_in_ms = terms.lifetime()?;
        ensure(manifest.size <= MAX_FILE, Error::QuotaExceeded)?;
        if let Some(password) = terms.password {
            ensure_valid(!password.is_empty() && password.len() <= 4096, "password")?;
        }
        let now = crate::now_ms();
        let mut pending = 0;
        for key in self.workspace.store.sealed_keys(INTENT)? {
            let id = &key[INTENT.len()..];
            let mut old = self.file_link_intent(id)?;
            if self.file_link_attempt(id)?.is_some_and(|a| a.published) && !old.complete {
                old.complete = true;
                old.password = None;
                old.manifest = None;
                old.error = None;
                self.save_file_link_intent(&old)?;
            }
            if old.complete && now.saturating_sub(old.updated_at) > crate::approval::RETAIN_MS {
                self.workspace.delete_private(&key)?;
                self.workspace
                    .delete_private(&format!("{PUBLICATION}{id}"))?;
            } else if !old.complete {
                pending += 1;
            }
        }
        ensure(pending < MAX_PENDING, Error::QuotaExceeded)?;
        let intent = Intent {
            id: random_hex(),
            file: manifest.id.clone(),
            version: manifest.version.clone(),
            name: manifest.name.clone(),
            mime: manifest.mime.clone(),
            size: manifest.size,
            manifest: Some(manifest),
            backup,
            expires_in_ms,
            max_downloads: terms.max_downloads,
            password: terms.password.map(str::to_owned),
            protected: terms.password.is_some(),
            show_profile: terms.show_profile,
            created_at: now,
            updated_at: now,
            error: None,
            complete: false,
        };
        self.save_file_link_intent(&intent)?;
        self.file_link_intent_view(&intent, now)
    }

    /// Freeze one file version and the chosen terms locally; nothing is uploaded.
    /// # Errors
    /// Refuses unavailable versions, invalid limits or an unavailable workspace.
    pub fn prepare_file_link_intent(
        &mut self,
        file: &str,
        version: &str,
        terms: LinkTerms<'_>,
    ) -> Result<Value> {
        let (_, record, manifest) = self.library_manifest(file, Some(version))?;
        let backup = if record.local {
            None
        } else {
            Some(record.backup.ok_or(Error::RecoveryIncomplete)?.upload_id)
        };
        self.prepare_link_intent(manifest, backup, terms)
    }

    /// Freeze a readable cloud backup without adding it to the local library.
    /// # Errors
    /// Refuses missing backups/keys and invalid link terms.
    pub fn prepare_backup_link_intent(
        &mut self,
        upload: &str,
        terms: LinkTerms<'_>,
    ) -> Result<Value> {
        let entry = self.cloud_entry(upload)?.ok_or(Error::NotFound)?;
        self.prepare_link_intent(
            entry.manifest.ok_or(Error::RecoveryIncomplete)?.file,
            Some(upload.into()),
            terms,
        )
    }

    /// List the original creations, with no password or unpublished link secret.
    /// # Errors
    /// Returns local workspace/record errors, never a fabricated empty result.
    pub fn file_link_intents(&self) -> Result<Vec<Value>> {
        let now = crate::now_ms();
        let mut intents = self
            .workspace
            .store
            .sealed_keys(INTENT)?
            .into_iter()
            .map(|key| self.file_link_intent(&key[INTENT.len()..]))
            .collect::<Result<Vec<_>>>()?;
        intents.sort_by_key(|i| std::cmp::Reverse(i.created_at));
        intents
            .iter()
            .map(|intent| self.file_link_intent_view(intent, now))
            .collect()
    }

    /// Discard a draft only while it has no remote publication attempt.
    /// # Errors
    /// A started/uncertain creation must be reconciled and its link revoked instead.
    pub fn discard_file_link_intent(&mut self, id: &str) -> Result<()> {
        self.file_link_intent(id)?;
        ensure(
            self.file_link_attempt(id)?.is_none(),
            Error::ExecutionUnknown,
        )?;
        self.workspace.delete_private(&format!("{INTENT}{id}"))
    }
}

/// Resume this exact creation. Hosts serialize calls for the same intent ID;
/// the mobile bridge uses its per-resource operation lane.
/// # Errors
/// Network uncertainty retains the original upload/share and never changes terms.
pub async fn run<H: Hold>(
    h: &H,
    id: &str,
    progress: &mut (dyn FnMut(&'static str, u64, u64) + Send),
) -> Result<Value> {
    let h = crate::hold::Unlocked::new(h).await?;
    run_inner(&h, id, progress).await
}

pub(super) async fn run_inner<H: Hold>(
    h: &H,
    id: &str,
    progress: &mut (dyn FnMut(&'static str, u64, u64) + Send),
) -> Result<Value> {
    let (intent, attempt) = h
        .with(|c| Ok((c.file_link_intent(id)?, c.file_link_attempt(id)?)))
        .await?;
    let publication = format!("{PUBLICATION}{id}");
    let result = match attempt {
        Some(attempt) => submit_link(h, attempt, Some(&publication), progress).await,
        None => {
            publish_link(
                h,
                intent.manifest.clone().ok_or(Error::IntegrityFailed)?,
                intent.backup.as_deref(),
                LinkTerms {
                    expires_in_ms: Some(intent.expires_in_ms),
                    max_downloads: intent.max_downloads,
                    password: intent.password.as_deref(),
                    show_profile: intent.show_profile,
                },
                progress,
                Some(&publication),
            )
            .await
        }
    };
    h.with(|c| {
        // Discarding an unstarted draft while its local preparation ran must
        // never revive it. A published link remains independently manageable.
        let mut current = c.file_link_intent(id)?;
        current.updated_at = crate::now_ms();
        current.error = result.as_ref().err().map(crate::error_code);
        if c.file_link_attempt(id)?.is_some() {
            current.password = None;
            current.manifest = None;
        }
        if result.is_ok() {
            current.complete = true;
        }
        c.save_file_link_intent(&current)?;
        result?;
        c.file_link_intent_view(&current, current.updated_at)
    })
    .await
}

/// Called inside the publication journal's transaction: a concurrently discarded
/// draft cannot acquire a remote upload after the person removed it.
pub(super) fn check_intent(client: &Client, key: &str) -> Result<()> {
    if let Some(id) = key.strip_prefix(PUBLICATION) {
        client.file_link_intent(id)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "link_jobs_tests.rs"]
mod tests;
