//! Formal attestations (`docs/protocol/README.md`): text, digest and file
//! statements, and registered application actions, signed by this device
//! and recorded by the user home with a certified execution receipt.
//!
//! Approving one signs the exact COSE Sig_structure and the device approval,
//! writes both to an encrypted journal, then submits them. A lost reply is
//! reconciled by the execution ID with `get_attestation`; the device never
//! signs the same intent twice. Each finished attestation keeps its artifact
//! and receipt, so it can be exported as a self-contained bundle that
//! [`verify_bundle`] checks without the device.

use crate::approval::{Action, APPROVAL_TTL_MS};
use crate::client::Client;
use crate::codec::{b64, canonical, decode, unb64};
use crate::crypto::sign;
use crate::now_ms;
use candid::Principal;
use dmsg_protocol::{
    app_action_statement, approval_message, attest_approval_command, execution_request_id,
    prepare_attestation, validate_statement, verify_attestation, ATTEST_APPROVAL_DOMAIN,
};
use dmsg_types::{
    app_action::AppAction, ensure, user::Capability, AccountId, AppActionAttestRequest, Approval,
    AttestRequest, CertifiedBatch, Ed25519Signature, Error, ExecutionReceipt, Hash, Result,
    SignedArtifact, Statement, StatementContent,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use std::path::Path;

/// The application origin of this device's own attestations (plan D-19).
pub const DMSG_ORIGIN: &str = "https://dmsg.net";
/// Format of an exported attestation bundle.
pub const BUNDLE_FORMAT: &str = "dmsg-attestation/1";
/// A home accepts approvals at most five minutes ahead; leave a skew margin.
const APPROVAL_MS: u64 = 270_000;
const PREFIX: &str = "attest:";

/// What is attested.
pub enum Subject {
    /// A document statement under an application origin.
    Statement {
        /// The statement.
        statement: Statement,
        /// The checked origin.
        origin: String,
    },
    /// A registered application action; it carries its own origin.
    Action(Box<AppAction>),
}

/// The signed request, as journaled before it is sent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Signed {
    /// `attest`.
    Document(AttestRequest),
    /// `attest_app_action`.
    Action(AppActionAttestRequest),
}

impl Signed {
    fn approval(&self) -> &Approval {
        match self {
            Signed::Document(r) => &r.approval,
            Signed::Action(r) => &r.approval,
        }
    }

    fn account(&self) -> AccountId {
        match self {
            Signed::Document(r) => r.account_id,
            Signed::Action(r) => r.account_id,
        }
    }

    fn origin(&self) -> &str {
        match self {
            Signed::Document(r) => &r.origin,
            Signed::Action(r) => &r.action.origin,
        }
    }
}

/// Where an attestation stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Signed and journaled; the home's answer is not known.
    Unknown,
    /// The home recorded it; artifact and receipt are kept.
    Complete,
    /// The home refused it.
    Failed,
    /// Its approval lapsed without ever executing.
    Expired,
}

/// The journal of one attestation, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Attestation {
    /// The approval request.
    pub approval: String,
    /// The signed request.
    pub request: Signed,
    /// Where it stands.
    pub stage: Stage,
    /// The recorded artifact.
    pub artifact: Option<SignedArtifact>,
    /// The certified execution receipt.
    pub receipt: Option<CertifiedBatch>,
    /// The home's refusal.
    pub error: Option<String>,
    /// Unix milliseconds.
    pub created_at: u64,
}

impl Attestation {
    /// The execution ID, hex.
    pub fn execution_id(&self) -> String {
        hex::encode(*self.request.approval().request_id)
    }
}

/// A self-contained attestation: the artifact and the certified receipt
/// that binds it to the account. Byte fields are base64url.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    /// [`BUNDLE_FORMAT`].
    pub format: String,
    /// The signing account.
    pub account: String,
    /// The account's user home, which certified the receipt.
    pub home: String,
    /// The execution ID, hex.
    pub request_id: String,
    /// Tagged COSE_Sign1.
    pub cose_sign1: String,
    /// Public COSE_Key of the device.
    pub cose_key: String,
    /// Canonical CBOR of the certified receipt batch.
    pub receipt: String,
}

/// A JSON rendering of a statement for review.
pub fn statement_view(statement: &Statement) -> Json {
    let (profile, content) = match &statement.content {
        StatementContent::Text(text) => ("text", json!({"text": text})),
        StatementContent::Digest {
            sha256,
            content_type,
            location,
        } => (
            "digest",
            json!({"sha256": hex::encode(**sha256), "content_type": content_type, "location": location}),
        ),
        StatementContent::FileStatement {
            text,
            sha256,
            content_type,
            location,
        } => (
            "file_statement",
            json!({"text": text, "sha256": hex::encode(**sha256), "content_type": content_type, "location": location}),
        ),
        StatementContent::AppAction(action) => (
            "app_action",
            json!({
                "app_id": action.app_id,
                "origin": action.origin,
                "command": action.command.name,
                "args": action.command.args.0.iter().map(|a| json!({"name": a.name, "value": format!("{:?}", a.value)})).collect::<Vec<_>>(),
                "operation_id": hex::encode(*action.operation_id),
                "expires_at": action.expires_at_ms,
            }),
        ),
    };
    json!({
        "profile": profile,
        "issuer": statement.issuer,
        "subject": statement.subject,
        "issued_at": statement.issued_at,
        "content": content,
    })
}

/// SHA-256 of a file, read in 1 MiB blocks.
///
/// # Errors
/// Returns `Error::NotFound` or `Error::Unavailable` when the file cannot be read.
pub fn file_sha256(path: &Path) -> Result<Hash> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::NotFound,
        _ => Error::Unavailable(format!("file: {e}")),
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| Error::Unavailable(format!("file: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(Hash::new(hasher.finalize().into()))
}

/// A statement this device's clients may ask it to attest.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "profile", rename_all = "snake_case", deny_unknown_fields)]
pub enum StatementInput {
    /// Text, 1 to 4,096 bytes.
    Text {
        /// The text.
        text: String,
        /// What the text is about.
        subject: Option<String>,
    },
    /// A SHA-256 digest of a file, given as the file or its hex digest.
    Digest {
        /// Hex SHA-256 when no file is given.
        sha256: Option<String>,
        /// Optional media type.
        content_type: Option<String>,
        /// Optional URI of the original.
        location: Option<String>,
        /// What the file is about.
        subject: Option<String>,
    },
    /// Text about one file.
    FileStatement {
        /// The text.
        text: String,
        /// Optional media type.
        content_type: Option<String>,
        /// Optional URI of the file.
        location: Option<String>,
        /// What the file is about.
        subject: Option<String>,
    },
}

/// What [`verify_bundle`] found, check by check (PRD S05).
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Verification {
    /// The statement.
    pub statement: Json,
    /// `matches_file`, `text`, `digest_only` or `action`.
    pub content: &'static str,
    /// The device's signature over the statement verifies.
    pub signature: bool,
    /// RFC 9679 thumbprint of the signing key, hex.
    pub key_fingerprint: String,
    /// The account the home certified as authorizing it.
    pub account: String,
    /// The approving device, hex.
    pub device_id: String,
    /// The account's security epoch at approval.
    pub security_epoch: u64,
    /// Approval time, Unix milliseconds.
    pub approved_at: u64,
    /// The origin the device checked.
    pub origin: String,
    /// The receipt certificate's time, Unix milliseconds.
    pub certified_at: u64,
    /// The device's status now: `active`, `revoked` or `unknown` (not checked).
    pub current: String,
}

/// Verify a bundle as kept evidence: the signature, the receipt certificate
/// under `root_key` and the home, their binding, and the statement against
/// `file` when given. The device's present status is the caller's to add.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for anything that does not verify, and
/// for a file whose SHA-256 differs from the statement's.
pub fn verify_bundle(
    bundle: &Bundle,
    file: Option<&Path>,
    root_key: &[u8],
) -> Result<Verification> {
    ensure(bundle.format == BUNDLE_FORMAT, Error::UnsupportedProtocol)?;
    let account: AccountId = bundle
        .account
        .parse()
        .map_err(|_| dmsg_types::invalid("account"))?;
    let home = Principal::from_text(&bundle.home).map_err(|_| dmsg_types::invalid("home"))?;
    let request = Hash::new(
        hex::decode(&bundle.request_id)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| dmsg_types::invalid("request_id"))?,
    );
    let artifact = SignedArtifact {
        cose_sign1: unb64(&bundle.cose_sign1)?.into(),
        cose_key: unb64(&bundle.cose_key)?.into(),
    };
    let receipt: CertifiedBatch = decode(&unb64(&bundle.receipt)?, 262_144)?;
    let (statement, leaf, certified_at): (Statement, ExecutionReceipt, u64) =
        verify_attestation(&artifact, &receipt, &account, request, home, root_key)?;
    ensure(
        statement.issuer.ends_with(&account.to_string()) && leaf.issuer == statement.issuer,
        Error::IntegrityFailed,
    )?;
    let content = match (&statement.content, file) {
        (
            StatementContent::Digest { sha256, .. }
            | StatementContent::FileStatement { sha256, .. },
            Some(path),
        ) => {
            ensure(file_sha256(path)? == *sha256, Error::IntegrityFailed)?;
            "matches_file"
        }
        (StatementContent::Digest { .. } | StatementContent::FileStatement { .. }, None) => {
            "digest_only"
        }
        (StatementContent::Text(_), _) => "text",
        (StatementContent::AppAction(_), _) => "action",
    };
    Ok(Verification {
        statement: statement_view(&statement),
        content,
        signature: true,
        key_fingerprint: hex::encode(*leaf.public_key_fingerprint),
        account: account.to_string(),
        device_id: hex::encode(*leaf.device_id),
        security_epoch: leaf.security_epoch,
        approved_at: leaf.approved_at,
        origin: leaf.origin,
        certified_at,
        current: "unknown".into(),
    })
}

impl Client {
    fn attestation(&self, execution: &str) -> Result<Option<Attestation>> {
        self.workspace
            .private(self.keys()?, &format!("{PREFIX}{execution}"))
    }

    fn save_attestation(&self, attestation: &Attestation) -> Result<()> {
        self.workspace.put_private(
            self.keys()?,
            &format!("{PREFIX}{}", attestation.execution_id()),
            attestation,
        )
    }

    /// Check that a file still has the digest its statement names.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` when the file changed.
    pub(crate) fn check_statement_file(&self, statement: &Statement, path: &Path) -> Result<()> {
        match &statement.content {
            StatementContent::Digest { sha256, .. }
            | StatementContent::FileStatement { sha256, .. } => {
                ensure(file_sha256(path)? == *sha256, Error::IntegrityFailed)
            }
            _ => Err(Error::InvalidInput("file".into())),
        }
    }

    /// Ask this device to attest a statement under [`DMSG_ORIGIN`]; the
    /// request waits in the approval center. `file` is hashed now and again
    /// on approval.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and statement validation errors.
    pub fn attest_request(
        &mut self,
        source: &str,
        input: StatementInput,
        file: Option<&Path>,
    ) -> Result<Json> {
        self.keys()?;
        let issuer = self.account()?.issuer.clone();
        let digest_of = |sha256: Option<String>| -> Result<Hash> {
            match (file, sha256) {
                (Some(path), None) => file_sha256(path),
                (None, Some(text)) => hex::decode(&text)
                    .ok()
                    .and_then(|b| b.try_into().ok())
                    .map(Hash::new)
                    .ok_or_else(|| dmsg_types::invalid("sha256")),
                _ => Err(dmsg_types::invalid("give either a file or a sha256")),
            }
        };
        let (subject, content) = match input {
            StatementInput::Text { text, subject } => {
                ensure(file.is_none(), dmsg_types::invalid("file"))?;
                (subject, StatementContent::Text(text))
            }
            StatementInput::Digest {
                sha256,
                content_type,
                location,
                subject,
            } => (
                subject,
                StatementContent::Digest {
                    sha256: digest_of(sha256)?,
                    content_type,
                    location,
                },
            ),
            StatementInput::FileStatement {
                text,
                content_type,
                location,
                subject,
            } => (
                subject,
                StatementContent::FileStatement {
                    text,
                    sha256: digest_of(None)?,
                    content_type,
                    location,
                },
            ),
        };
        let statement = Statement {
            issuer,
            subject,
            issued_at: None,
            content,
        };
        validate_statement(&statement)?;
        let file = file
            .map(|p| {
                std::path::absolute(p)
                    .map(|p| p.to_string_lossy().into_owned())
                    .map_err(|e| Error::InvalidInput(format!("file: {e}")))
            })
            .transpose()?;
        let request = self.approval_create(
            source,
            Action::Attest {
                statement,
                origin: DMSG_ORIGIN.into(),
                file,
            },
            now_ms() + APPROVAL_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// Sign an attestation for an approved request and journal it; nothing
    /// is sent yet. Needs a login: the home accepts attestations only from a
    /// Principal bound to the account.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a login, `Error::Forbidden` for
    /// a device without `FormalApprove` or a purpose the account does not
    /// allow, and `Error::Locked` while the account freezes executions.
    pub(crate) async fn attest_prepare(
        &mut self,
        approval: &str,
        subject: Subject,
    ) -> Result<Attestation> {
        self.session()?;
        let account = self.account()?.clone();
        let state = self.refresh(account.home, &account.id).await?;
        let device_id = self.workspace.meta.device_id;
        let device = state.active(&device_id)?;
        ensure(
            device
                .input
                .capabilities
                .contains(&Capability::FormalApprove),
            Error::Forbidden,
        )?;
        let policy = &state.info.sensitive_policy;
        ensure(!policy.frozen, Error::Locked)?;
        let now = now_ms();
        let mut approval_fields = Approval {
            device_id,
            security_epoch: state.info.security_epoch,
            sequence: device.next_sequence,
            request_id: execution_request_id(
                &account.id,
                state.info.security_epoch,
                device_id,
                device.next_sequence,
            ),
            expires_at: now + APPROVAL_MS,
            signature: Ed25519Signature::new([0; 64]),
        };
        let (statement, origin) = match &subject {
            Subject::Statement { statement, origin } => (statement.clone(), origin.clone()),
            Subject::Action(action) => {
                approval_fields.expires_at = approval_fields.expires_at.min(action.expires_at_ms);
                (
                    Statement {
                        issuer: state.info.issuer.clone(),
                        subject: None,
                        issued_at: None,
                        content: StatementContent::AppAction(action.clone()),
                    },
                    action.origin.clone(),
                )
            }
        };
        ensure(now < approval_fields.expires_at, Error::Expired)?;
        ensure(statement.issuer == state.info.issuer, Error::Forbidden)?;
        let prepared = prepare_attestation(&statement, &self.workspace.meta.signing_pub)?;
        ensure(
            policy.allowed_purposes.contains(&prepared.purpose),
            Error::Forbidden,
        )?;
        let keys = self.keys()?;
        let signature = Ed25519Signature::new(sign(&keys.signing, &prepared.to_be_signed));
        let message = approval_message(
            account.home,
            &account.id,
            ATTEST_APPROVAL_DOMAIN,
            &attest_approval_command(&statement, &origin, &signature),
            &approval_fields,
        );
        approval_fields.signature = Ed25519Signature::new(sign(&keys.signing, message.as_slice()));
        let request = match subject {
            Subject::Statement { statement, origin } => Signed::Document(AttestRequest {
                account_id: account.id,
                statement,
                origin,
                signature,
                approval: approval_fields,
            }),
            Subject::Action(action) => {
                let request = AppActionAttestRequest {
                    account_id: account.id,
                    issuer: state.info.issuer.clone(),
                    action: *action,
                    signature,
                    approval: approval_fields,
                };
                ensure(
                    app_action_statement(&request)? == statement,
                    Error::IntegrityFailed,
                )?;
                Signed::Action(request)
            }
        };
        let attestation = Attestation {
            approval: approval.into(),
            request,
            stage: Stage::Unknown,
            artifact: None,
            receipt: None,
            error: None,
            created_at: now,
        };
        // Journaled before it is sent: the home may record it even if the
        // reply is lost.
        self.save_attestation(&attestation)?;
        Ok(attestation)
    }

    /// Send a journaled attestation and keep its artifact and receipt.
    ///
    /// # Errors
    /// Returns `Error::ExecutionUnknown` when the reply is lost and the
    /// home's refusal otherwise.
    pub(crate) async fn attest_dispatch(
        &mut self,
        mut attestation: Attestation,
    ) -> Result<Attestation> {
        let home = self.account()?.home;
        let ic = self.session()?.ic.clone();
        let reply: Result<SignedArtifact> = match &attestation.request {
            Signed::Document(request) => ic.update(home, "attest", (request,)).await?,
            Signed::Action(request) => ic.update(home, "attest_app_action", (request,)).await?,
        };
        match reply {
            Ok(artifact) => self.attest_finish(attestation, artifact).await,
            Err(error) => {
                attestation.stage = Stage::Failed;
                attestation.error = Some(crate::error_code(&error));
                self.save_attestation(&attestation)?;
                Err(error)
            }
        }
    }

    /// Check an artifact against what was signed, then fetch and check its
    /// receipt.
    async fn attest_finish(
        &mut self,
        mut attestation: Attestation,
        artifact: SignedArtifact,
    ) -> Result<Attestation> {
        let account = self.account()?.clone();
        let approval = attestation.request.approval().clone();
        let receipt: Result<CertifiedBatch> = self
            .session()?
            .ic
            .query(
                account.home,
                "get_execution_receipt",
                (account.id, approval.request_id),
            )
            .await?;
        let receipt = receipt?;
        let root_key = self.session()?.ic.root_key();
        let (_, leaf, _) = verify_attestation(
            &artifact,
            &receipt,
            &attestation.request.account(),
            approval.request_id,
            account.home,
            &root_key,
        )?;
        ensure(
            leaf.device_id == approval.device_id
                && leaf.security_epoch == approval.security_epoch
                && leaf.expires_at == approval.expires_at
                && leaf.origin == attestation.request.origin()
                && leaf.public_key_fingerprint
                    == prepare_attestation_thumbprint(&self.workspace.meta.signing_pub)?,
            Error::IntegrityFailed,
        )?;
        attestation.stage = Stage::Complete;
        attestation.artifact = Some(artifact);
        attestation.receipt = Some(receipt);
        self.save_attestation(&attestation)?;
        Ok(attestation)
    }

    /// Settle an attestation whose outcome is unknown, by its execution ID.
    ///
    /// # Errors
    /// Returns `Error::ExecutionUnknown` while the home's answer is still
    /// open, and the recorded refusal of a failed one.
    pub(crate) async fn attest_reconcile(&mut self, execution: &str) -> Result<Attestation> {
        let mut attestation = self.attestation(execution)?.ok_or(Error::NotFound)?;
        match attestation.stage {
            Stage::Complete => return Ok(attestation),
            Stage::Failed | Stage::Expired => {
                return Err(match attestation.error.as_deref() {
                    Some("EXPIRED") | None => Error::Expired,
                    Some(_) => Error::Forbidden,
                })
            }
            Stage::Unknown => {}
        }
        let account = self.account()?.clone();
        let approval = attestation.request.approval().clone();
        let found: Result<SignedArtifact> = self
            .session()?
            .ic
            .query(
                account.home,
                "get_attestation",
                (account.id, approval.request_id),
            )
            .await?;
        match found {
            Ok(artifact) => self.attest_finish(attestation, artifact).await,
            Err(Error::ResultExpired) => {
                let state = self.refresh(account.home, &account.id).await?;
                let device = state.device(&approval.device_id);
                // A certified time past the deadline with the sequence
                // unconsumed proves it never ran and never can.
                if state.security.certified_at >= approval.expires_at
                    && device.map(|d| d.next_sequence) == Some(approval.sequence)
                {
                    attestation.stage = Stage::Expired;
                    attestation.error = Some("EXPIRED".into());
                    self.save_attestation(&attestation)?;
                    return Err(Error::Expired);
                }
                if now_ms() < approval.expires_at {
                    // Still valid: sending the same signed request again is
                    // safe, the home deduplicates it.
                    return self.attest_dispatch(attestation).await;
                }
                Err(Error::ExecutionUnknown)
            }
            Err(error) => Err(error),
        }
    }

    /// Finished attestations, newest first.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn attest_history(&self) -> Result<Vec<Json>> {
        let keys = self.keys()?;
        let mut items = Vec::new();
        for key in self.workspace.store.sealed_keys(PREFIX)? {
            let Some(attestation) = self.workspace.private::<Attestation>(keys, &key)? else {
                continue;
            };
            let statement = match &attestation.request {
                Signed::Document(r) => r.statement.clone(),
                Signed::Action(r) => app_action_statement(r)?,
            };
            items.push(json!({
                "execution": attestation.execution_id(),
                "approval": attestation.approval,
                "stage": attestation.stage,
                "origin": attestation.request.origin(),
                "created_at": attestation.created_at,
                "error": attestation.error,
                "statement": statement_view(&statement),
            }));
        }
        items.sort_by_key(|i| std::cmp::Reverse(i["created_at"].as_u64()));
        Ok(items)
    }

    /// The bundle of a finished attestation.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown or unfinished one.
    pub fn attest_bundle(&self, execution: &str) -> Result<Bundle> {
        let attestation = self.attestation(execution)?.ok_or(Error::NotFound)?;
        let (Some(artifact), Some(receipt)) = (&attestation.artifact, &attestation.receipt) else {
            return Err(Error::NotFound);
        };
        let account = self.account()?;
        Ok(Bundle {
            format: BUNDLE_FORMAT.into(),
            account: attestation.request.account().to_string(),
            home: account.home.to_text(),
            request_id: attestation.execution_id(),
            cose_sign1: b64(&artifact.cose_sign1),
            cose_key: b64(&artifact.cose_key),
            receipt: b64(&canonical(receipt)),
        })
    }

    /// [`verify_online`] with this client's reader.
    ///
    /// # Errors
    /// As [`verify_bundle`].
    pub async fn attest_verify(
        &mut self,
        bundle: &Bundle,
        file: Option<&Path>,
    ) -> Result<Verification> {
        let ic = self.reader().await?;
        verify_online(&ic, bundle, file).await
    }
}

/// [`verify_bundle`] under the agent's root key, then the approving device's
/// present status from the account's certified security, when it can be read.
///
/// # Errors
/// As [`verify_bundle`].
pub async fn verify_online(
    ic: &crate::ic::Ic,
    bundle: &Bundle,
    file: Option<&Path>,
) -> Result<Verification> {
    let mut report = verify_bundle(bundle, file, &ic.root_key())?;
    let account: AccountId = report
        .account
        .parse()
        .map_err(|_| dmsg_types::invalid("account"))?;
    let home = Principal::from_text(&bundle.home).map_err(|_| dmsg_types::invalid("home"))?;
    let device = Hash::new(
        hex::decode(&report.device_id)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(Error::IntegrityFailed)?,
    );
    // Offline, the present status stays unknown; the evidence stands alone.
    let batch: Result<Result<CertifiedBatch>> = ic
        .query(home, "security_snapshot_batch", (vec![account],))
        .await;
    type Bundled = (
        dmsg_types::user::SecuritySnapshot,
        std::collections::BTreeMap<Hash, dmsg_types::user::Device>,
    );
    let devices: Result<Result<Bundled>> = ic.query(home, "get_device_bundle", (account,)).await;
    if let (Ok(Ok(batch)), Ok(Ok((_, devices)))) = (batch, devices) {
        let security = crate::account::verify_security(
            home,
            &account,
            &batch,
            devices,
            &ic.root_key(),
            now_ms(),
        )?;
        report.current = match security.device(&device) {
            Some(d) if d.revoked_at.is_none() => "active",
            Some(_) => "revoked",
            None => "unknown",
        }
        .into();
    }
    Ok(report)
}

fn prepare_attestation_thumbprint(signing_pub: &Hash) -> Result<Hash> {
    dmsg_protocol::key_thumbprint(&dmsg_protocol::public_cose_key(
        &[],
        signing_pub.as_slice(),
    )?)
}

#[cfg(test)]
#[path = "attest_tests.rs"]
mod tests;
