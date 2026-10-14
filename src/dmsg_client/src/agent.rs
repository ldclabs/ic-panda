//! Agent Delegation with this account as the principal
//! (`docs/protocol/agent_zh.md`): enabling the principal, the controller
//! keys, and the grants and revocations they sign.
//!
//! A controller key is an Ed25519 seed kept as a vault item with a fixed
//! ID, so every device holding the content root can sign with it. Every
//! change goes through the approval center: principal changes are account
//! mutations the user home applies and publishes to the directory; a grant
//! or revocation is signed here, journaled, then submitted to the
//! delegation service. A signed envelope is resubmitted unchanged after a
//! lost reply or a 5xx/408/429 answer; any other refusal is final and needs
//! a new approval.

use crate::client::Client;
use crate::codec::{b64, canonical, unb64};
use crate::crypto::{random_id, sign, signing_public, Key};
use crate::now_ms;
use crate::vault::Item;
use agent_protocols::{
    delegation::{
        validate_audience, validate_delegation_grant_payload, DelegationCredential,
        DelegationGrantPayload, DelegationPayload, DelegationRevokePayload, DELEGATION_GRANT,
        DELEGATION_REVOKE, PROTOCOL,
    },
    error::SdkError,
    http_client::DelegationClient,
    identity::{AgentId, AgentSigner, ClientNonceManager, Envelope, Event},
};
use dmsg_protocol::{controller_pop_message, digest};
use dmsg_types::{
    agent::{DelegationAuthority, PrincipalInfo, PrincipalType},
    ensure, ensure_valid,
    user::{AccountCommand, Capability},
    Ed25519Signature, Error, Hash, Result, DAY,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const JOURNAL: &str = "delegation:";
const NONCE: &str = "delegation-nonce:";
/// Grants expire within this many days (the dMsg delegation policy).
pub const MAX_GRANT_DAYS: u64 = 366;

/// A principal change, run as an account mutation once a person approves it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum PrincipalChange {
    /// Publish the account as a principal with no controllers.
    Enable {
        /// Display type.
        principal_type: PrincipalType,
    },
    /// Create the next controller key in the vault and register it.
    Register {
        /// The next unused generation.
        generation: u32,
        /// Display label.
        name: Option<String>,
        /// Its ceiling, fixed for its life.
        delegation: DelegationAuthority,
    },
    /// Retire a controller.
    Retire {
        /// Generation.
        generation: u32,
    },
    /// Retire a controller if needed and distrust its signatures from a time.
    Compromise {
        /// Generation.
        generation: u32,
        /// Earliest untrusted signature time.
        invalid_from: u64,
    },
    /// Change a controller's display label.
    Rename {
        /// Generation.
        generation: u32,
        /// New label, or none.
        name: Option<String>,
    },
}

/// What a delegation event does.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DelegationKind {
    /// Grant, or replace with the same ID, a credential.
    Grant {
        /// Delegation ID, `<account_id>.<suffix>`.
        id: String,
        /// The agent, `did:agent:…`.
        subject: String,
        /// Exact scopes.
        scopes: Vec<String>,
        /// Relying parties: HTTPS origins or Agent IDs.
        audiences: Vec<String>,
        /// Unix milliseconds.
        expires_at: u64,
        /// Free-text relationship shown to relying parties.
        relationship: Option<String>,
    },
    /// Revoke a credential.
    Revoke {
        /// Delegation ID.
        id: String,
        /// Free-text reason.
        reason: Option<String>,
    },
}

impl DelegationKind {
    fn id(&self) -> &str {
        match self {
            DelegationKind::Grant { id, .. } | DelegationKind::Revoke { id, .. } => id,
        }
    }
}

/// Where a delegation event stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Approved or waiting; not signed.
    Prepared,
    /// Signed and journaled; the service's answer is not known.
    Signed,
    /// The service accepted it.
    Accepted,
    /// The service refused it; a new approval is needed.
    Failed,
}

/// One delegation event's journal, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DelegationJob {
    /// Random job ID, hex.
    pub id: String,
    /// The signing controller's generation.
    pub generation: u32,
    /// What it does.
    pub kind: DelegationKind,
    /// Progress.
    pub stage: Stage,
    /// The signed envelope, from [`Stage::Signed`] on.
    pub envelope: Option<Envelope<DelegationPayload>>,
    /// The credential the service returned.
    pub credential: Option<DelegationCredential>,
    /// The service's refusal code.
    pub error: Option<String>,
    /// Unix milliseconds.
    pub created_at: u64,
}

impl DelegationJob {
    /// The commitment a person approves.
    pub fn digest(&self) -> Hash {
        digest(
            "dmsg/delegation-approval/v1",
            &(&self.id, self.generation, &self.kind),
        )
    }

    /// The job for display.
    pub fn view(&self) -> Json {
        json!({
            "id": self.id,
            "generation": self.generation,
            "kind": self.kind,
            "stage": self.stage,
            "event": self.envelope.as_ref().map(|e| &e.hash),
            "credential": self.credential,
            "error": self.error,
            "created_at": self.created_at,
        })
    }
}

/// The vault item ID of controller `generation` of `account`.
pub fn controller_item(account: &str, generation: u32) -> String {
    hex::encode(Sha256::digest(canonical(&(
        "dmsg/agent-controller/1",
        account,
        generation,
    ))))
}

/// `did:agent:` of an Ed25519 public key.
pub fn agent_id(public_key: &Key) -> String {
    AgentId::from_public_key(public_key).to_string()
}

/// The next generation to register: one past the last, current or retired.
pub fn next_generation(info: &PrincipalInfo) -> u32 {
    info.state
        .controllers
        .iter()
        .map(|c| c.generation)
        .max()
        .unwrap_or(0)
        + 1
}

/// Check a ceiling as the user home will.
fn check_authority(authority: &DelegationAuthority) -> Result<()> {
    if let DelegationAuthority::Restricted { scopes, audiences } = authority {
        check_list(scopes, "scopes", |s| s.len() <= 64)?;
        check_list(audiences, "audiences", |a| validate_audience(a).is_ok())?;
    }
    Ok(())
}

/// Nonempty, distinct, explicit values that pass `valid`.
fn check_list(values: &[String], name: &str, valid: impl Fn(&str) -> bool) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    ensure_valid(
        !values.is_empty()
            && values
                .iter()
                .all(|v| !v.is_empty() && v.trim() == v && v != "*" && seen.insert(v) && valid(v)),
        name,
    )
}

/// Sign `kind` for principal `principal_id` with a controller seed whose
/// binding starts at `valid_from`, taking the nonce from `next` (Agent
/// Identity §6.2: `max(last + 1, created_at)`). Returns the envelope and
/// the next nonce to keep.
///
/// # Errors
/// Returns `Error::Expired` for a grant whose lifetime no longer fits and
/// `Error::InvalidInput` for a payload the protocol refuses.
pub fn sign_delegation(
    seed: &Key,
    principal_id: &str,
    kind: &DelegationKind,
    valid_from: u64,
    next: u64,
    now: u64,
) -> Result<(Envelope<DelegationPayload>, u64)> {
    let signer = AgentSigner::from_seed(*seed);
    // A new binding starts at valid_from; nothing is backdated before it.
    let created_at = now.max(valid_from);
    let mut nonces = ClientNonceManager::with_next(next).map_err(sdk)?;
    let nonce = nonces.next_nonce_at(created_at as i64).map_err(sdk)?;
    let (event_type, payload) = match kind {
        DelegationKind::Grant {
            id,
            subject,
            scopes,
            audiences,
            expires_at,
            relationship,
        } => {
            ensure(
                *expires_at > created_at && *expires_at <= created_at + MAX_GRANT_DAYS * DAY,
                Error::Expired,
            )?;
            let mut payload = DelegationGrantPayload::new(
                id.clone(),
                principal_id,
                subject.parse().map_err(|_| Error::IntegrityFailed)?,
                scopes.clone(),
                audiences.clone(),
            );
            payload.expires_at = Some(*expires_at as i64);
            payload.relationship = relationship.clone();
            validate_delegation_grant_payload(&payload, Some(created_at as i64)).map_err(sdk)?;
            (DELEGATION_GRANT, DelegationPayload::Grant(payload))
        }
        DelegationKind::Revoke { id, reason } => (
            DELEGATION_REVOKE,
            DelegationPayload::Revoke(DelegationRevokePayload {
                id: id.clone(),
                principal_id: principal_id.into(),
                reason: reason.clone(),
            }),
        ),
    };
    let event = Event::new(
        PROTOCOL,
        event_type,
        signer.agent_id(),
        created_at as i64,
        nonce,
        payload,
    );
    let envelope = signer.sign_event(event).map_err(sdk)?;
    Ok((envelope, nonces.peek()))
}

/// The error a delegation service's refusal becomes; the job keeps its code.
fn refusal(code: &str) -> Error {
    Error::InvalidInput(format!("delegation service refused: {code}"))
}

impl Client {
    fn delegation_job(&self, id: &str) -> Result<DelegationJob> {
        self.workspace
            .private(self.keys()?, &format!("{JOURNAL}{id}"))?
            .ok_or(Error::NotFound)
    }

    fn save_delegation_job(&self, job: &DelegationJob) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{JOURNAL}{}", job.id), job)
    }

    /// The account's principal and its publication, `None` before it is enabled.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` without an account.
    pub async fn principal(&mut self) -> Result<Option<PrincipalInfo>> {
        let account = self.account()?.clone();
        let ic = self.reader().await?;
        let info: Result<PrincipalInfo> = ic
            .query(account.home, "get_principal", (account.id,))
            .await?;
        match info {
            Ok(info) => Ok(Some(info)),
            Err(Error::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The principal for display, with each controller's Agent ID.
    ///
    /// # Errors
    /// As [`Self::principal`].
    pub async fn principal_view(&mut self) -> Result<Json> {
        let Some(info) = self.principal().await? else {
            return Ok(Json::Null);
        };
        let controllers: Vec<Json> = info
            .state
            .controllers
            .iter()
            .map(|c| {
                json!({
                    "generation": c.generation,
                    "agent_id": agent_id(&c.public_key),
                    "name": c.name,
                    "valid_from": c.valid_from,
                    "delegation": match &c.delegation {
                        DelegationAuthority::Unrestricted => json!("*"),
                        DelegationAuthority::Restricted { scopes, audiences } =>
                            json!({"scopes": scopes, "audiences": audiences}),
                    },
                    "retired_at": c.retired_at,
                    "invalid_from": c.invalid_from,
                })
            })
            .collect();
        Ok(json!({
            "principal_id": info.principal_id,
            "type": format!("{:?}", info.state.principal_type),
            "controllers": controllers,
            "version": info.state.version,
            "published_version": info.published_version,
            "published": info.published_version == info.state.version,
            "delegation_service": self.config.relay_origin(),
            "next_generation": next_generation(&info),
        }))
    }

    /// Ask the user home to publish the principal to the directory again.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a login.
    pub async fn principal_publish(&mut self) -> Result<u64> {
        let account = self.account()?.clone();
        let login = self.session()?;
        let version: Result<u64> = login
            .ic
            .update(account.home, "publish_principal", (account.id,))
            .await?;
        version
    }

    /// Let the delegation service drop its cached principal document, after
    /// a published controller change.
    ///
    /// # Errors
    /// Relay errors as returned.
    pub async fn principal_refresh(&mut self) -> Result<Json> {
        let path = format!("{}/principal/refresh", self.base()?);
        let state = self.cloud_state().await?;
        self.cloud
            .post_raw(&self.signer(&state)?, &path, &(), now_ms())
            .await
    }

    /// Ask to change the principal; an administrator device approves it.
    ///
    /// # Errors
    /// Returns `Error::NotFound` before the principal is enabled (except to
    /// enable it), `Error::VersionConflict` for a stale generation and
    /// `Error::InvalidInput` for an invalid ceiling or label.
    pub async fn principal_request(
        &mut self,
        source: &str,
        change: PrincipalChange,
    ) -> Result<Json> {
        self.keys()?;
        let info = self.principal().await?;
        match (&change, &info) {
            (PrincipalChange::Enable { .. }, None) => {}
            (PrincipalChange::Enable { .. }, Some(_)) => return Err(Error::IdempotencyConflict),
            (_, None) => return Err(Error::NotFound),
            (
                PrincipalChange::Register {
                    generation,
                    name,
                    delegation,
                },
                Some(info),
            ) => {
                ensure(*generation == next_generation(info), Error::VersionConflict)?;
                if let Some(name) = name {
                    dmsg_protocol::agent::validate_controller_name(name)?;
                }
                check_authority(delegation)?;
            }
            (
                PrincipalChange::Rename {
                    name: Some(name), ..
                },
                _,
            ) => {
                dmsg_protocol::agent::validate_controller_name(name)?;
            }
            _ => {}
        }
        let request = self.approval_create(
            source,
            crate::approval::Action::Principal { change },
            now_ms() + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// Run an approved principal change. A lost reply is settled through
    /// the account journal, like every account mutation.
    pub(crate) async fn principal_execute(&mut self, change: &PrincipalChange) -> Result<()> {
        let command = match change.clone() {
            PrincipalChange::Enable { principal_type } => {
                AccountCommand::EnablePrincipal { principal_type }
            }
            PrincipalChange::Register {
                generation,
                name,
                delegation,
            } => return self.controller_register(generation, name, delegation).await,
            PrincipalChange::Retire { generation } => {
                AccountCommand::RetireController { generation }
            }
            PrincipalChange::Compromise {
                generation,
                invalid_from,
            } => AccountCommand::MarkControllerCompromised {
                generation,
                invalid_from,
            },
            PrincipalChange::Rename { generation, name } => {
                AccountCommand::RenameController { generation, name }
            }
        };
        self.mutate(command, None).await?;
        self.principal_settled().await;
        Ok(())
    }

    /// After a committed change, tell the delegation service once the
    /// directory has it; a failure only delays the service's view.
    async fn principal_settled(&mut self) {
        if let Ok(Some(info)) = self.principal().await {
            if info.published_version == info.state.version {
                let _ = self.principal_refresh().await;
            }
        }
    }

    async fn controller_register(
        &mut self,
        generation: u32,
        name: Option<String>,
        delegation: DelegationAuthority,
    ) -> Result<()> {
        let account = self.account()?.clone();
        let info = self.principal().await?.ok_or(Error::NotFound)?;
        ensure(generation == next_generation(&info), Error::VersionConflict)?;
        let seed = self.controller_seed(generation, true)?;
        let state = self.refresh(account.home, &account.id).await?;
        let request_id = Hash::new(random_id());
        let message = controller_pop_message(
            account.home,
            &account.id,
            generation,
            &delegation,
            request_id,
        );
        let command = AccountCommand::RegisterController {
            generation,
            public_key: Hash::new(signing_public(&seed)),
            name,
            delegation,
            proof: Ed25519Signature::new(sign(&seed, message.as_slice())),
        };
        self.mutate(command, Some((request_id, state.info.account_version)))
            .await?;
        self.principal_settled().await;
        Ok(())
    }

    /// The seed of controller `generation`, created in the vault on first
    /// use with `create`.
    pub(crate) fn controller_seed(
        &mut self,
        generation: u32,
        create: bool,
    ) -> Result<Zeroizing<Key>> {
        let account = self.account()?.id.to_string();
        let id = controller_item(&account, generation);
        let item = Item {
            kind: "key".into(),
            title: format!("dMsg controller #{generation}"),
            tags: vec!["dmsg-controller".into()],
            body: format!("Agent Delegation controller key of {account}, generation {generation}."),
            username: String::new(),
            secret: String::new(),
            url: String::new(),
            favorite: false,
            created_at: now_ms(),
            updated_at: now_ms(),
            file: None,
            resolved_conflicts: None,
        };
        self.vault_key(&id, create.then_some(item))
    }

    /// The 32-byte seed kept in vault item `id`; with `new`, an absent item
    /// is created from it with a fresh random seed.
    pub(crate) fn vault_key(&mut self, id: &str, new: Option<Item>) -> Result<Zeroizing<Key>> {
        let item = match (self.vault_show(id), new) {
            (Ok(entry), _) if !entry.deleted => entry.item,
            (Err(Error::NotFound), Some(mut item)) => {
                item.secret = b64(&crate::crypto::random_key());
                self.vault_item_with_id(id, &item)?;
                item
            }
            (Ok(_) | Err(Error::NotFound), _) => return Err(Error::NotFound),
            (Err(error), _) => return Err(error),
        };
        let seed = Zeroizing::new(unb64(&item.secret)?);
        let seed: Key = seed
            .as_slice()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        Ok(Zeroizing::new(seed))
    }

    /// Ask to grant `subject` a credential signed by controller `generation`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an invalid subject, scope,
    /// audience or lifetime, and `Error::Forbidden` for a retired or
    /// unknown controller.
    #[allow(clippy::too_many_arguments)]
    pub async fn delegation_grant_request(
        &mut self,
        source: &str,
        generation: u32,
        subject: &str,
        scopes: Vec<String>,
        audiences: Vec<String>,
        days: u64,
        relationship: Option<String>,
    ) -> Result<Json> {
        self.keys()?;
        let account = self.account()?.id.to_string();
        let info = self.principal().await?.ok_or(Error::NotFound)?;
        ensure_valid((1..=MAX_GRANT_DAYS).contains(&days), "days")?;
        check_list(&scopes, "scopes", |s| s.len() <= 64)?;
        check_list(&audiences, "audiences", |a| validate_audience(a).is_ok())?;
        let subject_id: AgentId = subject
            .parse()
            .map_err(|_| dmsg_types::invalid("subject"))?;
        let now = now_ms();
        let kind = DelegationKind::Grant {
            id: format!("{account}.{}", b64(&random_id()[..16])),
            subject: subject_id.to_string(),
            scopes,
            audiences,
            expires_at: now + days * DAY,
            relationship,
        };
        self.delegation_prepare(source, &info, generation, kind, now)
    }

    /// Ask to revoke credential `id` with controller `generation`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for another principal's credential.
    pub async fn delegation_revoke_request(
        &mut self,
        source: &str,
        generation: u32,
        id: &str,
        reason: Option<String>,
    ) -> Result<Json> {
        self.keys()?;
        let account = self.account()?.id.to_string();
        ensure_valid(id.starts_with(&format!("{account}.")), "id")?;
        let info = self.principal().await?.ok_or(Error::NotFound)?;
        let kind = DelegationKind::Revoke {
            id: id.into(),
            reason,
        };
        self.delegation_prepare(source, &info, generation, kind, now_ms())
    }

    fn delegation_prepare(
        &mut self,
        source: &str,
        info: &PrincipalInfo,
        generation: u32,
        kind: DelegationKind,
        now: u64,
    ) -> Result<Json> {
        ensure(
            info.state
                .controllers
                .iter()
                .any(|c| c.generation == generation && c.retired_at.is_none()),
            Error::Forbidden,
        )?;
        let job = DelegationJob {
            id: hex::encode(random_id()),
            generation,
            kind,
            stage: Stage::Prepared,
            envelope: None,
            credential: None,
            error: None,
            created_at: now,
        };
        self.save_delegation_job(&job)?;
        let request = self.approval_create(
            source,
            crate::approval::Action::Delegation {
                job: job.id.clone(),
                digest: job.digest(),
            },
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// The journal of a delegation event.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown job.
    pub fn delegation(&self, id: &str) -> Result<DelegationJob> {
        self.delegation_job(id)
    }

    /// Every delegation event journaled here, newest first.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn delegations(&self) -> Result<Vec<Json>> {
        let keys = self.keys()?;
        let mut jobs: Vec<DelegationJob> = self
            .workspace
            .store
            .sealed_keys(JOURNAL)?
            .iter()
            .filter_map(|key| self.workspace.private(keys, key).transpose())
            .collect::<Result<_>>()?;
        jobs.sort_by_key(|j| std::cmp::Reverse(j.created_at));
        Ok(jobs.iter().map(DelegationJob::view).collect())
    }

    /// Sign an approved event, journal it and submit it; a signed event is
    /// only resubmitted.
    ///
    /// # Errors
    /// Returns `Error::ExecutionUnknown` while the service's answer is not
    /// known, `Error::Forbidden` for a retired controller or a device
    /// without `FormalApprove`, and `Error::InvalidInput` naming the
    /// service's refusal.
    pub(crate) async fn delegation_execute(&mut self, id: &str) -> Result<DelegationJob> {
        let mut job = self.delegation_job(id)?;
        match job.stage {
            Stage::Prepared => {
                self.delegation_sign(&mut job).await?;
                self.delegation_submit(job).await
            }
            Stage::Signed => self.delegation_submit(job).await,
            Stage::Accepted => Ok(job),
            Stage::Failed => Err(refusal(job.error.as_deref().unwrap_or("failed"))),
        }
    }

    async fn delegation_sign(&mut self, job: &mut DelegationJob) -> Result<()> {
        let account = self.account()?.clone();
        let info = self.principal().await?.ok_or(Error::NotFound)?;
        let controller = info
            .state
            .controllers
            .iter()
            .find(|c| c.generation == job.generation && c.retired_at.is_none())
            .ok_or(Error::Forbidden)?
            .clone();
        // Signing for the principal is a formal act of this device.
        let security = self.refresh_security(account.home, &account.id).await?;
        ensure(
            security
                .active(&self.workspace.meta.device_id)?
                .input
                .capabilities
                .contains(&Capability::FormalApprove),
            Error::Forbidden,
        )?;
        let seed = self.controller_seed(job.generation, false)?;
        ensure(
            signing_public(&seed) == *controller.public_key,
            Error::IntegrityFailed,
        )?;
        let nonce_key = format!("{NONCE}{}", job.generation);
        let next: u64 = self
            .workspace
            .private(self.keys()?, &nonce_key)?
            .unwrap_or(1);
        let (envelope, next) = sign_delegation(
            &seed,
            &info.principal_id,
            &job.kind,
            controller.valid_from,
            next,
            now_ms(),
        )?;
        job.envelope = Some(envelope);
        job.stage = Stage::Signed;
        // The nonce and the signed envelope are kept together, before any
        // network: a crash can only resubmit the same bytes.
        let keys = self.keys()?;
        self.workspace.store.atomic(|_| {
            self.workspace.put_private(keys, &nonce_key, &next)?;
            self.workspace
                .put_private(keys, &format!("{JOURNAL}{}", job.id), &*job)
        })
    }

    async fn delegation_submit(&mut self, mut job: DelegationJob) -> Result<DelegationJob> {
        let envelope = job.envelope.clone().ok_or(Error::IntegrityFailed)?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| Error::Unavailable(format!("http: {e}")))?;
        let service = DelegationClient::with_client(self.config.relay_origin(), http);
        match service.submit_delegation_event(&envelope).await {
            Ok(credential) => {
                ensure(
                    credential.id == job.kind.id() && credential.event_id == envelope.hash,
                    Error::IntegrityFailed,
                )?;
                job.credential = Some(credential);
                job.stage = Stage::Accepted;
                job.error = None;
                self.save_delegation_job(&job)?;
                Ok(job)
            }
            Err(SdkError::HttpStatus {
                status,
                code,
                max_seen_nonce,
                ..
            }) if status < 500 && !matches!(status, 408 | 429) => {
                let code = code.unwrap_or_else(|| format!("http_{status}"));
                // Every device holding the key signs as the same actor:
                // continue above the service's nonce, within the bound.
                if let Some(max) = max_seen_nonce {
                    let nonce_key = format!("{NONCE}{}", job.generation);
                    let next: u64 = self
                        .workspace
                        .private(self.keys()?, &nonce_key)?
                        .unwrap_or(1);
                    let mut nonces = ClientNonceManager::with_next(next).map_err(sdk)?;
                    if nonces
                        .observe_max_nonce_header(&max, now_ms() as i64)
                        .is_ok()
                    {
                        self.workspace
                            .put_private(self.keys()?, &nonce_key, &nonces.peek())?;
                    }
                }
                job.stage = Stage::Failed;
                job.error = Some(code.clone());
                self.save_delegation_job(&job)?;
                Err(refusal(&code))
            }
            // A transient answer or a lost reply says nothing about an
            // earlier acceptance; the same bytes go again later.
            Err(SdkError::HttpStatus { status, code, .. }) => {
                job.error = Some(code.unwrap_or_else(|| format!("http_{status}")));
                self.save_delegation_job(&job)?;
                Err(Error::ExecutionUnknown)
            }
            Err(_) => Err(Error::ExecutionUnknown),
        }
    }

    /// The principal's credentials as the delegation service holds them.
    ///
    /// # Errors
    /// Relay errors as returned.
    pub async fn delegation_credentials(&mut self) -> Result<Vec<Json>> {
        let base = format!("{}/delegations", self.base()?);
        let mut credentials = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let path = match &cursor {
                Some(cursor) => format!("{base}?limit=100&cursor={cursor}"),
                None => format!("{base}?limit=100"),
            };
            let page = self.relay_get(&path).await?;
            credentials.extend(page["result"].as_array().cloned().unwrap_or_default());
            match page["next_cursor"].as_str() {
                Some(next) if credentials.len() < 10_000 => cursor = Some(next.into()),
                _ => return Ok(credentials),
            }
        }
    }
}

fn sdk(error: SdkError) -> Error {
    Error::InvalidInput(error.to_string())
}
