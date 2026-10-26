//! Device-local observations and owner-reported rotation steps, never upstream claims.
use super::*;
use dmsg_types::DAY;

const DISMISSED: &str = "secret-health-dismissed";
const ROTATIONS: &str = "secret-rotations";

/// A value-free observation, fixed to its subject revision.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    /// Deterministic ID of rule, subject and revision; no credential hash.
    pub id: String,
    /// Local rule identifier.
    pub rule: String,
    /// info, warning or high.
    pub severity: String,
    /// Opaque object, policy or share ID.
    pub subject: String,
    /// Exact observed revision/version.
    pub revision: String,
    /// Optional affected field.
    pub field: Option<String>,
    /// This device's observation time.
    pub observed_at: u64,
    /// local_record or last_known_cloud_state.
    pub evidence: String,
    /// Concrete suggested next action.
    pub action: String,
    /// Hidden only for exactly this observation identity.
    pub dismissed: bool,
}

fn finding(
    rule: &str,
    subject: &str,
    revision: &str,
    field: Option<String>,
    now: u64,
    action: &str,
) -> Finding {
    Finding {
        id: hex::encode(Sha256::digest(canonical(&(
            "dmsg/secret-finding/1",
            rule,
            subject,
            revision,
            &field,
        )))),
        rule: rule.into(),
        severity: "warning".into(),
        subject: subject.into(),
        revision: revision.into(),
        field,
        observed_at: now,
        evidence: "local_record".into(),
        action: action.into(),
        dismissed: false,
    }
}

/// Explicit human observations alongside locally verified Vault revisions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rotation {
    /// Device-local record ID.
    pub id: String,
    /// Credential reference.
    pub reference: SecretRef,
    /// Version before rotation.
    pub previous_revision: String,
    /// Current version observed after owner updates the Vault.
    pub new_revision: Option<String>,
    /// Owner's statement that upstream created a credential.
    pub created: bool,
    /// Owner's statement or local adapter record, explicitly labeled.
    pub tested: bool,
    /// Owner checked listed references and shares.
    pub references_checked: bool,
    /// Exact inventory acknowledged by the owner; invalidated by dependency changes.
    pub references_digest: Option<String>,
    /// Owner reports upstream revocation; dMsg did not verify upstream state.
    pub upstream_revoked: bool,
    /// All step reports are owner_statement; revisions are local_record.
    pub evidence: String,
    /// Start time.
    pub created_at: u64,
    /// Last edit time.
    pub updated_at: u64,
    /// Time all ordered steps were reported complete.
    pub completed_at: Option<u64>,
}

impl Client {
    /// Run rules explicitly; never performs a network query or directory scan.
    pub fn secret_health(&mut self) -> Result<Value> {
        let now = now_ms();
        let mut findings = Vec::new();
        for f in self.vault_health()? {
            let mut item = self.vault_show(&f.id)?;
            findings.push(finding(
                f.issue,
                &f.id,
                &item.revision,
                Some(f.field),
                now,
                "review_field",
            ));
            for field in &mut item.item.fields {
                field.value.zeroize();
            }
            item.item.notes.zeroize();
        }
        let uses = self.secret_records()?;
        for p in self.secret_policies()? {
            let revision = format!(
                "{}:{}",
                p.version,
                self.workspace
                    .store
                    .head(&p.policy.credential.item_id)?
                    .unwrap_or_default()
            );
            if self.secret_resolve(&p.policy.credential, true).is_err() {
                findings.push(finding(
                    "invalid_policy_reference",
                    &p.id,
                    &revision,
                    None,
                    now,
                    "repair_reference",
                ));
            }
            if p.policy.enabled && now.saturating_sub(p.updated_at) >= 90 * DAY {
                findings.push(finding(
                    "policy_review_due",
                    &p.id,
                    &revision,
                    None,
                    now,
                    "review_device_policy",
                ));
            }
            let used = uses
                .iter()
                .filter_map(|r| match &r.action {
                    Action::SecretUse { plan } if plan.input.policy_id == p.id && r.started => {
                        Some(r.updated_at)
                    }
                    _ => None,
                })
                .max();
            if p.policy.enabled && now.saturating_sub(used.unwrap_or(p.created_at)) >= 30 * DAY {
                findings.push(finding(
                    "not_observed_on_this_device",
                    &p.id,
                    &revision,
                    None,
                    now,
                    "review_device_history",
                ));
            }
        }
        for b in self.secret_ssh_bindings()? {
            if b.enabled
                && !self
                    .secret_resolve(&b.resolved.reference, false)
                    .is_ok_and(|r| r == b.resolved)
            {
                findings.push(finding(
                    "invalid_ssh_reference",
                    &b.id,
                    &format!(
                        "{}:{}",
                        b.resolved.item_revision,
                        self.workspace
                            .store
                            .head(&b.resolved.reference.item_id)?
                            .unwrap_or_default()
                    ),
                    None,
                    now,
                    "approve_current_key",
                ));
            }
        }
        for config in self.secret_mount_configs()? {
            let checked = self.secret_mount_prepare(&config.input, &config.mode);
            if checked.is_err() {
                let mut current = Vec::new();
                for entry in &config.input.entries {
                    match &entry.source {
                        mounts::EntrySource::FieldUtf8 { reference } => {
                            if let Ok(reference) = SecretRef::parse(reference) {
                                current.push(
                                    self.workspace
                                        .store
                                        .head(&reference.item_id)?
                                        .unwrap_or_default(),
                                );
                            }
                        }
                        mounts::EntrySource::RefsDotenv { variables } => {
                            for variable in variables {
                                current.push(
                                    self.workspace
                                        .store
                                        .head(&variable.reference.item_id)?
                                        .unwrap_or_default(),
                                );
                            }
                        }
                        mounts::EntrySource::LibraryFile {
                            file_id,
                            version_id,
                        } => {
                            for key in [
                                format!("{}{file_id}", crate::library::catalog::FILE),
                                format!(
                                    "{}{file_id}:{version_id}",
                                    crate::library::catalog::VERSION
                                ),
                            ] {
                                current
                                    .push(self.workspace.store.sealed(&key)?.unwrap_or_default());
                            }
                        }
                    }
                }
                let revision = format!(
                    "{}:{}",
                    config.version,
                    hex::encode(Sha256::digest(canonical(&(&config.input, current))))
                );
                findings.push(finding(
                    "invalid_mount_reference",
                    &config.id,
                    &revision,
                    None,
                    now,
                    "review_mount_sources_and_restart",
                ));
            }
        }
        for link in self.files_share_links()? {
            if link.state == "active" {
                let mut f = finding(
                    "active_share",
                    &link.share_id,
                    &link.created_at.to_string(),
                    None,
                    now,
                    "review_share_expiry_and_recipients",
                );
                f.evidence = if link.last_checked_at.is_some() {
                    "last_known_cloud_state"
                } else {
                    "cloud_state_not_refreshed"
                }
                .into();
                f.observed_at = link.last_checked_at.unwrap_or(link.created_at);
                findings.push(f);
            }
            if let Some(crate::library::link::LinkSource::VaultSnapshot {
                item_id,
                item_revision,
                ..
            }) = &link.source
            {
                let current = self.workspace.store.head(item_id)?.unwrap_or_default();
                if current != *item_revision && link.revoked_at.is_none() {
                    let mut f = finding(
                        "snapshot_precedes_current_credential",
                        &link.share_id,
                        &format!("{item_revision}:{current}"),
                        None,
                        now,
                        "review_and_revoke_old_snapshot",
                    );
                    f.evidence = "local_revision_and_last_known_share".into();
                    findings.push(f);
                }
            }
        }
        // Inspect attachment references, never infer backup from Vault synchronization.
        for mut entry in self.vault_list(&crate::vault::Filter::default())? {
            for attachment in &entry.item.files {
                let status = self.files_show(attachment);
                let unavailable = match status {
                    Ok(details) => details
                        .versions
                        .iter()
                        .find(|v| v.current)
                        .is_none_or(|v| v.backup.is_none()),
                    Err(_) => true,
                };
                if unavailable {
                    // One finding per attachment, each with its own identity.
                    findings.push(finding(
                        "attachment_backup_unconfirmed",
                        &entry.id,
                        &entry.revision,
                        Some(attachment.clone()),
                        now,
                        "back_up_selected_file_version",
                    ));
                }
            }
            for f in &mut entry.item.fields {
                f.value.zeroize();
            }
            entry.item.notes.zeroize();
        }
        let dismissed: Vec<String> = self
            .workspace
            .private(self.keys()?, DISMISSED)?
            .unwrap_or_default();
        for f in &mut findings {
            f.dismissed = dismissed.contains(&f.id);
        }
        Ok(
            json!({"findings":findings,"observed_at":now,"device_local":true,"upstream_revocation":"unknown","rotations":self.secret_rotations()?}),
        )
    }

    /// Ignore only an existing current finding; future revisions receive different IDs.
    pub fn secret_health_dismiss(&mut self, id: &str) -> Result<Value> {
        let current = self.secret_health()?;
        ensure(
            current["findings"]
                .as_array()
                .is_some_and(|fs| fs.iter().any(|f| f["id"] == id)),
            Error::NotFound,
        )?;
        let mut dismissed: Vec<String> = self
            .workspace
            .private(self.keys()?, DISMISSED)?
            .unwrap_or_default();
        if !dismissed.iter().any(|v| v == id) {
            ensure(dismissed.len() < 4096, Error::QuotaExceeded)?;
            dismissed.push(id.into());
        }
        self.workspace
            .put_private(self.keys()?, DISMISSED, &dismissed)?;
        Ok(json!({"dismissed":id}))
    }

    /// Device-local rotation tracking, not upstream automation.
    pub fn secret_rotations(&self) -> Result<Vec<Rotation>> {
        Ok(self
            .workspace
            .private(self.keys()?, ROTATIONS)?
            .unwrap_or_default())
    }

    /// Rotation progress with a value-free check of whether the credential
    /// actually changed. Editing only a title cannot unlock the first step.
    pub fn secret_rotation_views(&self) -> Result<Vec<Value>> {
        self.secret_rotations()?
            .into_iter()
            .map(|rotation| {
                let mut view = json!(rotation);
                let checked = (|| -> Result<(String, bool)> {
                    let current = self.secret_resolve(&rotation.reference, false)?;
                    let mut previous = self
                        .vault_version(&rotation.reference.item_id, &rotation.previous_revision)?;
                    let value = self.secret_value(&rotation.reference)?;
                    let changed =
                        previous.item.value(&rotation.reference.field_id) != value.as_str();
                    for field in &mut previous.item.fields {
                        field.value.zeroize();
                    }
                    previous.item.notes.zeroize();
                    Ok((current.item_revision, changed))
                })();
                match checked {
                    Ok((revision, changed)) => {
                        view["can_report_created"] = json!(
                            rotation.completed_at.is_none()
                                && changed
                                && rotation.new_revision.as_ref() != Some(&revision)
                        );
                        view["current_revision"] = json!(revision);
                    }
                    Err(error) => {
                        view["can_report_created"] = json!(false);
                        view["source_error"] = json!(crate::error_code(&error));
                    }
                }
                Ok(view)
            })
            .collect()
    }

    /// Record the credential version before the owner changes it upstream.
    pub fn secret_rotation_create(&mut self, reference: SecretRef) -> Result<Value> {
        let resolved = self.secret_resolve(&reference, false)?;
        let now = now_ms();
        let mut rotations = self.secret_rotations()?;
        ensure(rotations.len() < 256, Error::QuotaExceeded)?;
        let r = Rotation {
            id: hex::encode(random_id()),
            reference,
            previous_revision: resolved.item_revision,
            new_revision: None,
            created: false,
            tested: false,
            references_checked: false,
            references_digest: None,
            upstream_revoked: false,
            evidence: "owner_statement".into(),
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        rotations.push(r.clone());
        self.workspace
            .put_private(self.keys()?, ROTATIONS, &rotations)?;
        Ok(json!(r))
    }

    /// Record ordered human steps. Changing roots/grants cannot complete any step.
    pub fn secret_rotation_update(
        &mut self,
        id: &str,
        step: &str,
        dependencies_digest: Option<&str>,
    ) -> Result<Value> {
        let now = now_ms();
        let mut rotations = self.secret_rotations()?;
        let r = rotations
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)?;
        ensure(r.completed_at.is_none(), Error::Expired)?;
        let current = self.secret_resolve(&r.reference, false)?;
        match step {
            "created" => {
                ensure(
                    current.item_revision != r.previous_revision,
                    Error::VersionConflict,
                )?;
                let mut previous =
                    self.vault_version(&r.reference.item_id, &r.previous_revision)?;
                let value = self.secret_value(&r.reference)?;
                let changed = previous.item.value(&r.reference.field_id) != value.as_str();
                for field in &mut previous.item.fields {
                    field.value.zeroize();
                }
                previous.item.notes.zeroize();
                ensure(changed, Error::VersionConflict)?;
                r.new_revision = Some(current.item_revision);
                r.created = true;
                r.tested = false;
                r.references_checked = false;
                r.references_digest = None;
                r.upstream_revoked = false;
            }
            "tested" => {
                ensure(
                    r.created && r.new_revision.as_ref() == Some(&current.item_revision),
                    Error::VersionConflict,
                )?;
                r.tested = true;
            }
            "references_checked" => {
                ensure(
                    r.tested && r.new_revision.as_ref() == Some(&current.item_revision),
                    Error::VersionConflict,
                )?;
                let index = self.secret_references_at(&r.reference, now)?;
                ensure(
                    dependencies_digest == Some(index.digest.as_str()),
                    Error::VersionConflict,
                )?;
                r.references_digest = Some(index.digest);
                r.references_checked = true;
            }
            "upstream_revoked" => {
                ensure(
                    r.references_checked
                        && r.new_revision.as_ref() == Some(&current.item_revision)
                        && r.references_digest.as_ref()
                            == Some(&self.secret_references_at(&r.reference, now)?.digest),
                    Error::VersionConflict,
                )?;
                r.upstream_revoked = true;
            }
            _ => return Err(Error::InvalidInput("rotation step".into())),
        }
        r.updated_at = now;
        if r.upstream_revoked {
            r.completed_at = Some(now);
        }
        let view = json!(r);
        self.workspace
            .put_private(self.keys()?, ROTATIONS, &rotations)?;
        Ok(view)
    }
}
