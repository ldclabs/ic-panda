//! Value-free, device-local dependency inventory. This is not a filesystem scan.
use super::*;

/// One configured consumer or retained delivery observation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dependency {
    /// policy, mount, ssh, snapshot_link, snapshot_channel or process_observation.
    pub kind: String,
    /// Consumer's local identity.
    pub id: String,
    /// Display metadata only; never arguments, values or access keys.
    pub label: String,
    /// Version of the configuration or source observation.
    pub revision: String,
    /// Current, disabled, stale, or the delivery state.
    pub state: String,
    /// Next human action, not a claim of upstream verification.
    pub action: String,
}

/// A canonical inventory that can be acknowledged without accepting a stale list.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReferenceIndex {
    /// Credential identity.
    pub reference: SecretRef,
    /// Current Vault head, if any; invalid references remain inspectable.
    pub revision: Option<String>,
    /// Canonically sorted local dependencies.
    pub dependencies: Vec<Dependency>,
    /// Commitment to this exact metadata inventory and revision.
    pub digest: String,
    /// Always true; other devices and arbitrary project files are not searched.
    pub device_local: bool,
}

impl Client {
    /// Enumerate local references without resolving or returning credential values.
    pub fn secret_references(&self, reference: &SecretRef) -> Result<ReferenceIndex> {
        self.secret_references_at(reference, now_ms())
    }

    pub(crate) fn secret_references_at(
        &self,
        reference: &SecretRef,
        now: u64,
    ) -> Result<ReferenceIndex> {
        self.keys()?;
        reference.validate()?;
        ensure(
            reference.account_id == self.account()?.id.to_string(),
            Error::Forbidden,
        )?;
        let revision = self.workspace.store.head(&reference.item_id)?;
        let grants = self.capability_grants()?;
        let mut dependencies = Vec::new();
        let mut add =
            |kind: &str, id: String, label: String, revision: String, state: &str, action: &str| {
                dependencies.push(Dependency {
                    kind: kind.into(),
                    id,
                    label,
                    revision,
                    state: state.into(),
                    action: action.into(),
                });
            };
        for p in self.secret_policies()? {
            if p.policy.credential == *reference {
                for grant in &grants {
                    if grant.capabilities.contains(&format!("secret.use:{}", p.id)) {
                        let state = if grant.revoked_at.is_some() {
                            "revoked"
                        } else if grant.expires_at.is_some_and(|t| t <= now) {
                            "expired"
                        } else {
                            "granted"
                        };
                        add(
                            "policy_grant",
                            format!("{}:{}", p.id, grant.id),
                            grant.grantee.clone(),
                            hex::encode(Sha256::digest(canonical(grant))),
                            state,
                            "review_agent_authorization",
                        );
                    }
                }
                add(
                    "policy",
                    p.id,
                    p.policy.name,
                    p.version.to_string(),
                    if p.policy.enabled {
                        "current"
                    } else {
                        "disabled"
                    },
                    "review_policy_and_grants",
                );
            }
        }
        for config in self.secret_mount_configs()? {
            for entry in &config.input.entries {
                let matched = match &entry.source {
                    mounts::EntrySource::FieldUtf8 { reference: uri } => {
                        SecretRef::parse(uri).is_ok_and(|r| r == *reference)
                    }
                    mounts::EntrySource::RefsDotenv { variables } => {
                        variables.iter().any(|v| v.reference == *reference)
                    }
                    _ => false,
                };
                if matched {
                    add(
                        "mount",
                        format!("{}:{}", config.id, entry.relative_path),
                        format!("{} / {}", config.input.project_label, entry.relative_path),
                        config.version.to_string(),
                        "configured",
                        "restart_mount_after_source_change",
                    );
                }
            }
        }
        for b in self.secret_ssh_bindings()? {
            if b.resolved.reference == *reference {
                let state = if !b.enabled {
                    "disabled"
                } else if revision.as_ref() != Some(&b.resolved.item_revision) {
                    "stale"
                } else {
                    "current"
                };
                add(
                    "ssh",
                    b.id,
                    b.fingerprint,
                    b.resolved.item_revision,
                    state,
                    "approve_current_key",
                );
            }
        }
        dependencies.extend(self.secret_snapshot_dependencies(reference, now)?);
        for request in self.secret_records()? {
            if let Action::SecretProcess { plan } = &request.action {
                let matched = plan.env.iter().any(|(_, r)| r.reference == *reference)
                    || plan.files.iter().any(|(_, s, _)| matches!(s, process::Source::Field{resolved} if resolved.reference == *reference));
                if matched {
                    dependencies.push(Dependency {
                        kind: "process_observation".into(),
                        id: request.id.clone(),
                        label: plan.input.executable.path.clone(),
                        revision: plan
                            .env
                            .iter()
                            .find(|(_, r)| r.reference == *reference)
                            .map(|(_, r)| r.item_revision.clone())
                            .or_else(|| {
                                plan.files.iter().find_map(|(_, s, _)| match s {
                                    process::Source::Field { resolved }
                                        if resolved.reference == *reference =>
                                    {
                                        Some(resolved.item_revision.clone())
                                    }
                                    _ => None,
                                })
                            })
                            .unwrap_or_default(),
                        state: request.state.name().into(),
                        action: "check_project_refs_and_delivered_copies".into(),
                    });
                }
            }
        }
        dependencies.sort_by(|a, b| (&a.kind, &a.id).cmp(&(&b.kind, &b.id)));
        let digest = hex::encode(Sha256::digest(canonical(&(
            "dmsg/secret-references/1",
            reference,
            &revision,
            &dependencies,
        ))));
        Ok(ReferenceIndex {
            reference: reference.clone(),
            revision,
            dependencies,
            digest,
            device_local: true,
        })
    }

    /// Refresh the inventory used for one rotation; no step is implicitly acknowledged.
    pub fn secret_rotation_dependencies(&self, id: &str) -> Result<ReferenceIndex> {
        let rotation = self
            .secret_rotations()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)?;
        self.secret_references(&rotation.reference)
    }
}
