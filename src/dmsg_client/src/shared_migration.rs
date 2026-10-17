//! The official successor of a shared old channel
//! (`docs/protocol/channel_migration_zh.md`, "唯一旧频道继承").
//!
//! A frozen manager proposes a new owner and a signed genesis for the
//! channel identified by `(legacy canister, channel id)`; every frozen
//! manager approves that exact version on the old site with the old
//! identity; the relay's directory then fixes the mapping and the owner
//! activates the channel. Old members claim their new account and device
//! with an old-identity approval, join through an ordinary invitation, and
//! receive the channel's history through a history grant sealed to that
//! device: one scoped archive with only this channel's content and keys.
//!
//! The relay's directory is never trusted: every view is checked here
//! against the frozen evidence, the old approvals and the signed commands.

use crate::channel::{FileRef, Upload};
use crate::client::Client;
use crate::cloud::Evidence;
use crate::codec::{b64, canonical, sha256, unb64};
use crate::crypto::{HpkeEnvelope, Key, hpke_open, hpke_seal, open, random_key, seal};
use crate::evidence::Verified;
use crate::files::{FileManifest, MAX_FILE, Source, read_file, store_file};
use crate::legacy::{self, Archive, Scope};
use crate::legacy_proof::{
    self, Approval, MemberClaim, SharedProposal, SharedSource, SourceInput, Trust, source_key,
};
use crate::migration::{MIGRATION, REPORT_PAGE};
use crate::now_ms;
use crate::upload::random_hex;
use dmsg_types::{AccountId, Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::str::FromStr;
use zeroize::Zeroizing;

/// Frozen source file format, as the old site exports it.
pub const SOURCE_FILE: &str = "dmsg-legacy-shared-source/1";
/// Draft format.
pub const DRAFT: &str = "dmsg-shared-migration-draft/1";
/// Approval request format, opened on the old site.
pub const REQUEST: &str = "dmsg-legacy-approval-request/1";
const PROPOSAL: &str = "dmsg-legacy-shared-proposal/1";
const CLAIM: &str = "dmsg-legacy-member-claim/1";
const GRANT: &str = "dmsg-legacy-history-grant/1";
const STAGES: [&str; 4] = ["reserving", "pending", "committed", "active"];

/// The old site's export of a channel's frozen ACL and name roles.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    /// [`SOURCE_FILE`].
    pub format: String,
    /// The old channel canister.
    pub source: String,
    /// The old name-identity canister.
    pub identity: String,
    /// The cutover, hex.
    pub cutover: String,
    /// The frozen pages.
    pub input: SourceInput,
}

/// A command as the relay stores it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Signed {
    /// The COSE_Sign1 command, base64url.
    pub cose_sign1: String,
}

/// The owner's signed genesis with the security evidence it was signed under.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GenesisAuthorization {
    /// The genesis command.
    pub signed: Signed,
    /// The owner's certified security then.
    pub evidence: Evidence,
    /// When it was signed.
    pub approved_at: u64,
}

/// One proposal version, kept by its owner and handed to the managers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    /// [`DRAFT`].
    pub format: String,
    /// The proposal.
    pub proposal: SharedProposal,
    /// The frozen source it is about.
    pub source_file: SourceFile,
    /// The new channel's genesis.
    pub genesis: GenesisAuthorization,
}

/// A stored signed command with the signer's evidence.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Stored {
    /// The command.
    pub signed: Signed,
    /// The signer's certified security when the relay stored it.
    pub evidence: Evidence,
    /// When the relay stored it.
    pub stored_at: u64,
}

/// A manager's recorded approval.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Vote {
    /// The account that submitted it.
    pub account: String,
    /// The frozen manager.
    pub manager: String,
    /// The old identity's approval.
    pub approval: Approval,
}

/// A member's recorded claim.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClaimRecord {
    /// The claim.
    pub claim: MemberClaim,
    /// The old identity's approval.
    pub approval: Approval,
    /// The claiming account joined the new channel.
    pub joined: bool,
    /// The claiming device's signed command.
    pub acceptance: Stored,
}

/// What a history grant covers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HistoryScope {
    /// [`GRANT`].
    pub format: String,
    /// The directory.
    pub directory_key: String,
    /// The proposal.
    pub proposal_digest: String,
    /// The frozen source.
    pub source_digest: String,
    /// Random, hex.
    pub grant_id: String,
    /// The new channel.
    pub channel_id: String,
    /// Its epoch the parts were uploaded in.
    pub epoch: u64,
    /// Its head then.
    pub control_head: String,
    /// The old member.
    pub member: String,
    /// The receiving account.
    pub account: String,
    /// The receiving device.
    pub device: String,
    /// The receiving account's security epoch.
    pub security_epoch: u64,
    /// SHA-256 of the scoped archive, hex.
    pub archive_digest: String,
    /// The old Principal whose archive it was cut from.
    pub principal: String,
}

/// One recipient of a grant key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GrantRecipient {
    /// `device:<account>:<device>`.
    pub recipient: String,
    /// The grant key, sealed to the device.
    pub envelope: HpkeEnvelope,
}

/// One uploaded part.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GrantPart {
    /// The channel upload.
    pub upload_id: String,
    /// SHA-256 of its sealed manifest.
    pub manifest_digest: String,
}

/// `dmsg-legacy-history-grant/1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HistoryGrant {
    /// What it covers.
    pub scope: HistoryScope,
    /// The parts' file references, sealed under the grant key.
    pub manifest: String,
    /// The one receiving device.
    pub recipients: Vec<GrantRecipient>,
    /// The parts.
    pub parts: Vec<GrantPart>,
}

/// A grant as the directory stores it.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredGrant {
    /// The grant.
    pub grant: HistoryGrant,
    /// The granting command.
    pub signed: Signed,
    /// The granter's evidence.
    pub evidence: Evidence,
    /// When the relay stored it.
    pub stored_at: u64,
}

/// The relay's directory record of one old channel.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SharedView {
    /// [`source_key`].
    pub key: String,
    /// The current proposal.
    pub proposal: SharedProposal,
    /// Its digest.
    pub proposal_digest: String,
    /// The frozen source; replaced by the locally verified one.
    pub source: Json,
    /// The frozen pages.
    pub material: SourceInput,
    /// The new channel's genesis.
    pub genesis: GenesisAuthorization,
    /// `reserving`, `pending`, `committed` or `active`.
    pub stage: String,
    /// Approvals by manager.
    pub votes: BTreeMap<String, Vote>,
    /// Claims by old member.
    #[serde(default)]
    pub claims: BTreeMap<String, ClaimRecord>,
    /// History grants by ID.
    #[serde(default)]
    pub grants: BTreeMap<String, StoredGrant>,
}

/// A grant being prepared, journaled so a retry sends the same parts.
#[derive(Serialize, Deserialize)]
struct GrantJob {
    grant: HistoryGrant,
    uploads: Vec<Upload>,
}

fn grant_context(scope: &HistoryScope) -> (&'static str, &HistoryScope) {
    ("dmsg/legacy-history-grant/1", scope)
}

fn grant_key_context<'a>(
    scope: &'a HistoryScope,
    recipient: &'a str,
) -> (&'static str, &'a HistoryScope, &'a str) {
    ("dmsg/legacy-history-grant-key/1", scope, recipient)
}

fn to_json<T: Serialize>(value: &T) -> Json {
    serde_json::to_value(value).expect("serializable")
}

fn from_json<T: for<'de> Deserialize<'de>>(value: Json) -> Result<T> {
    serde_json::from_value(value).map_err(|_| Error::IntegrityFailed)
}

/// The old-site links of an approval request.
fn request_links(request: &Json) -> Json {
    let fragment = b64(request.to_string().as_bytes());
    json!(
        legacy::ORIGINS
            .iter()
            .map(|o| format!("{o}/legacy#approve={fragment}"))
            .collect::<Vec<_>>()
    )
}

impl Client {
    /// The reviewed freeze and the trust to check its evidence.
    async fn legacy_trust(&mut self) -> Result<(crate::config::LegacyConfig, Vec<u8>)> {
        let legacy = self
            .config
            .legacy
            .clone()
            .ok_or_else(|| Error::Unavailable("legacy freeze is not configured".into()))?;
        let root_key = self.reader().await?.root_key();
        Ok((legacy, root_key))
    }

    /// Verify an old site's frozen source file against the reviewed freeze.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for an unreviewed channel, identity or
    /// cutover and an error for evidence that does not verify.
    pub async fn shared_source(&mut self, file: &SourceFile) -> Result<SharedSource> {
        let (legacy, root_key) = self.legacy_trust().await?;
        ensure(
            file.format == SOURCE_FILE
                && legacy.channels.contains(&file.source)
                && file.identity == legacy.identity
                && file.cutover == hex::encode(legacy.cutover),
            Error::Forbidden,
        )?;
        legacy_proof::verify_source(
            &file.input,
            &file.source,
            &file.identity,
            &Trust {
                root_key: &root_key,
                cutover: &legacy.cutover,
                now: now_ms(),
            },
        )
    }

    /// Propose this account as the owner of the old channel's successor:
    /// sign the new channel's genesis and keep the draft.
    ///
    /// # Errors
    /// As [`Self::shared_source`].
    pub async fn shared_prepare(&mut self, file: &SourceFile, version: u64) -> Result<Draft> {
        ensure_valid((1..=1000).contains(&version), "version")?;
        let source = self.shared_source(file).await?;
        let (channel, cose) = self
            .legacy_genesis(&format!("旧频道 {} 的继承", source.channel))
            .await?;
        let state = self.cloud_state().await?;
        let draft = Draft {
            format: DRAFT.into(),
            proposal: SharedProposal {
                format: PROPOSAL.into(),
                source: source.source.clone(),
                channel: source.channel,
                source_digest: source.digest.clone(),
                version,
                owner: self.me()?,
                channel_id: channel.clone(),
                genesis_digest: hex::encode(*sha256(&unb64(&cose)?)),
                nonce: random_hex(),
            },
            source_file: file.clone(),
            genesis: GenesisAuthorization {
                signed: Signed { cose_sign1: cose },
                evidence: state.evidence,
                approved_at: now_ms(),
            },
        };
        self.save_job(&channel, "legacy-draft", &draft)?;
        Ok(draft)
    }

    /// Check a draft: its frozen source, its proposal and the owner's
    /// signed genesis of the proposed channel.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` when any part disagrees.
    pub async fn shared_check_draft(&mut self, draft: &Draft) -> Result<SharedSource> {
        let source = self.shared_source(&draft.source_file).await?;
        let proposal = &draft.proposal;
        ensure(
            draft.format == DRAFT
                && proposal.format == PROPOSAL
                && proposal.source == source.source
                && proposal.channel == source.channel
                && proposal.source_digest == source.digest
                && proposal.genesis_digest
                    == hex::encode(*sha256(&unb64(&draft.genesis.signed.cose_sign1)?)),
            Error::IntegrityFailed,
        )?;
        let trust = self.trust().await?;
        let verified: Verified<Json> = trust.command(
            &draft.genesis.signed.cose_sign1,
            &draft.genesis.evidence,
            draft.genesis.approved_at,
            Some("dmsg/channel/genesis/v1"),
            now_ms(),
        )?;
        ensure(
            verified.account.to_string() == proposal.owner
                && verified.body.payload["channel_id"] == proposal.channel_id.as_str(),
            Error::IntegrityFailed,
        )?;
        Ok(source)
    }

    /// The request a frozen manager approves on the old site, with links
    /// that open it there.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for anyone but a frozen manager.
    pub async fn shared_manager_request(&mut self, draft: &Draft, manager: &str) -> Result<Json> {
        let source = self.shared_check_draft(draft).await?;
        ensure(
            source.managers.iter().any(|m| m == manager),
            Error::Forbidden,
        )?;
        let request = json!({
            "format": REQUEST,
            "kind": "manager",
            "proposal": draft.proposal,
            "identity": manager,
            "digest": hex::encode(draft.proposal.manager_consent(manager)),
            "expires_at": now_ms() + legacy_proof::APPROVAL_MS - 60_000,
        });
        Ok(json!({ "request": request, "links": request_links(&request) }))
    }

    /// Check an old approval of `digest` by the frozen identity `identity`.
    async fn check_approval(
        &mut self,
        source: &SharedSource,
        identity: &str,
        digest: &[u8; 32],
        approval: &Approval,
        at: u64,
    ) -> Result<()> {
        let (legacy, root_key) = self.legacy_trust().await?;
        let caller = legacy_proof::verify_approval(
            approval,
            digest,
            source,
            &Trust {
                root_key: &root_key,
                cutover: &legacy.cutover,
                now: at,
            },
        )?;
        ensure(source.controls(identity, &caller), Error::Forbidden)
    }

    /// Send one directory command, journaled under the new channel.
    async fn directory_post(
        &mut self,
        proposal: &SharedProposal,
        route: &str,
        action: &str,
        payload: Json,
    ) -> Result<SharedView> {
        let key = source_key(&proposal.source, proposal.channel);
        let result = self
            .post_journaled(
                &proposal.channel_id,
                &format!(
                    "legacy:{route}:{}",
                    hex::encode(*sha256(&canonical(&payload)))
                ),
                &format!("/v1/legacy/{key}"),
                action,
                payload,
                &format!("/{route}"),
            )
            .await?;
        self.shared_check_view(from_json(result)?).await
    }

    /// Submit a draft with the first manager's approval.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless this account is the proposed owner
    /// and the approval is the manager's.
    pub async fn shared_propose(
        &mut self,
        draft: &Draft,
        manager: &str,
        approval: &Approval,
    ) -> Result<SharedView> {
        let source = self.shared_check_draft(draft).await?;
        ensure(
            draft.proposal.owner == self.me()? && source.managers.iter().any(|m| m == manager),
            Error::Forbidden,
        )?;
        let digest = draft.proposal.manager_consent(manager);
        self.check_approval(&source, manager, &digest, approval, now_ms())
            .await?;
        self.directory_post(
            &draft.proposal,
            "proposals",
            "dmsg/legacy/propose/v1",
            json!({
                "proposal": draft.proposal,
                "source": draft.source_file.input,
                "genesis": draft.genesis,
                "manager": manager,
                "approval": approval,
            }),
        )
        .await
    }

    /// Record another frozen manager's approval of the same version.
    ///
    /// # Errors
    /// As [`Self::shared_propose`], for any frozen manager.
    pub async fn shared_consent(
        &mut self,
        draft: &Draft,
        manager: &str,
        approval: &Approval,
    ) -> Result<SharedView> {
        let source = self.shared_check_draft(draft).await?;
        ensure(
            source.managers.iter().any(|m| m == manager),
            Error::Forbidden,
        )?;
        let digest = draft.proposal.manager_consent(manager);
        self.check_approval(&source, manager, &digest, approval, now_ms())
            .await?;
        self.directory_post(
            &draft.proposal,
            "consents",
            "dmsg/legacy/consent/v1",
            json!({
                "proposal_digest": draft.proposal.digest(),
                "manager": manager,
                "approval": approval,
            }),
        )
        .await
    }

    /// Check a directory record, from the relay or handed over by another
    /// participant: its source, proposal and genesis, every vote, claim and
    /// grant. A committed record must carry every frozen manager's vote.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for anything inconsistent and
    /// `Error::Expired` for an expired vote of an uncommitted proposal.
    pub async fn shared_check_view(&mut self, mut view: SharedView) -> Result<SharedView> {
        let (legacy, _) = self.legacy_trust().await?;
        let draft = Draft {
            format: DRAFT.into(),
            proposal: view.proposal.clone(),
            source_file: SourceFile {
                format: SOURCE_FILE.into(),
                source: view.proposal.source.clone(),
                identity: legacy.identity.clone(),
                cutover: hex::encode(legacy.cutover),
                input: view.material.clone(),
            },
            genesis: view.genesis.clone(),
        };
        let source = self.shared_check_draft(&draft).await?;
        let committed = ["committed", "active"].contains(&view.stage.as_str());
        ensure(
            STAGES.contains(&view.stage.as_str())
                && view.key == source_key(&source.source, source.channel)
                && view.proposal_digest == view.proposal.digest(),
            Error::IntegrityFailed,
        )?;
        let now = now_ms();
        for (manager, vote) in &view.votes {
            ensure(
                *manager == vote.manager && source.managers.contains(manager),
                Error::IntegrityFailed,
            )?;
            ensure(vote.approval.expires_at > now || committed, Error::Expired)?;
            let at = match committed {
                true => now.min(vote.approval.expires_at - 1),
                false => now,
            };
            let digest = view.proposal.manager_consent(manager);
            self.check_approval(&source, manager, &digest, &vote.approval, at)
                .await?;
        }
        if committed {
            ensure(
                source.managers.iter().all(|m| view.votes.contains_key(m)),
                Error::RecoveryIncomplete,
            )?;
        }
        let trust = self.trust().await?;
        for (member, record) in &view.claims {
            let claim = &record.claim;
            ensure(
                *member == claim.member
                    && source.members.contains(member)
                    && claim.format == CLAIM
                    && claim.proposal_digest == view.proposal_digest,
                Error::IntegrityFailed,
            )?;
            let at = now.min(record.approval.expires_at - 1);
            self.check_approval(&source, member, &claim.digest(), &record.approval, at)
                .await?;
            let accepted: Verified<Json> = trust.command(
                &record.acceptance.signed.cose_sign1,
                &record.acceptance.evidence,
                record.acceptance.stored_at,
                Some("dmsg/legacy/claim/v1"),
                now,
            )?;
            ensure(
                accepted.account.to_string() == claim.account
                    && hex::encode(*accepted.device) == claim.device
                    && accepted.body.payload["claim"] == to_json(claim),
                Error::IntegrityFailed,
            )?;
        }
        for (id, stored) in &view.grants {
            let verified: Verified<Json> = trust.command(
                &stored.signed.cose_sign1,
                &stored.evidence,
                stored.stored_at,
                Some("dmsg/legacy/history/v1"),
                now,
            )?;
            ensure(
                *id == stored.grant.scope.grant_id
                    && verified.body.payload == to_json(&stored.grant),
                Error::IntegrityFailed,
            )?;
        }
        view.source = to_json(&source);
        Ok(view)
    }

    /// The directory record of `key`, checked.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` to an account that has not taken part yet:
    /// it checks a record another participant hands over instead.
    pub async fn shared_view(&mut self, key: &str) -> Result<SharedView> {
        ensure_valid(
            key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit()),
            "key",
        )?;
        let value = self.relay_get(&format!("/v1/legacy/{key}")).await?;
        self.shared_check_view(from_json(value)?).await
    }

    /// Fix the mapping once every frozen manager approved this version.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless this account is the owner and
    /// `Error::RecoveryIncomplete` while a vote is missing.
    pub async fn shared_commit(&mut self, key: &str) -> Result<SharedView> {
        let view = self.shared_view(key).await?;
        let source: SharedSource = from_json(view.source.clone())?;
        ensure(view.proposal.owner == self.me()?, Error::Forbidden)?;
        ensure(
            source.managers.iter().all(|m| view.votes.contains_key(m)),
            Error::RecoveryIncomplete,
        )?;
        self.directory_post(
            &view.proposal,
            "commit",
            "dmsg/legacy/commit/v1",
            json!({ "proposal_digest": view.proposal_digest }),
        )
        .await
    }

    /// Create the committed successor and open its first epoch.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless this account is the owner of a
    /// committed mapping.
    pub async fn shared_activate(&mut self, key: &str) -> Result<SharedView> {
        let view = self.shared_view(key).await?;
        ensure(
            ["committed", "active"].contains(&view.stage.as_str())
                && view.proposal.owner == self.me()?,
            Error::Forbidden,
        )?;
        let genesis = view.proposal.genesis_digest.clone();
        let name = format!("旧频道 {} 的继承", view.proposal.channel);
        self.remember_channel(&view.proposal.channel_id, &name, &genesis)?;
        self.channel_activate_legacy(
            &view.proposal.channel_id,
            json!({ "source_key": view.key, "proposal_digest": view.proposal_digest }),
        )
        .await?;
        self.shared_view(key).await
    }

    /// The request an old member approves on the old site to claim this
    /// account and device, with links that open it there.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for anyone outside the frozen members.
    pub async fn shared_claim_request(&mut self, view: SharedView, member: &str) -> Result<Json> {
        let view = self.shared_check_view(view).await?;
        let source: SharedSource = from_json(view.source.clone())?;
        ensure(source.members.iter().any(|m| m == member), Error::Forbidden)?;
        let state = self.cloud_state().await?;
        let claim = MemberClaim {
            format: CLAIM.into(),
            proposal_digest: view.proposal_digest.clone(),
            member: member.into(),
            account: self.me()?,
            device: self.device_hex(),
            hpke_pub: hex::encode(*self.workspace.meta.hpke_pub),
            security_epoch: state.snapshot.security_epoch,
        };
        let request = json!({
            "format": REQUEST,
            "kind": "member",
            "proposal": view.proposal,
            "claim": claim,
            "identity": member,
            "digest": hex::encode(claim.digest()),
            "expires_at": now_ms() + legacy_proof::APPROVAL_MS - 60_000,
        });
        Ok(json!({ "request": request, "links": request_links(&request) }))
    }

    /// Submit a member claim with the old member's approval.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a claim of another account or device,
    /// or an approval of anyone else.
    pub async fn shared_claim(
        &mut self,
        view: SharedView,
        claim: &MemberClaim,
        approval: &Approval,
    ) -> Result<SharedView> {
        let view = self.shared_check_view(view).await?;
        let source: SharedSource = from_json(view.source.clone())?;
        ensure(
            claim.account == self.me()?
                && claim.device == self.device_hex()
                && claim.proposal_digest == view.proposal_digest,
            Error::Forbidden,
        )?;
        self.check_approval(&source, &claim.member, &claim.digest(), approval, now_ms())
            .await?;
        self.directory_post(
            &view.proposal,
            "claims",
            "dmsg/legacy/claim/v1",
            json!({ "claim": claim, "approval": approval }),
        )
        .await
    }

    /// Record that this account accepted its invitation and holds the
    /// channel's current epoch.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` before that.
    pub async fn shared_joined(&mut self, key: &str) -> Result<SharedView> {
        let view = self.shared_view(key).await?;
        self.directory_post(
            &view.proposal,
            "joined",
            "dmsg/legacy/joined/v1",
            json!({ "proposal_digest": view.proposal_digest }),
        )
        .await
    }

    /// Hand a joined member the channel's history from one of this
    /// account's kept archives: cut to the frozen channel, uploaded to the
    /// new channel and sealed to the member's claimed device. The grant's
    /// ID follows from the archive and the claimed device, so a retry
    /// continues the same grant.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless this account manages the new
    /// channel and the member joined, and errors of [`legacy::scoped`].
    pub async fn shared_grant(
        &mut self,
        key: &str,
        archive: &str,
        member: &str,
    ) -> Result<SharedView> {
        let view = self.shared_view(key).await?;
        let source: SharedSource = from_json(view.source.clone())?;
        let claim = view
            .claims
            .get(member)
            .filter(|c| c.joined)
            .ok_or(Error::Forbidden)?
            .claim
            .clone();
        let channel = view.proposal.channel_id.clone();
        let grant_id = hex::encode(*dmsg_protocol::digest(
            "dmsg/legacy-grant-id/1",
            &(&view.key, archive, member, &claim.account, &claim.device),
        ));
        let job_key = format!("legacy-grant:{grant_id}");
        let job = match self.job::<GrantJob>(&channel, &job_key)? {
            Some(job) => job,
            None => {
                let ledger = self.channel_pull(&channel, None, None).await?;
                let me = self.me()?;
                ensure(
                    ledger.members.get(&me).is_some_and(|m| {
                        matches!(crate::channel::role_name(m.role), "owner" | "admin")
                    }),
                    Error::Forbidden,
                )?;
                let target =
                    AccountId::from_str(&claim.account).map_err(|_| Error::IntegrityFailed)?;
                let security = self.peer_security(&target).await?;
                let device = security
                    .devices
                    .iter()
                    .find(|(id, d)| {
                        hex::encode(***id) == claim.device
                            && d.revoked_at.is_none()
                            && d.input
                                .capabilities
                                .contains(&dmsg_types::user::Capability::ContentSign)
                    })
                    .map(|(_, d)| d.clone())
                    .ok_or(Error::AuthRequired)?;
                let (_, plain) = self.kept_archive(archive)?;
                let kept = Archive::decode(&plain)?;
                let scoped = legacy::scoped(
                    &kept,
                    &Scope {
                        source: &source.source,
                        channel: source.channel,
                        dek_digest: &source.dek_digest,
                        messages: (source.message_start, source.message_end),
                        digest: &source.digest,
                    },
                    now_ms(),
                )?;
                let encoded = Zeroizing::new(scoped.encode()?);
                let scope = HistoryScope {
                    format: GRANT.into(),
                    directory_key: view.key.clone(),
                    proposal_digest: view.proposal_digest.clone(),
                    source_digest: source.digest.clone(),
                    grant_id: grant_id.clone(),
                    channel_id: channel.clone(),
                    epoch: ledger.epoch,
                    control_head: ledger.head.clone(),
                    member: member.into(),
                    account: claim.account.clone(),
                    device: claim.device.clone(),
                    security_epoch: security.snapshot.security_epoch,
                    archive_digest: hex::encode(*sha256(&encoded)),
                    principal: scoped.inventory.principal.clone(),
                };
                let mut files = vec![];
                let mut uploads = vec![];
                for (index, chunk) in encoded.chunks(crate::migration::PART).enumerate() {
                    let file = store_file(
                        &self.workspace.store,
                        &format!("legacy-grant-{grant_id}-{index}.cbor"),
                        "application/cbor",
                        chunk,
                    )?;
                    let (reference, upload) =
                        self.prepare_upload(&channel, &ledger, &file, &self.workspace.store)?;
                    files.push(reference);
                    uploads.push(upload);
                }
                let grant_key = Zeroizing::new(random_key());
                let recipient = format!("device:{}:{}", claim.account, claim.device);
                let grant = HistoryGrant {
                    manifest: seal(&grant_key, &canonical(&files), &grant_context(&scope)),
                    recipients: vec![GrantRecipient {
                        envelope: hpke_seal(
                            &device.input.hpke_pub,
                            &grant_key[..],
                            &grant_key_context(&scope, &recipient),
                        )?,
                        recipient,
                    }],
                    parts: files
                        .iter()
                        .map(|f| GrantPart {
                            upload_id: f.upload_id.clone(),
                            manifest_digest: f.manifest_digest.clone(),
                        })
                        .collect(),
                    scope,
                };
                let job = GrantJob { grant, uploads };
                self.save_job(&channel, &job_key, &job)?;
                job
            }
        };
        for upload in &job.uploads {
            self.channel_upload(&channel, upload, Source::Store).await?;
        }
        self.directory_post(
            &view.proposal,
            "history",
            "dmsg/legacy/history/v1",
            to_json(&job.grant),
        )
        .await
    }

    /// Receive a history grant to this device: open it, fetch and check its
    /// parts, and keep the scoped archive like an imported one.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a grant to another account or device,
    /// and `Error::IntegrityFailed` for anything that does not match the
    /// grant or the frozen source.
    pub async fn shared_receive(&mut self, key: &str, grant_id: &str) -> Result<Json> {
        let view = self.shared_view(key).await?;
        let source: SharedSource = from_json(view.source.clone())?;
        let stored = view.grants.get(grant_id).ok_or(Error::NotFound)?.clone();
        let scope = &stored.grant.scope;
        let me = self.me()?;
        let device = self.device_hex();
        ensure(
            scope.format == GRANT
                && scope.account == me
                && scope.device == device
                && scope.directory_key == view.key
                && scope.proposal_digest == view.proposal_digest
                && scope.source_digest == source.digest
                && view
                    .claims
                    .get(&scope.member)
                    .is_some_and(|c| c.claim.account == me),
            Error::Forbidden,
        )?;
        let granter = {
            let trust = self.trust().await?;
            let verified: Verified<Json> = trust.command(
                &stored.signed.cose_sign1,
                &stored.evidence,
                stored.stored_at,
                Some("dmsg/legacy/history/v1"),
                now_ms(),
            )?;
            verified.account.to_string()
        };
        let channel = scope.channel_id.clone();
        self.channel_pull(&channel, None, None).await?;
        let recipient = format!("device:{me}:{device}");
        let envelope = match stored.grant.recipients.as_slice() {
            [only] if only.recipient == recipient => &only.envelope,
            _ => return Err(Error::IntegrityFailed),
        };
        let seed: Zeroizing<Key> = self.keys()?.hpke.clone();
        let grant_key = hpke_open(&seed, envelope, &grant_key_context(scope, &recipient))?;
        let grant_key: Key = grant_key
            .as_slice()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        let files: Vec<FileRef> = crate::codec::decode(
            &open(&grant_key, &stored.grant.manifest, &grant_context(scope))?,
            65_536,
        )?;
        ensure(
            !files.is_empty()
                && files.len() <= legacy::MAX_ARCHIVE / crate::migration::PART
                && files.len() == stored.grant.parts.len()
                && files.iter().zip(&stored.grant.parts).all(|(f, p)| {
                    f.epoch == scope.epoch
                        && f.upload_id == p.upload_id
                        && f.manifest_digest == p.manifest_digest
                }),
            Error::IntegrityFailed,
        )?;
        let mut plain = Zeroizing::new(Vec::new());
        for file in &files {
            let manifest: FileManifest = self.channel_file(&channel, file, &granter).await?;
            read_file(
                &self.workspace.store,
                &manifest,
                MAX_FILE,
                Some(&mut *plain),
            )?;
        }
        ensure(
            hex::encode(*sha256(&plain)) == scope.archive_digest,
            Error::IntegrityFailed,
        )?;
        let archive = Archive::decode(&plain)?;
        // Cutting again checks the frozen wrapper and keeps out any master
        // key or other channel's history the granter might have added.
        let again = legacy::scoped(
            &archive,
            &Scope {
                source: &source.source,
                channel: source.channel,
                dek_digest: &source.dek_digest,
                messages: (source.message_start, source.message_end),
                digest: &source.digest,
            },
            now_ms(),
        )?;
        ensure(
            again.keys.len() == archive.keys.len()
                && again.inventory.objects.len() == archive.inventory.objects.len()
                && archive
                    .keys
                    .iter()
                    .all(|k| ["channel_kek", "channel_dek"].contains(&k.purpose.as_str())),
            Error::Forbidden,
        )?;
        let content_id = archive.content_id();
        for (id, _) in self.workspace.store.heads(MIGRATION)? {
            if self.archive_record(&id)?.content_id == content_id {
                return self.legacy_report(&id, 0, REPORT_PAGE);
            }
        }
        let checked = legacy::verify(&archive);
        let id = self.keep_archive(&plain, &archive, &scope.archive_digest, checked.gaps.len())?;
        self.legacy_report(&id, 0, REPORT_PAGE)
    }
}
