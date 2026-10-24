//! Device-local policies and single-use approvals. No API resolves secret values.

pub mod discovery;
pub mod github;
pub mod health;
pub mod mounts;
pub mod package;
pub mod process;
pub mod receiving;
pub(crate) mod run;
pub mod shares;
pub mod ssh;
pub mod staging;
#[cfg(test)]
mod tests;

use crate::{
    Client,
    approval::{Action, Request, State},
    codec::canonical,
    crypto::random_id,
    now_ms,
};
use dmsg_types::{AccountId, Error, Result, ensure, ensure_valid};
pub use run::SecretRun;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const POLICIES: &str = "secret-policies";
const MAX_POLICIES: usize = 64;
/// Stable failures, never containing values or upstream response text.
pub const ERROR_CODES: &[&str] = &[
    "SSH_KEY_INVALID",
    "SSH_KEY_PASSPHRASE_REQUIRED",
    "SSH_ALGORITHM_UNSUPPORTED",
    "SECRET_REFERENCE_INVALID",
    "SECRET_VERSION_STALE",
    "SECRET_POLICY_STALE",
    "SECRET_SESSION_ENDED",
    "SECRET_OUTPUT_BLOCKED",
];

pub(crate) fn failure(code: &str) -> Error {
    Error::InvalidInput(code.into())
}

/// A reference to a current field in this account.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SecretRef {
    /// Wire format version.
    pub schema_version: u8,
    /// Account Xid.
    pub account_id: String,
    /// Vault object ID.
    pub item_id: String,
    /// Stable field ID.
    pub field_id: String,
}

pub(crate) fn hex_id(id: &str, len: usize) -> bool {
    id.len() == len
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl SecretRef {
    /// Parse a canonical URI; no URL normalization, escapes or authority extras.
    pub fn parse(uri: &str) -> Result<Self> {
        let parts: Vec<_> = uri
            .strip_prefix("dmsg-secret://")
            .unwrap_or("")
            .split('/')
            .collect();
        ensure(parts.len() == 3, failure("SECRET_REFERENCE_INVALID"))?;
        let reference = Self {
            schema_version: 1,
            account_id: parts[0].into(),
            item_id: parts[1].into(),
            field_id: parts[2].into(),
        };
        reference.validate()?;
        Ok(reference)
    }

    /// Canonical text, containing no authority to use the field.
    pub fn uri(&self) -> String {
        format!(
            "dmsg-secret://{}/{}/{}",
            self.account_id, self.item_id, self.field_id
        )
    }

    fn validate(&self) -> Result<()> {
        ensure(
            self.schema_version == 1
                && self
                    .account_id
                    .parse::<AccountId>()
                    .is_ok_and(|a| a.to_string() == self.account_id)
                && hex_id(&self.item_id, 64)
                && (1..=40).contains(&self.field_id.len())
                && self
                    .field_id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            failure("SECRET_REFERENCE_INVALID"),
        )
    }
}

/// Resolved metadata, with no value.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResolvedSecret {
    /// Reference selected by the owner.
    pub reference: SecretRef,
    /// Exact current revision approved for execution.
    pub item_revision: String,
    /// Field type.
    pub field_kind: String,
    /// Human-readable title and field name at request time.
    pub item_title: String,
    /// Human-readable field name.
    pub field_label: String,
}

/// The only supported destination; the caller cannot override it per use.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Always https://api.github.com.
    pub api_origin: String,
    /// Exact owner path segment.
    pub owner: String,
    /// Exact repository path segment.
    pub repo: String,
}

/// Owner-editable policy fields.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PolicyInput {
    /// Wire format.
    pub schema_version: u8,
    /// Display name.
    pub name: String,
    /// Whether new requests may use it.
    pub enabled: bool,
    /// Always github.issues/1.
    pub adapter: String,
    /// Concealed credential field.
    pub credential: SecretRef,
    /// Fixed repository.
    pub target: Target,
    /// Exactly issue.create.
    pub operations: Vec<String>,
}

/// A policy sealed under this device's LDK; never synchronized.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsePolicy {
    /// Random ID.
    pub id: String,
    /// Owning account.
    pub account_id: String,
    /// Owning device.
    pub device_id: String,
    /// Monotonic edit version.
    pub version: u64,
    /// Policy fields.
    pub policy: PolicyInput,
    /// Creation time.
    pub created_at: u64,
    /// Last change time.
    pub updated_at: u64,
}

/// An immutable configuration approval; removal retains usage history.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyChange {
    /// Destination policy ID, allocated before approval.
    pub id: String,
    /// Expected version, absent for creation.
    pub base_version: Option<u64>,
    /// None deletes the policy.
    pub policy: Option<PolicyInput>,
    /// Optional capability grant included in this configuration approval.
    pub grant_to: Option<String>,
    /// Current account.
    pub account_id: String,
    /// Current device.
    pub device_id: String,
}

/// Request shape accepted from MCP, never including actor or authentication data.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UseRequestInput {
    /// Unique within the account/device/actor retention window.
    pub idempotency_key: String,
    /// Owner-configured policy.
    pub policy_id: String,
    /// Exactly issue.create.
    pub operation: String,
    /// Strict GitHub issue arguments.
    pub arguments: github::IssueInput,
    /// Untrusted reason, only for the person to read.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Immutable approval payload, all stored under the LDK.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretUsePlan {
    /// Wire format.
    pub schema_version: u8,
    /// Account and device binding.
    pub account_id: String,
    /// Device binding.
    pub device_id: String,
    /// Authenticated Agent ID.
    pub actor: String,
    /// Credential generation, never shown to clients.
    pub credential_digest: String,
    /// Exact grant that admitted this request; a later grant cannot revive it.
    pub grant_id: String,
    /// The Agent's pause generation when the request was admitted.
    pub pause_generation: u64,
    /// Client/daemon lifetime, never persisted outside the encrypted plan.
    pub daemon_boot_id: String,
    /// Unlocked session generation.
    pub session_generation: u64,
    /// Full validated caller input.
    pub input: UseRequestInput,
    /// Canonical input commitment.
    pub input_digest: String,
    /// Policy version.
    pub policy_version: u64,
    /// Policy name shown to the owner.
    pub policy_name: String,
    /// Fixed adapter.
    pub adapter_version: String,
    /// Fixed destination.
    pub target: Target,
    /// Exact field metadata.
    pub resolved: ResolvedSecret,
}

impl SecretUsePlan {
    /// Owner approval view, excluding the credential digest and internal generation.
    pub fn view(&self) -> Value {
        json!({"account_id":self.account_id,"device_id":self.device_id,"actor":self.actor,
            "policy_id":self.input.policy_id,"policy_version":self.policy_version,"policy_name":self.policy_name,
            "adapter":self.adapter_version,"operation":self.input.operation,"target":self.target,
            "arguments":self.input.arguments,"reason":self.input.reason,"resolved":self.resolved,
            "single_use":true,"result_fields":["number","html_url"]})
    }
}

impl Client {
    fn secret_device(&self) -> String {
        hex::encode(*self.workspace.meta.device_id)
    }

    /// Resolve only metadata; existing Vault encryption is at item granularity.
    pub fn secret_resolve(&self, reference: &SecretRef, api: bool) -> Result<ResolvedSecret> {
        reference.validate()?;
        self.keys()?;
        ensure(
            self.account()?.id.to_string() == reference.account_id,
            failure("SECRET_REFERENCE_INVALID"),
        )?;
        let mut entry = self.vault_show(&reference.item_id).map_err(|e| match e {
            Error::NotFound => failure("SECRET_REFERENCE_INVALID"),
            e => e,
        })?;
        let result = (|| {
            ensure(
                !entry.deleted && !entry.item.reserved() && entry.item.kind != "wallet",
                failure("SECRET_REFERENCE_INVALID"),
            )?;
            ensure(
                !self
                    .workspace
                    .store
                    .conflicts(crate::content::VAULT)?
                    .iter()
                    .any(|r| r.item == reference.item_id),
                failure("SECRET_VERSION_STALE"),
            )?;
            let field = entry
                .item
                .fields
                .iter()
                .find(|f| f.id == reference.field_id)
                .ok_or_else(|| failure("SECRET_REFERENCE_INVALID"))?;
            ensure(
                field.kind != "otp" && (!api || field.kind == "concealed"),
                failure("SECRET_REFERENCE_INVALID"),
            )?;
            Ok(ResolvedSecret {
                reference: reference.clone(),
                item_revision: entry.revision.clone(),
                field_kind: field.kind.clone(),
                item_title: entry.item.title.clone(),
                field_label: if field.label.is_empty() {
                    field.id.clone()
                } else {
                    field.label.clone()
                },
            })
        })();
        for field in &mut entry.item.fields {
            field.value.zeroize();
        }
        entry.item.notes.zeroize();
        result
    }

    /// Construct a canonical reference without revealing the field.
    pub fn secret_ref(&self, item_id: String, field_id: String) -> Result<Value> {
        let reference = SecretRef {
            schema_version: 1,
            account_id: self.account()?.id.to_string(),
            item_id,
            field_id,
        };
        self.secret_resolve(&reference, false)?;
        Ok(json!({"uri": reference.uri(), "reference": reference}))
    }

    /// All device-local policies, for the owner.
    pub fn secret_policies(&self) -> Result<Vec<UsePolicy>> {
        Ok(self
            .workspace
            .private(self.keys()?, POLICIES)?
            .unwrap_or_default())
    }

    fn secret_policy(&self, id: &str) -> Result<UsePolicy> {
        self.secret_policies()?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or(Error::NotFound)
    }

    fn check_policy(&self, input: &PolicyInput) -> Result<()> {
        ensure_valid(
            input.schema_version == 1
                && !input.name.trim().is_empty()
                && input.name.chars().count() <= 100
                && !input.name.chars().any(char::is_control),
            "policy name/version",
        )?;
        ensure_valid(
            input.adapter == "github.issues/1" && input.operations == ["issue.create"],
            "adapter/operation",
        )?;
        github::validate_target(&input.target)?;
        self.secret_resolve(&input.credential, true)?;
        Ok(())
    }

    /// Ask the person to create, replace or remove a policy; the optional grant is in the same decision.
    pub fn secret_policy_request(
        &mut self,
        source: &str,
        id: Option<String>,
        base_version: Option<u64>,
        policy: Option<PolicyInput>,
        grant_to: Option<String>,
    ) -> Result<Value> {
        ensure(matches!(source, "gui" | "cli"), Error::Forbidden)?;
        if let Some(input) = &policy {
            self.check_policy(input)?;
        }
        ensure_valid(policy.is_some() || id.is_some(), "policy")?;
        let id = match id {
            Some(id) => {
                ensure(
                    Some(self.secret_policy(&id)?.version) == base_version,
                    failure("SECRET_POLICY_STALE"),
                )?;
                id
            }
            None => {
                ensure_valid(base_version.is_none(), "base_version")?;
                ensure(
                    self.secret_policies()?.len() < MAX_POLICIES,
                    Error::QuotaExceeded,
                )?;
                hex::encode(&random_id()[..16])
            }
        };
        let grant_to = grant_to
            .map(|id| self.agent_find(&id).map(|a| a.agent_id))
            .transpose()?;
        ensure_valid(grant_to.is_none() || policy.is_some(), "grant_to")?;
        let change = PolicyChange {
            id,
            base_version,
            policy,
            grant_to,
            account_id: self.account()?.id.to_string(),
            device_id: self.secret_device(),
        };
        let now = now_ms();
        let request = self.approval_create_at(
            source,
            Action::SecretPolicy { change },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    pub(crate) fn secret_policy_apply(
        &mut self,
        change: &PolicyChange,
        request_id: &str,
    ) -> Result<()> {
        ensure(
            change.account_id == self.account()?.id.to_string()
                && change.device_id == self.secret_device(),
            Error::Forbidden,
        )?;
        let mut policies = self.secret_policies()?;
        let previous = policies.iter().find(|p| p.id == change.id).cloned();
        ensure(
            previous.as_ref().map(|p| p.version) == change.base_version,
            failure("SECRET_POLICY_STALE"),
        )?;
        if let Some(input) = &change.policy {
            self.check_policy(input)?;
        }
        let now = now_ms();
        policies.retain(|p| p.id != change.id);
        if let Some(input) = &change.policy {
            ensure(policies.len() < MAX_POLICIES, Error::QuotaExceeded)?;
            policies.push(UsePolicy {
                id: change.id.clone(),
                account_id: change.account_id.clone(),
                device_id: change.device_id.clone(),
                version: previous.as_ref().map_or(1, |p| p.version + 1),
                policy: input.clone(),
                created_at: previous.as_ref().map_or(now, |p| p.created_at),
                updated_at: now,
            });
        }
        // Both the policy and optional grant must commit together. The grant
        // is not copied to resident storage because secret.use is never resident.
        let mut grants = self.capability_grants()?;
        if let Some(actor) = &change.grant_to {
            self.agent_find(actor)?;
            if grants.len() >= crate::agents::MAX_GRANTS {
                grants.retain(|g| g.revoked_at.is_none() && g.expires_at.is_none_or(|at| at > now));
            }
            ensure(
                grants.len() < crate::agents::MAX_GRANTS,
                Error::QuotaExceeded,
            )?;
            grants.push(crate::agents::CapabilityGrant {
                id: request_id[..32].into(),
                grantee: actor.clone(),
                capabilities: vec![format!("secret.use:{}", change.id)],
                created_at: now,
                expires_at: None,
                revoked_at: None,
            });
        }
        self.workspace.store.atomic(|_| {
            self.workspace
                .put_private(self.keys()?, POLICIES, &policies)?;
            self.workspace
                .put_private(self.keys()?, "agent-grants", &grants)?;
            let mut request = self.approval_record(request_id)?;
            request.state = State::Done;
            request.updated_at = now;
            self.write_approval(&request)
        })?;
        self.secret_abort(None, Some(&change.id));
        Ok(())
    }

    /// Disable immediately; expanding scope still requires a new approval.
    pub fn secret_policy_disable(&mut self, id: &str) -> Result<Value> {
        let mut policies = self.secret_policies()?;
        let p = policies
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(Error::NotFound)?;
        p.policy.enabled = false;
        p.version += 1;
        p.updated_at = now_ms();
        let view = json!(p);
        self.workspace
            .put_private(self.keys()?, POLICIES, &policies)?;
        self.secret_abort(None, Some(id));
        Ok(view)
    }

    fn secret_actor(&self, actor: &str) -> Result<crate::agents::AgentEntry> {
        let agent = self.agent_find(actor)?;
        ensure(
            !agent.paused && agent.credential.is_some(),
            Error::Forbidden,
        )?;
        Ok(agent)
    }

    fn secret_grant(
        &self,
        actor: &str,
        policy: &str,
        grant_id: Option<&str>,
        now: u64,
    ) -> Result<crate::agents::CapabilityGrant> {
        self.keys()?;
        let needed = format!("secret.use:{policy}");
        self.capability_live_at(actor, now)?
            .into_iter()
            .find(|g| grant_id.is_none_or(|id| id == g.id) && g.capabilities.contains(&needed))
            .ok_or(Error::Forbidden)
    }

    /// Only policy metadata the Agent may request; no credential references.
    pub fn secret_agent_policies(&self, actor: &str) -> Result<Value> {
        let now = now_ms();
        self.secret_actor(actor)?;
        let policies = self.secret_policies()?;
        let visible = policies.iter().filter(|p| {
            p.policy.enabled && self.capability_check_at(actor, &format!("secret.use:{}", p.id), now).is_ok()
        }).map(|p| json!({
            "id": p.id, "name": p.policy.name, "version": p.version,
            "adapter": p.policy.adapter, "target": p.policy.target, "operations": p.policy.operations,
        })).collect();
        Ok(Value::Array(visible))
    }

    /// Validate, deduplicate and enqueue a use. Does not contact GitHub.
    pub fn secret_use_request(&mut self, actor: &str, input: UseRequestInput) -> Result<Value> {
        let now = now_ms();
        let agent = self.secret_actor(actor)?;
        ensure_valid(
            hex_id(&input.policy_id, 32)
                && (1..=128).contains(&input.idempotency_key.len())
                && input
                    .idempotency_key
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)),
            "idempotency_key/policy_id",
        )?;
        ensure_valid(
            input.operation == "issue.create"
                && input.reason.as_ref().is_none_or(|r| r.len() <= 4096),
            "operation/reason",
        )?;
        input.arguments.validate()?;
        let grant = self.secret_grant(actor, &input.policy_id, None, now)?;
        self.expire_approvals(now)?;
        let input_digest = hex::encode(Sha256::digest(canonical(&input)));
        for r in self.secret_records()? {
            if let Action::SecretUse { plan } = &r.action
                && plan.actor == actor
                && plan.input.idempotency_key == input.idempotency_key
            {
                ensure(
                    plan.input_digest == input_digest,
                    Error::IdempotencyConflict,
                )?;
                return self.secret_use_view(&r, false);
            }
        }
        let p = self.secret_policy(&input.policy_id)?;
        ensure(p.policy.enabled, failure("SECRET_POLICY_STALE"))?;
        let resolved = self.secret_resolve(&p.policy.credential, true)?;
        let plan = SecretUsePlan {
            schema_version: 1,
            account_id: self.account()?.id.to_string(),
            device_id: self.secret_device(),
            actor: actor.into(),
            credential_digest: agent.credential.unwrap(),
            grant_id: grant.id,
            pause_generation: agent.pause_generation,
            daemon_boot_id: self.secret_boot.clone(),
            session_generation: self.session_generation,
            input,
            input_digest,
            policy_version: p.version,
            policy_name: p.policy.name,
            adapter_version: p.policy.adapter,
            target: p.policy.target,
            resolved,
        };
        let r = self.approval_create_at(
            &format!("agent:{actor}"),
            Action::SecretUse {
                plan: Box::new(plan),
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        self.secret_use_view(&r, false)
    }

    fn secret_records(&self) -> Result<Vec<Request>> {
        self.workspace
            .store
            .approvals(&[])?
            .iter()
            .filter(|r| r.class == "secret")
            .map(|r| self.approval_record(&r.id))
            .collect()
    }

    pub(crate) fn secret_use_view(&self, r: &Request, owner: bool) -> Result<Value> {
        let mut view = json!({"request_id":r.id,"state":r.state.name(),"expires_at":r.expires_at,"created_at":r.created_at,"updated_at":r.updated_at,"error":r.error,"result":r.secret_result,"reviewed_at":r.reviewed_at});
        if owner {
            view["approval"] = self.approval_view(r)?;
        }
        Ok(view)
    }

    /// Get one use; Agents must still hold its capability and own the request.
    pub fn secret_use_get(&mut self, id: &str, actor: Option<&str>) -> Result<Value> {
        if self.keys.is_none() {
            // Ownership is sealed too: a locked Agent cannot inspect another
            // request by guessing IDs. Owner status needs no decrypted index.
            ensure(actor.is_none(), Error::Locked)?;
            let row = self
                .workspace
                .store
                .approvals(&[])?
                .into_iter()
                .find(|r| r.id == id && r.class == "secret")
                .ok_or(Error::NotFound)?;
            return Ok(json!({"request_id":row.id,"state":row.state}));
        }
        let now = now_ms();
        self.expire_approvals(now)?;
        let request = self.secret_owned_request(id, actor, now)?;
        self.secret_use_view(&request, actor.is_none())
    }

    fn secret_owned_request(&self, id: &str, actor: Option<&str>, now: u64) -> Result<Request> {
        let request = self.approval_record(id)?;
        let Action::SecretUse { plan } = &request.action else {
            return Err(Error::NotFound);
        };
        if let Some(actor) = actor {
            self.secret_actor(actor)?;
            ensure(plan.actor == actor, Error::Forbidden)?;
            self.capability_check_at(actor, &format!("secret.use:{}", plan.input.policy_id), now)?;
        }
        Ok(request)
    }

    /// Cancel only an undecided owned request.
    pub fn secret_use_cancel(&mut self, id: &str, actor: Option<&str>) -> Result<Value> {
        let now = now_ms();
        self.expire_approvals(now)?;
        let mut request = self.secret_owned_request(id, actor, now)?;
        ensure(
            request.state == State::AwaitingUser && now < request.expires_at,
            Error::Expired,
        )?;
        request.state = State::Cancelled;
        request.updated_at = now;
        self.save_approval(&request)?;
        self.secret_use_view(&request, actor.is_none())
    }

    /// Owner's device-local history, with a stable created-time/ID cursor.
    pub fn secret_uses(
        &mut self,
        before: Option<&str>,
        actor: Option<&str>,
        limit: usize,
    ) -> Result<Value> {
        self.expire_approvals(now_ms())?;
        let mut rows = self.secret_records()?;
        rows.sort_by(|a, b| (b.created_at, &b.id).cmp(&(a.created_at, &a.id)));
        if let Some(before) = before {
            let i = rows
                .iter()
                .position(|r| r.id == before)
                .ok_or(Error::NotFound)?;
            rows.drain(..=i);
        }
        let uses: Vec<_> = rows.iter().filter(|r| matches!(&r.action,Action::SecretUse{plan} if actor.is_none_or(|a| a==plan.actor))).take(limit.clamp(1,100)).map(|r| self.secret_use_view(r,true)).collect::<Result<_>>()?;
        Ok(
            json!({"uses":uses,"device_local":true,"next":uses.last().and_then(|v|v["request_id"].as_str())}),
        )
    }

    /// Record manual inspection of a lost result without pretending it succeeded.
    pub fn secret_use_reviewed(&mut self, id: &str) -> Result<Value> {
        let now = now_ms();
        let mut r = self.secret_owned_request(id, None, now)?;
        ensure(r.state == State::Lost, Error::Forbidden)?;
        r.reviewed_at = Some(now);
        self.save_approval(&r)?;
        self.secret_use_view(&r, true)
    }

    pub(crate) fn secret_plan_check(&self, p: &SecretUsePlan, now: u64) -> Result<()> {
        ensure(
            p.account_id == self.account()?.id.to_string()
                && p.device_id == self.secret_device()
                && p.daemon_boot_id == self.secret_boot
                && p.session_generation == self.session_generation,
            failure("SECRET_SESSION_ENDED"),
        )?;
        let agent = self
            .secret_actor(&p.actor)
            .map_err(|_| failure("SECRET_SESSION_ENDED"))?;
        ensure(
            agent.credential.as_ref() == Some(&p.credential_digest)
                && agent.pause_generation == p.pause_generation,
            failure("SECRET_SESSION_ENDED"),
        )?;
        self.secret_grant(&p.actor, &p.input.policy_id, Some(&p.grant_id), now)
            .map_err(|_| failure("SECRET_SESSION_ENDED"))?;
        let policy = self
            .secret_policy(&p.input.policy_id)
            .map_err(|_| failure("SECRET_POLICY_STALE"))?;
        ensure(
            policy.policy.enabled
                && policy.version == p.policy_version
                && policy.policy.target == p.target
                && policy.policy.credential == p.resolved.reference
                && policy.policy.adapter == p.adapter_version,
            failure("SECRET_POLICY_STALE"),
        )?;
        ensure(
            self.secret_resolve(&p.resolved.reference, true)? == p.resolved,
            failure("SECRET_VERSION_STALE"),
        )
    }

    fn secret_value(&self, reference: &SecretRef) -> Result<Zeroizing<String>> {
        let mut entry = self.vault_show(&reference.item_id)?;
        let value = entry
            .item
            .fields
            .iter()
            .find(|f| f.id == reference.field_id)
            .ok_or_else(|| failure("SECRET_REFERENCE_INVALID"))?
            .value
            .clone();
        for f in &mut entry.item.fields {
            f.value.zeroize();
        }
        entry.item.notes.zeroize();
        Ok(Zeroizing::new(value))
    }
}
