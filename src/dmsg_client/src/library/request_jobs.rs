//! Local creation intents retain a request's original key, terms and command.
//! A lost reply is resolved by reading that request, never by making another.
use super::*;
use serde_json::Value;

const INTENT: &str = "file-request-intent:";
const MAX_PENDING: usize = 200;

#[derive(Clone, Serialize, Deserialize)]
struct Publication {
    request_id: String,
    expires_at: u64,
    max_total_bytes: u64,
    token_hash: String,
    descriptor: String,
    management: ManagementEnvelope,
}

#[derive(Clone, Serialize, Deserialize)]
struct Intent {
    id: String,
    batch: String,
    position: u32,
    title: String,
    instructions: String,
    label: String,
    expires_in_ms: u64,
    max_total_bytes: Option<u64>,
    command: Hash,
    publication: Option<Publication>,
    complete: bool,
    created_at: u64,
    updated_at: u64,
    error: Option<String>,
}

impl Client {
    fn request_intent(&self, id: &str) -> Result<Intent> {
        hex_id(id)?;
        self.workspace
            .private(self.keys()?, &format!("{INTENT}{id}"))?
            .ok_or(Error::NotFound)
    }

    fn save_request_intent(&self, intent: &Intent) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{INTENT}{}", intent.id), intent)
    }

    fn request_intent_view(&self, intent: &Intent, now: u64) -> Result<Value> {
        let link = if intent.complete {
            Some(
                self.request_view(
                    &self
                        .request_record(&intent.id)?
                        .ok_or(Error::IntegrityFailed)?,
                    now,
                )?,
            )
        } else {
            None
        };
        Ok(json!({
            "id":intent.id,"batch":intent.batch,"position":intent.position,"title":intent.title,
            "instructions":intent.instructions,"label":intent.label,
            "expires_in_ms":intent.expires_in_ms,"max_total_bytes":intent.max_total_bytes,
            "stage":if intent.complete {"complete"} else if intent.publication.is_some() {"checking"} else {"prepared"},
            "can_discard":intent.publication.is_none(),"link":link,
            "created_at":intent.created_at,"updated_at":intent.updated_at,"error":intent.error,
        }))
    }

    /// Freeze all reviewed recipients in one local transaction. Empty labels
    /// create one unlabelled link; no remote publication happens here.
    /// # Errors
    /// Refuses invalid text/terms, more than 50 labels or too many pending jobs.
    pub fn prepare_file_request_intents(
        &mut self,
        title: &str,
        instructions: &str,
        labels: &[String],
        terms: RequestTerms,
    ) -> Result<Vec<Value>> {
        self.keys()?;
        let lifetime = terms.expires_in_ms.unwrap_or(LIFETIME_MS);
        ensure_valid(lifetime > 0 && lifetime <= MAX_LIFETIME_MS, "expires_in_ms")?;
        if let Some(total) = terms.max_total_bytes {
            ensure_valid(total >= MIN_TOTAL, "max_total_bytes")?;
        }
        ensure_valid(labels.len() <= 50, "labels")?;
        let labels = if labels.is_empty() {
            vec![String::new()]
        } else {
            labels.to_vec()
        };
        for label in &labels {
            check_draft(&RequestDraft {
                title,
                instructions,
                label,
            })?;
        }
        let now = now_ms();
        let batch = random_hex();
        self.workspace.store.atomic(|_| {
            let mut pending = 0;
            for key in self.workspace.store.sealed_keys(INTENT)? {
                let old = self.request_intent(&key[INTENT.len()..])?;
                if old.complete && now.saturating_sub(old.updated_at) > crate::approval::RETAIN_MS {
                    self.workspace.delete_private(&key)?;
                } else if !old.complete {
                    pending += 1;
                }
            }
            ensure(pending + labels.len() <= MAX_PENDING, Error::QuotaExceeded)?;
            labels
                .into_iter()
                .enumerate()
                .map(|(position, label)| {
                    let intent = Intent {
                        id: random_hex(),
                        batch: batch.clone(),
                        position: position as u32,
                        title: title.into(),
                        instructions: instructions.into(),
                        label,
                        expires_in_ms: lifetime,
                        max_total_bytes: terms.max_total_bytes,
                        command: Hash::new(random_id()),
                        publication: None,
                        complete: false,
                        created_at: now,
                        updated_at: now,
                        error: None,
                    };
                    self.save_request_intent(&intent)?;
                    self.request_intent_view(&intent, now)
                })
                .collect()
        })
    }

    /// Original creations, without unpublished page keys or HPKE secrets.
    /// # Errors
    /// Returns local workspace/record errors.
    pub fn file_request_intents(&self) -> Result<Vec<Value>> {
        let now = now_ms();
        let mut intents = self
            .workspace
            .store
            .sealed_keys(INTENT)?
            .into_iter()
            .map(|key| self.request_intent(&key[INTENT.len()..]))
            .collect::<Result<Vec<_>>>()?;
        intents.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.batch.cmp(&b.batch))
                .then(a.position.cmp(&b.position))
        });
        intents
            .iter()
            .map(|i| self.request_intent_view(i, now))
            .collect()
    }

    /// Remove only an intent that has never acquired a publication command.
    /// # Errors
    /// An uncertain publication must be checked and closed instead.
    pub fn discard_file_request_intent(&mut self, id: &str) -> Result<()> {
        ensure(
            self.request_intent(id)?.publication.is_none(),
            Error::ExecutionUnknown,
        )?;
        self.workspace.delete_private(&format!("{INTENT}{id}"))
    }

    fn prepare_request_publication(&mut self, id: &str, total: u64) -> Result<Intent> {
        let mut intent = self.request_intent(id)?;
        ensure(intent.publication.is_none(), Error::ExecutionUnknown)?;
        self.root_now()?;
        ensure_valid(total >= MIN_TOTAL, "max_total_bytes")?;
        let account = self.account()?.id.to_string();
        let page_key = Zeroizing::new(random_key());
        let seed = Zeroizing::new(random_key());
        let secrets = RequestSecrets {
            hpke_seed: b64(&seed[..]),
            page_key: b64(&page_key[..]),
            label: intent.label.clone(),
            title: intent.title.clone(),
            instructions: intent.instructions.clone(),
        };
        let descriptor = seal_descriptor(
            &page_key,
            &RequestDescriptor {
                format: REQUEST_FORMAT.into(),
                account: account.clone(),
                request_id: id.into(),
                hpke_pub: b64(&hpke_public(&seed)),
                title: intent.title.clone(),
                instructions: intent.instructions.clone(),
            },
        );
        ensure_valid(descriptor.len() <= MAX_DESCRIPTOR, "instructions")?;
        let now = now_ms();
        intent.publication = Some(Publication {
            request_id: id.into(),
            expires_at: now + intent.expires_in_ms.min(MAX_LIFETIME_MS - MINUTE),
            max_total_bytes: total,
            token_hash: token_hash(&upload_token(&page_key, &account, id)),
            descriptor,
            management: self.seal_request_secrets(id, &secrets)?,
        });
        intent.max_total_bytes = Some(total);
        intent.updated_at = now;
        self.save_request_intent(&intent)?;
        Ok(intent)
    }

    fn settle_request_intent(&mut self, id: &str, remote: RemoteLink) -> Result<Value> {
        let mut intent = self.request_intent(id)?;
        let original = intent.publication.as_ref().ok_or(Error::IntegrityFailed)?;
        ensure(
            remote.request_id == id
                && remote.expires_at == original.expires_at
                && remote.max_total_bytes == original.max_total_bytes
                && remote.management == original.management,
            Error::IntegrityFailed,
        )?;
        let now = now_ms();
        self.workspace.store.atomic(|_| {
            self.keep_link(remote, now)?;
            intent.complete = true;
            intent.updated_at = now;
            intent.error = None;
            self.save_request_intent(&intent)
        })?;
        self.request_intent_view(&intent, now)
    }
}

async fn find_original<H: Hold>(h: &H, base: &str, id: &str) -> Result<Option<RemoteLink>> {
    let mut after = 0;
    loop {
        let page: Page<RemoteLink> = parse(
            h.get(&format!("{base}/links?after={after}&limit=100"))
                .await?,
        )?;
        if let Some(remote) = page.entries.into_iter().find(|r| r.request_id == id) {
            return Ok(Some(remote));
        }
        if page.end {
            return Ok(None);
        }
        ensure(page.next_cursor > after, Error::IntegrityFailed)?;
        after = page.next_cursor;
    }
}

/// Resume one original creation; the host serializes run/discard by intent ID.
/// # Errors
/// Keeps uncertain requests for reconciliation; refuses a changed root instead
/// of silently creating another key, expiry or link.
pub async fn run<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let h = Unlocked::new(h).await?;
    run_inner(&h, id).await
}

async fn run_inner<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let result = publish(h, id).await;
    if let Err(error) = &result {
        h.with(|c| {
            let mut current = c.request_intent(id)?;
            if !current.complete {
                current.error = Some(crate::error_code(error));
                current.updated_at = now_ms();
                c.save_request_intent(&current)?;
            }
            Ok(())
        })
        .await?;
    }
    result
}

async fn publish<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let (mut intent, base, account_base) = h
        .with(|c| Ok((c.request_intent(id)?, c.requests_base()?, c.base()?)))
        .await?;
    if intent.complete {
        return h.with(|c| c.request_intent_view(&intent, now_ms())).await;
    }
    if intent.publication.is_some() {
        if let Some(remote) = find_original(h, &base, id).await? {
            return h.with(|c| c.settle_request_intent(id, remote)).await;
        }
    } else {
        let total = match intent.max_total_bytes {
            Some(total) => total,
            None => {
                let quota = h.get(&format!("{account_base}/quota")).await?;
                let lifecycle = &quota["lifecycle"];
                let wanted = if lifecycle["backup_max_file_bytes"]
                    .as_u64()
                    .is_some_and(|n| n > PLAIN_FILE_LIMIT)
                {
                    DEFAULT_TOTAL_MAX
                } else {
                    DEFAULT_TOTAL
                };
                lifecycle["storage_limit"]
                    .as_u64()
                    .map_or(wanted, |limit| wanted.min(limit))
            }
        };
        intent = h
            .signed(|c| c.prepare_request_publication(id, total))
            .await?
            .1;
    }
    let publication = intent.publication.ok_or(Error::IntegrityFailed)?;
    // The generation/ready checks and device command are prepared in the same
    // synchronous step, including after an asynchronous evidence refresh.
    let reply = h
        .send(|c| {
            c.request_intent(id)?;
            c.root_now()?;
            ensure(
                c.roots()?.generation == publication.management.generation,
                Error::RekeyRequired,
            )?;
            ensure(publication.expires_at > now_ms(), Error::Expired)?;
            c.signed_post(
                &format!("{base}/links"),
                "dmsg/request/create/v1",
                &publication,
                &intent.command,
            )
        })
        .await?;
    let remote = parse(reply)?;
    h.with(|c| c.settle_request_intent(id, remote)).await
}

#[cfg(test)]
#[path = "request_jobs_tests.rs"]
mod tests;
