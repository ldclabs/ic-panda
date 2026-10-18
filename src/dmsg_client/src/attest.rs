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
//!
//! The account keeps every finished attestation as an [`ATTESTATION`]
//! content object, encrypted under the content root and synced like vault
//! items: the account's other devices, and a device recovered after all
//! were lost, list and export it too.

use crate::approval::{APPROVAL_TTL_MS, Action, AttestFile, file_name, file_stamp};
use crate::client::Client;
use crate::codec::{b64, canonical, decode, unb64};
use crate::content::MAX_OBJECT;
use crate::crypto::sign;
use crate::hold::Hold;
use crate::now_ms;
use candid::Principal;
use dmsg_protocol::{
    ATTEST_APPROVAL_DOMAIN, app_action_statement, approval_message, attest_approval_command,
    execution_receipt_key, execution_request_id, match_execution_receipt, prepare_attestation,
    statement_purpose, validate_statement, verify_artifact, verify_attestation,
};
use dmsg_types::app_action::{
    ActionArg, ActionLabel, ActionSchema, ActionValue, FieldSchema, FieldType,
};
use dmsg_types::billing::ExecutionUsage;
use dmsg_types::{
    AccountId, AppActionAttestRequest, Approval, AttestRequest, CertifiedBatch, Ed25519Signature,
    Error, ExecutionReceipt, Hash, Result, SignedArtifact, Statement, StatementContent,
    app_action::AppAction, ensure, user::Capability,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::path::Path;

/// An archived month of formal-execution usage, by its YYYYMM.
const USAGE: &str = "usage:";
/// The current month's usage as last read, for review between refreshes.
const USAGE_NOW: &str = "usage-now";
/// Content-object kind of the account's finished attestations.
pub const ATTESTATION: &str = "attestation";
/// Format of an [`ATTESTATION`] object's payload.
const EVIDENCE_FORMAT: &str = "dmsg-attestation-record/1";

/// The application origin of this device's own attestations (plan D-19).
pub const DMSG_ORIGIN: &str = "https://dmsg.net";
/// Format of an exported attestation bundle.
pub const BUNDLE_FORMAT: &str = "dmsg-attestation/1";
/// A home accepts approvals at most five minutes ahead; leave a skew margin.
const APPROVAL_MS: u64 = 270_000;
const PREFIX: &str = "attest:";

/// What is attested.
#[derive(Clone)]
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

/// An approved attestation of this device's own clients, run in steps so
/// that its network waits happen outside any lock on the client:
/// [`AttestRun::read`], [`Client::attest_sign`], [`AttestRun::send`] and
/// [`Client::attest_settle`]. [`Client::approval_respond`] runs them in turn.
pub struct AttestRun {
    request: crate::approval::Request,
    subject: Subject,
    file: Option<AttestFile>,
    ic: crate::ic::Ic,
    home: Principal,
    account: AccountId,
    attestation: Option<Attestation>,
}

/// What [`AttestRun::read`] found before signing.
pub struct RunRead {
    file: Option<Result<Hash>>,
    inspected: Option<Result<()>>,
    account: Result<crate::account::AccountState>,
}

/// What [`AttestRun::send`] got back.
pub struct RunSent {
    reply: Result<Result<SignedArtifact>>,
    receipt: Option<Result<Result<CertifiedBatch>>>,
}

impl AttestRun {
    /// The approval request's ID.
    pub fn id(&self) -> &str {
        &self.request.id
    }

    /// What signing needs read first: a local file's SHA-256, the product
    /// authority's verdict on an action, and the account's state.
    pub async fn read(&self) -> RunRead {
        let file = self
            .file
            .as_ref()
            .and_then(|f| f.path.as_deref())
            .map(|path| file_sha256(Path::new(path)));
        let inspected = match &self.subject {
            // The product authority accepts this exact action first, as
            // for a page's `signAction`.
            Subject::Action(action) => Some(
                self.ic
                    .update::<_, Result<()>>(
                        self.home,
                        "inspect_app_action",
                        (self.account, action.as_ref()),
                    )
                    .await
                    .and_then(|verdict| verdict),
            ),
            Subject::Statement { .. } => None,
        };
        let account = crate::account::read_account(&self.ic, self.home, &self.account).await;
        RunRead {
            file,
            inspected,
            account,
        }
    }

    /// Submit the signed attestation and read its receipt.
    pub async fn send(&self) -> RunSent {
        let Some(attestation) = &self.attestation else {
            return RunSent {
                reply: Err(Error::Expired),
                receipt: None,
            };
        };
        let reply: Result<Result<SignedArtifact>> = match &attestation.request {
            Signed::Document(request) => self.ic.update(self.home, "attest", (request,)).await,
            Signed::Action(request) => {
                self.ic
                    .update(self.home, "attest_app_action", (request,))
                    .await
            }
        };
        let receipt = match &reply {
            Ok(Ok(_)) => Some(
                self.ic
                    .query(
                        self.home,
                        "get_execution_receipt",
                        (self.account, attestation.request.approval().request_id),
                    )
                    .await,
            ),
            _ => None,
        };
        RunSent { reply, receipt }
    }
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
    /// Its sequence was used, but the home no longer keeps the result: it
    /// may have been recorded, and its evidence is gone.
    Lost,
}

/// A finished attestation as the account keeps it, in an [`ATTESTATION`]
/// object named by its execution ID.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct Evidence {
    /// [`EVIDENCE_FORMAT`].
    format: String,
    /// The user home that certified the receipt.
    home: String,
    /// The journal, with its artifact and receipt.
    attestation: Attestation,
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

/// An action argument's value as plain text, without its schema.
fn plain_value(value: &ActionValue) -> String {
    match value {
        ActionValue::Nat(n) => n.to_string(),
        ActionValue::Bool(b) => b.to_string(),
        ActionValue::Text(text) | ActionValue::Choice(text) => text.clone(),
        ActionValue::Hash(hash) => hex::encode(**hash),
        ActionValue::Principal(principal) => principal.to_text(),
        ActionValue::Artifact(a) => format!("{} (sha256 {})", a.uri, hex::encode(*a.sha256)),
        ActionValue::Null => "-".into(),
        ActionValue::List(items) => items.iter().map(plain_value).collect::<Vec<_>>().join(", "),
        ActionValue::Record(args) => args
            .0
            .iter()
            .map(|a| format!("{}: {}", a.name, plain_value(&a.value)))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Labels by locale.
fn labels(labels: &[ActionLabel]) -> Json {
    Json::Object(
        labels
            .iter()
            .map(|l| (l.locale.clone(), Json::String(l.text.clone())))
            .collect(),
    )
}

fn rendered_fields(schema: &[FieldSchema], args: &[ActionArg]) -> Vec<Json> {
    schema
        .iter()
        .zip(args)
        .map(|(field, arg)| {
            json!({
                "name": field.name,
                "label": labels(&field.label),
                "value": rendered_value(&field.ty, &arg.value),
            })
        })
        .collect()
}

fn rendered_value(ty: &FieldType, value: &ActionValue) -> Json {
    match (ty, value) {
        (FieldType::Optional { .. }, ActionValue::Null) => json!({"type": "null"}),
        (FieldType::Optional { item }, value) => rendered_value(item, value),
        (FieldType::Bool { yes, no }, ActionValue::Bool(b)) => {
            json!({"type": "bool", "label": labels(if *b { yes } else { no })})
        }
        (FieldType::Text { multiline, .. }, ActionValue::Text(text)) => {
            json!({"type": "text", "text": text, "multiline": multiline})
        }
        (FieldType::Choice { options }, ActionValue::Choice(choice)) => json!({
            "type": "choice",
            "text": choice,
            "label": options.iter().find(|o| o.value == *choice).map(|o| labels(&o.label)),
        }),
        (FieldType::Artifact, ActionValue::Artifact(a)) => json!({
            "type": "artifact",
            "uri": a.uri,
            "sha256": hex::encode(*a.sha256),
            "content_type": a.content_type,
            "size": a.size,
        }),
        (FieldType::List { item, .. }, ActionValue::List(items)) => json!({
            "type": "list",
            "items": items.iter().map(|v| rendered_value(item, v)).collect::<Vec<_>>(),
        }),
        (FieldType::Record { fields }, ActionValue::Record(args)) => {
            json!({"type": "record", "fields": rendered_fields(&fields.0, &args.0)})
        }
        (FieldType::Hash, _) => json!({"type": "hash", "text": plain_value(value)}),
        (FieldType::Principal, _) => json!({"type": "principal", "text": plain_value(value)}),
        _ => json!({"type": "nat", "text": plain_value(value)}),
    }
}

/// An application action as its certified registration's schema renders it
/// (`docs/protocol/app-action.md`): the command's title, each argument's
/// label and typed value, and the files, every label by locale. `null`
/// when no schema came with the action or it is not the one the action
/// names: then only the raw command can be shown.
pub fn render_action(action: &AppAction, schema: Option<&[u8]>) -> Json {
    let Some(schema) = schema.and_then(|b| dmsg_protocol::decode_canonical::<ActionSchema>(b).ok())
    else {
        return Json::Null;
    };
    if dmsg_protocol::app_action::action_schema_hash(&schema) != action.schema_hash
        || dmsg_protocol::app_action::validate_action_command(action, &schema).is_err()
    {
        return Json::Null;
    }
    let Some(command) = schema
        .commands
        .iter()
        .find(|c| c.name == action.command.name)
    else {
        return Json::Null;
    };
    json!({
        "title": labels(&command.title),
        "fields": rendered_fields(&command.fields.0, &action.command.args.0),
        "files": action.files.iter().map(|f| json!({
            "name": f.display_name.clone().unwrap_or_else(|| f.file_id.clone()),
            "revision": f.revision,
            "sha256": hex::encode(*f.sha256),
            "size": f.byte_length,
            "media_type": f.media_type,
            "encrypted": matches!(f.representation, dmsg_types::app_action::ActionFileRepresentation::Encrypted),
        })).collect::<Vec<_>>(),
    })
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
                "args": action.command.args.0.iter().map(|a| json!({"name": a.name, "value": plain_value(&a.value)})).collect::<Vec<_>>(),
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

/// The file a digest or file statement is about.
#[derive(Clone, Copy, Debug)]
pub enum FileSource<'a> {
    /// A local file, hashed now and again on approval.
    Path(&'a Path),
    /// A version in the file library: the file's ID and the version's.
    Library(&'a str, &'a str),
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
    /// Text about one file, given as the file or its hex digest.
    FileStatement {
        /// The text.
        text: String,
        /// Hex SHA-256 when no file is given, such as a library version's.
        sha256: Option<String>,
        /// Optional media type.
        content_type: Option<String>,
        /// Optional URI of the file.
        location: Option<String>,
        /// What the file is about.
        subject: Option<String>,
    },
}

/// What [`verify_bundle`] found, check by check (PRD S05): a failed check
/// is reported with the others, and a device revoked later never undoes a
/// signature that verifies.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Verification {
    /// Every check passed, and the content does not contradict the file.
    pub valid: bool,
    /// The statement, once its signature verifies.
    pub statement: Option<Json>,
    /// `matches_file`, `mismatch`, `text`, `digest_only`, `action`, or
    /// `unknown` while the signature does not verify.
    pub content: &'static str,
    /// The device's signature over the statement verifies.
    pub signature: bool,
    /// A user home of the deployment certified the receipt.
    pub evidence: bool,
    /// The receipt names this artifact, its key, account and request.
    pub binding: bool,
    /// RFC 9679 thumbprint of the signing key, hex.
    pub key_fingerprint: Option<String>,
    /// The account the bundle names.
    pub account: String,
    /// The user home the bundle names.
    pub home: String,
    /// The approving device, hex.
    pub device_id: Option<String>,
    /// The account's security epoch at approval.
    pub security_epoch: Option<u64>,
    /// Approval time, Unix milliseconds.
    pub approved_at: Option<u64>,
    /// The origin the device checked.
    pub origin: Option<String>,
    /// The receipt certificate's time, Unix milliseconds.
    pub certified_at: Option<u64>,
    /// The device's status now: `active`, `revoked` or `unknown` (not checked).
    pub current: String,
}

impl Verification {
    /// The checks that failed, by name.
    pub fn failed(&self) -> Vec<&'static str> {
        [
            (!self.signature).then_some("signature"),
            (!self.evidence).then_some("evidence"),
            (!self.binding).then_some("binding"),
            (self.content == "mismatch").then_some("content"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Verify a bundle as kept evidence, check by check: the device's
/// signature; the receipt's certificate under `root_key` by one of the
/// deployment's user `homes` (any canister can certify a receipt); the
/// receipt's binding to the artifact, its key, account and request; and the
/// statement against `file` when given. The device's present status is the
/// caller's to add.
///
/// # Errors
/// Returns `Error::UnsupportedProtocol` for another format,
/// `Error::InvalidInput` for a malformed bundle, and the file's read
/// errors; a check that fails is reported, not returned.
pub fn verify_bundle(
    bundle: &Bundle,
    file: Option<&Path>,
    root_key: &[u8],
    homes: &[Principal],
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
    let statement = verify_artifact(&artifact).ok();
    let key = execution_receipt_key(&account, request);
    let leaf = homes
        .contains(&home)
        .then(|| {
            dmsg_protocol::authentication::verify_recorded_leaf(&receipt, &key, home, root_key)
        })
        .and_then(|found| found.ok())
        .and_then(|(value, at)| {
            dmsg_protocol::decode_canonical::<ExecutionReceipt>(&value)
                .ok()
                .map(|leaf| (leaf, at))
        });
    let binding = match (&statement, &leaf) {
        (Some(statement), Some((leaf, _))) => {
            match_execution_receipt(&artifact, leaf).is_ok()
                && leaf.account_id == account
                && leaf.request_id == request
                && statement.issuer.ends_with(&account.to_string())
                && leaf.issuer == statement.issuer
        }
        _ => false,
    };
    let content = match (statement.as_ref().map(|s| &s.content), file) {
        (None, _) => "unknown",
        (
            Some(
                StatementContent::Digest { sha256, .. }
                | StatementContent::FileStatement { sha256, .. },
            ),
            Some(path),
        ) => match file_sha256(path)? == *sha256 {
            true => "matches_file",
            false => "mismatch",
        },
        (Some(StatementContent::Digest { .. } | StatementContent::FileStatement { .. }), None) => {
            "digest_only"
        }
        (Some(StatementContent::Text(_)), _) => "text",
        (Some(StatementContent::AppAction(_)), _) => "action",
    };
    let mut report = Verification {
        valid: false,
        statement: statement.as_ref().map(statement_view),
        content,
        signature: statement.is_some(),
        evidence: leaf.is_some(),
        binding,
        key_fingerprint: leaf
            .as_ref()
            .map(|(l, _)| hex::encode(*l.public_key_fingerprint)),
        account: account.to_string(),
        home: home.to_text(),
        device_id: leaf.as_ref().map(|(l, _)| hex::encode(*l.device_id)),
        security_epoch: leaf.as_ref().map(|(l, _)| l.security_epoch),
        approved_at: leaf.as_ref().map(|(l, _)| l.approved_at),
        origin: leaf.as_ref().map(|(l, _)| l.origin.clone()),
        certified_at: leaf.as_ref().map(|(_, at)| *at),
        current: "unknown".into(),
    };
    report.valid = report.failed().is_empty();
    Ok(report)
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

    /// The SHA-256 of a library file's version and the file it describes.
    fn library_file(&mut self, file: &str, version: &str) -> Result<(Hash, AttestFile)> {
        let record = self.file_record(file)?;
        let found = self.version(file, version)?;
        let sha256 = hex::decode(&found.sha256)
            .ok()
            .and_then(|b| b.try_into().ok())
            .map(Hash::new)
            .ok_or(Error::IntegrityFailed)?;
        Ok((
            sha256,
            AttestFile {
                path: None,
                library: Some((file.into(), version.into())),
                name: record.name,
                size: found.size,
                modified: None,
            },
        ))
    }

    /// Check that the file a statement describes still has its digest: a
    /// local file hashed again, a library version by its record.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` when the file changed.
    pub(crate) fn check_attest_file(&self, statement: &Statement, file: &AttestFile) -> Result<()> {
        let expected = crate::approval::statement_digest(statement)
            .ok_or_else(|| Error::InvalidInput("file".into()))?;
        let found = match (&file.path, &file.library) {
            (Some(path), _) => file_sha256(Path::new(path))?,
            (None, Some((id, version))) => {
                let found = self.version(id, version)?;
                hex::decode(&found.sha256)
                    .ok()
                    .and_then(|b| b.try_into().ok())
                    .map(Hash::new)
                    .ok_or(Error::IntegrityFailed)?
            }
            (None, None) => return Err(Error::InvalidInput("file".into())),
        };
        ensure(found == expected, Error::IntegrityFailed)
    }

    /// Whether the file a statement describes looks as it did when hashed,
    /// without reading it: a local file by its size and time, a library
    /// version by its record.
    pub(crate) fn attest_file_unchanged(&self, statement: &Statement, file: &AttestFile) -> bool {
        match &file.path {
            Some(path) => {
                file_stamp(Path::new(path)).is_ok_and(|stamp| stamp == (file.size, file.modified))
            }
            None => self.check_attest_file(statement, file).is_ok(),
        }
    }

    /// Ask this device to attest a statement under [`DMSG_ORIGIN`]; the
    /// request waits in the approval center. A digest or file statement
    /// names its file by `file`, hashed now and checked again on approval,
    /// or gives the digest itself.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and statement validation errors.
    pub fn attest_request(
        &mut self,
        source: &str,
        input: StatementInput,
        file: Option<FileSource<'_>>,
    ) -> Result<Json> {
        self.keys()?;
        let issuer = self.account()?.issuer.clone();
        let described = match file {
            Some(FileSource::Path(path)) => {
                let path = std::path::absolute(path)
                    .map_err(|e| Error::InvalidInput(format!("file: {e}")))?;
                let (size, modified) = file_stamp(&path)?;
                let sha256 = file_sha256(&path)?;
                Some((
                    sha256,
                    AttestFile {
                        name: file_name(&path),
                        path: Some(path.to_string_lossy().into_owned()),
                        library: None,
                        size,
                        modified,
                    },
                ))
            }
            Some(FileSource::Library(id, version)) => Some(self.library_file(id, version)?),
            None => None,
        };
        let digest_of = |sha256: Option<String>| -> Result<Hash> {
            match (&described, sha256) {
                (Some((sha256, _)), None) => Ok(*sha256),
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
                ensure(described.is_none(), dmsg_types::invalid("file"))?;
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
                sha256,
                content_type,
                location,
                subject,
            } => (
                subject,
                StatementContent::FileStatement {
                    text,
                    sha256: digest_of(sha256)?,
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
        let request = self.approval_create(
            source,
            Action::Attest {
                statement,
                origin: DMSG_ORIGIN.into(),
                file: described.map(|(_, file)| file),
            },
            now_ms() + APPROVAL_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// Ask this device to attest an application action its product
    /// prepared (`docs/protocol/app-action.md`), given as canonical CBOR,
    /// as a page's `signAction` does but from this device: the current
    /// registration must admit it and it must name this account. The
    /// request waits in the approval center at most until the action
    /// expires; approving asks the product authority before signing.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked, `Error::InvalidInput` for bytes
    /// that are not an action or an action of another account, and the
    /// admission errors of `validate_action_admission`.
    pub async fn attest_action_request(&mut self, source: &str, bytes: &[u8]) -> Result<Json> {
        self.keys()?;
        let action: AppAction = dmsg_protocol::decode_canonical(bytes)
            .map_err(|_| Error::InvalidInput("action".into()))?;
        let account = self.account()?.clone();
        ensure(
            action.signing_account == account.id,
            Error::InvalidInput("ACCOUNT_MISMATCH".into()),
        )?;
        let app = self.app_registration(&mut None, &action.app_id).await?;
        let now = now_ms();
        dmsg_protocol::app_action::validate_action_admission(&action, &app, now)?;
        let expires_at = (now + APPROVAL_TTL_MS).min(action.expires_at_ms);
        let request = self.approval_create(
            source,
            Action::AttestAction {
                action: bytes.to_vec().into(),
                // The schema the review renders, from the certified
                // registration that admitted the action.
                schema: app.action_schema.as_ref().map(|s| canonical(s).into()),
            },
            expires_at,
        )?;
        self.approval_view(&request)
    }

    /// The product authority's verdict on an application action before
    /// this device signs it (`inspect_app_action`).
    pub(crate) async fn inspect_action(&mut self, action: &AppAction) -> Result<()> {
        let account = self.account()?.clone();
        let ic = self.session()?.ic.clone();
        let inspected: Result<()> = ic
            .update(account.home, "inspect_app_action", (account.id, action))
            .await?;
        inspected
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
        self.attest_sign_with(approval, subject, &state)
    }

    /// [`Self::attest_prepare`] with the account's state already read.
    fn attest_sign_with(
        &mut self,
        approval: &str,
        subject: Subject,
        state: &crate::account::AccountState,
    ) -> Result<Attestation> {
        let account = self.account()?.clone();
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
        attestation: Attestation,
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
        self.attest_complete(attestation, artifact, receipt?)
    }

    /// Check an artifact and its receipt against what was signed and keep
    /// both.
    fn attest_complete(
        &mut self,
        mut attestation: Attestation,
        artifact: SignedArtifact,
        receipt: CertifiedBatch,
    ) -> Result<Attestation> {
        let account = self.account()?.clone();
        let approval = attestation.request.approval().clone();
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
        // The account keeps it for its other devices; when it cannot yet,
        // such as while the root is being replaced, the next reconcile
        // pass does.
        let _ = self.attest_keep(&attestation);
        Ok(attestation)
    }

    /// The run of an approved attestation request; needs a login, since the
    /// home accepts attestations only from a Principal bound to the account.
    pub(crate) fn attest_run(&self, request: &crate::approval::Request) -> Result<AttestRun> {
        let ic = self.session()?.ic.clone();
        let account = self.account()?.clone();
        let (subject, file) = match &request.action {
            Action::Attest {
                statement,
                origin,
                file,
            } => (
                Subject::Statement {
                    statement: statement.clone(),
                    origin: origin.clone(),
                },
                file.clone(),
            ),
            Action::AttestAction { action, .. } => (
                Subject::Action(Box::new(dmsg_protocol::decode_canonical(action)?)),
                None,
            ),
            _ => return Err(Error::InvalidInput("not an attestation".into())),
        };
        Ok(AttestRun {
            request: request.clone(),
            subject,
            file,
            ic,
            home: account.home,
            account: account.id,
            attestation: None,
        })
    }

    /// Sign an approved attestation with what [`AttestRun::read`] found,
    /// and journal it before it is sent. A failure settles the request.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a file that changed, the product
    /// authority's refusal and [`Self::attest_prepare`]'s errors.
    pub fn attest_sign(&mut self, run: &mut AttestRun, read: RunRead) -> Result<()> {
        match self.attest_signed(run, read) {
            Ok(attestation) => {
                run.request.execution = Some(attestation.execution_id());
                run.request.started = true;
                run.attestation = Some(attestation);
                self.save_approval(&run.request)
            }
            Err(error) => {
                self.running.remove(&run.request.id);
                self.finish_approval(&mut run.request, Err(error))
                    .map(|_| ())
            }
        }
    }

    fn attest_signed(&mut self, run: &AttestRun, read: RunRead) -> Result<Attestation> {
        if let (Subject::Statement { statement, .. }, Some(file)) = (&run.subject, &run.file) {
            match read.file {
                Some(found) => ensure(
                    Some(found?) == crate::approval::statement_digest(statement),
                    Error::IntegrityFailed,
                )?,
                None => self.check_attest_file(statement, file)?,
            }
        }
        if let Some(inspected) = read.inspected {
            inspected?;
        }
        let state = read.account?;
        self.observe(&run.account, &state.security)?;
        self.attest_sign_with(&run.request.id, run.subject.clone(), &state)
    }

    /// Keep what [`AttestRun::send`] got back and settle the request.
    ///
    /// # Errors
    /// Returns the home's refusal, `Error::ExecutionUnknown` while the
    /// outcome is open, and `Error::IntegrityFailed` for an artifact or
    /// receipt that does not match what was signed.
    pub fn attest_settle(&mut self, run: AttestRun, sent: RunSent) -> Result<Json> {
        let AttestRun {
            mut request,
            attestation,
            ..
        } = run;
        self.running.remove(&request.id);
        let outcome = match (attestation, sent.reply) {
            (None, _) => Err(Error::Expired),
            (Some(_), Err(error)) => Err(error),
            (Some(mut attestation), Ok(Err(refusal))) => {
                attestation.stage = Stage::Failed;
                attestation.error = Some(crate::error_code(&refusal));
                self.save_attestation(&attestation)?;
                Err(refusal)
            }
            // Recorded: a receipt that could not be read yet is read by the
            // reconciler.
            (Some(attestation), Ok(Ok(artifact))) => match sent.receipt {
                Some(Ok(Ok(receipt))) => self
                    .attest_complete(attestation, artifact, receipt)
                    .map(|_| ()),
                _ => Err(Error::ExecutionUnknown),
            },
        };
        self.finish_approval(&mut request, outcome)
    }

    /// Keep a finished attestation as an [`ATTESTATION`] object named by
    /// its execution ID, unless the account already holds it.
    fn attest_keep(&mut self, attestation: &Attestation) -> Result<()> {
        let id = attestation.execution_id();
        if attestation.stage != Stage::Complete || self.workspace.store.head(&id)?.is_some() {
            return Ok(());
        }
        let evidence = Evidence {
            format: EVIDENCE_FORMAT.into(),
            home: self.account()?.home.to_text(),
            attestation: attestation.clone(),
        };
        self.write(ATTESTATION, id, None, false, &canonical(&evidence), None)
            .map(|_| ())
    }

    /// Keep every finished attestation of this device the account does not
    /// hold yet; returns how many were kept.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and the content writer's errors.
    pub(crate) fn attest_backfill(&mut self) -> Result<usize> {
        let mut kept = 0;
        for key in self.workspace.store.sealed_keys(PREFIX)? {
            let execution = &key[PREFIX.len()..];
            if self.workspace.store.head(execution)?.is_some() {
                continue;
            }
            if let Some(attestation) = self.workspace.private::<Attestation>(self.keys()?, &key)?
                && attestation.stage == Stage::Complete
            {
                self.attest_keep(&attestation)?;
                kept += 1;
            }
        }
        Ok(kept)
    }

    /// The account's kept attestations, by execution ID.
    fn kept_attestations(&self) -> Result<Vec<Evidence>> {
        let mut kept = Vec::new();
        for (id, _) in self.workspace.store.heads(ATTESTATION)? {
            let (_, record) = self.current(&id)?;
            if record.tombstone {
                continue;
            }
            let evidence: Evidence =
                decode(&record.open(self.environment(), self.roots()?)?, MAX_OBJECT)?;
            ensure(
                evidence.format == EVIDENCE_FORMAT && evidence.attestation.execution_id() == id,
                Error::IntegrityFailed,
            )?;
            kept.push(evidence);
        }
        Ok(kept)
    }

    /// Settle an attestation whose outcome is unknown, by its execution ID.
    ///
    /// # Errors
    /// Returns `Error::ExecutionUnknown` while the home's answer is still
    /// open, `Error::ResultExpired` once it can no longer be known, and the
    /// recorded refusal of a failed one.
    pub(crate) async fn attest_reconcile(&mut self, execution: &str) -> Result<Attestation> {
        let mut attestation = self.attestation(execution)?.ok_or(Error::NotFound)?;
        match attestation.stage {
            Stage::Complete => return Ok(attestation),
            Stage::Failed | Stage::Expired => {
                return Err(match attestation.error.as_deref() {
                    Some("EXPIRED") | None => Error::Expired,
                    Some(_) => Error::Forbidden,
                });
            }
            Stage::Lost => return Err(Error::ResultExpired),
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
                // Past the deadline with the sequence used: it may have
                // been recorded, and the home no longer says.
                if state.security.certified_at >= approval.expires_at
                    && device.is_some_and(|d| d.next_sequence > approval.sequence)
                {
                    attestation.stage = Stage::Lost;
                    attestation.error = Some("RESULT_EXPIRED".into());
                    self.save_attestation(&attestation)?;
                    return Err(Error::ResultExpired);
                }
                Err(Error::ExecutionUnknown)
            }
            Err(error) => Err(error),
        }
    }

    /// This account's formal-execution allowance for the current month,
    /// refreshed from its plan, and the months archived in the workspace.
    /// The home keeps an account's ledgers for this month and the last, and
    /// the first refresh of a month drops older ones, so finished months it
    /// still holds are archived first.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a login.
    pub async fn execution_usage(&mut self) -> Result<Json> {
        let account = self.account()?.clone();
        let ic = self.session()?.ic.clone();
        let keys = self.keys()?;
        let now = now_ms();
        let earlier = |month: u32| {
            if month % 100 == 1 {
                month - 89
            } else {
                month - 1
            }
        };
        let last = earlier(dmsg_protocol::billing::month_utc(now)?);
        for month in [last, earlier(last)] {
            let found: Result<ExecutionUsage> = ic
                .query(account.home, "get_execution_usage", (account.id, month))
                .await?;
            let usage = match found {
                Err(Error::NotFound) => continue,
                found => found?,
            };
            ensure(
                usage.account_id == account.id && usage.month_utc == month,
                Error::IntegrityFailed,
            )?;
            self.workspace
                .put_private(keys, &format!("{USAGE}{month}"), &usage)?;
        }
        // An update reply is certified by the subnet; there is no separate proof.
        let usage: Result<ExecutionUsage> = ic
            .update(account.home, "refresh_execution_entitlement", (account.id,))
            .await?;
        let usage = usage?;
        ensure(
            usage.account_id == account.id && usage.valid_until_ms > now,
            Error::IntegrityFailed,
        )?;
        self.workspace
            .put_private(keys, USAGE_NOW, &(now, &usage))?;
        let mut history = self
            .workspace
            .store
            .sealed_keys(USAGE)?
            .iter()
            .map(|key| {
                self.workspace
                    .private::<ExecutionUsage>(self.keys()?, key)?
                    .ok_or(Error::IntegrityFailed)
            })
            .collect::<Result<Vec<_>>>()?;
        history.sort_by_key(|u| std::cmp::Reverse(u.month_utc));
        let view = |u: &ExecutionUsage| {
            json!({
                "month": u.month_utc,
                "allowed": u.allowed_units,
                "charged": u.charged_units,
                // A refund or downgrade may leave it below what was charged.
                "remaining": u.allowed_units.saturating_sub(u.charged_units),
                "valid_until_ms": u.valid_until_ms,
            })
        };
        let mut current = view(&usage);
        current["history"] = history.iter().map(view).collect();
        Ok(current)
    }

    /// This device's attestations and those the account keeps from its
    /// other devices, newest first.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn attest_history(&self) -> Result<Vec<Json>> {
        let keys = self.keys()?;
        let own = self.workspace.meta.device_id;
        let entry = |attestation: &Attestation| -> Result<Json> {
            let statement = match &attestation.request {
                Signed::Document(r) => r.statement.clone(),
                Signed::Action(r) => app_action_statement(r)?,
            };
            let device = attestation.request.approval().device_id;
            Ok(json!({
                "execution": attestation.execution_id(),
                "approval": attestation.approval,
                "stage": attestation.stage,
                "origin": attestation.request.origin(),
                "created_at": attestation.created_at,
                "error": attestation.error,
                "device_id": hex::encode(*device),
                "this_device": device == own,
                "statement": statement_view(&statement),
            }))
        };
        let mut items = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for key in self.workspace.store.sealed_keys(PREFIX)? {
            let Some(attestation) = self.workspace.private::<Attestation>(keys, &key)? else {
                continue;
            };
            seen.insert(attestation.execution_id());
            items.push(entry(&attestation)?);
        }
        for evidence in self.kept_attestations()? {
            if seen.insert(evidence.attestation.execution_id()) {
                items.push(entry(&evidence.attestation)?);
            }
        }
        items.sort_by_key(|i| std::cmp::Reverse(i["created_at"].as_u64()));
        Ok(items)
    }

    /// The bundle of a finished attestation, this device's or one the
    /// account keeps.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown or unfinished one.
    pub fn attest_bundle(&self, execution: &str) -> Result<Bundle> {
        let (attestation, home) = match self.attestation(execution)? {
            Some(attestation) => (attestation, self.account()?.home.to_text()),
            None => {
                let evidence = self
                    .kept_attestations()?
                    .into_iter()
                    .find(|e| e.attestation.execution_id() == execution)
                    .ok_or(Error::NotFound)?;
                (evidence.attestation, evidence.home)
            }
        };
        let (Some(artifact), Some(receipt)) = (&attestation.artifact, &attestation.receipt) else {
            return Err(Error::NotFound);
        };
        Ok(Bundle {
            format: BUNDLE_FORMAT.into(),
            account: attestation.request.account().to_string(),
            home,
            request_id: attestation.execution_id(),
            cose_sign1: b64(&artifact.cose_sign1),
            cose_key: b64(&artifact.cose_key),
            receipt: b64(&canonical(receipt)),
        })
    }

    /// [`verify_online`] with this client's reader and the deployment's
    /// user homes.
    ///
    /// # Errors
    /// As [`verify_bundle`].
    pub async fn attest_verify(
        &mut self,
        bundle: &Bundle,
        file: Option<&Path>,
    ) -> Result<Verification> {
        let homes = self.homes().await?;
        let ic = self.reader().await?;
        verify_online(&ic, &homes, bundle, file).await
    }
}

/// [`verify_bundle`] under the agent's root key and the user `homes`, then
/// the approving device's present status from the account's certified
/// security, when the evidence names the device and it can be read.
///
/// # Errors
/// As [`verify_bundle`].
pub async fn verify_online(
    ic: &crate::ic::Ic,
    homes: &[Principal],
    bundle: &Bundle,
    file: Option<&Path>,
) -> Result<Verification> {
    let mut report = verify_bundle(bundle, file, &ic.root_key(), homes)?;
    let account: AccountId = report
        .account
        .parse()
        .map_err(|_| dmsg_types::invalid("account"))?;
    let home = Principal::from_text(&bundle.home).map_err(|_| dmsg_types::invalid("home"))?;
    let Some(device) = report
        .device_id
        .as_ref()
        .and_then(|id| hex::decode(id).ok())
        .and_then(|b| b.try_into().ok())
        .map(Hash::new)
    else {
        return Ok(report);
    };
    // Offline, the present status stays unknown; the evidence stands alone.
    let batch: Result<Result<CertifiedBatch>> = ic
        .query(home, "security_snapshot_batch", (vec![account],))
        .await;
    type Bundled = (
        dmsg_types::user::SecuritySnapshot,
        std::collections::BTreeMap<Hash, dmsg_types::user::Device>,
    );
    let devices: Result<Result<Bundled>> = ic.query(home, "get_device_bundle", (account,)).await;
    if let (Ok(Ok(batch)), Ok(Ok((_, devices)))) = (batch, devices)
        && let Ok(security) = crate::account::verify_security(
            home,
            &account,
            &batch,
            devices,
            &ic.root_key(),
            now_ms(),
        )
    {
        report.current = match security.device(&device) {
            Some(d) if d.revoked_at.is_none() => "active",
            Some(_) => "revoked",
            None => "unknown",
        }
        .into();
    }
    Ok(report)
}

/// Whether this device can sign now, and for request `id` its purpose:
/// a login, `FormalApprove` on this device, a policy that is not frozen and
/// allows the purpose, and the month's allowance as last read. `blockers`
/// names what stops it (`AUTH_REQUIRED`, `FORMAL_APPROVE`, `FROZEN`,
/// `PURPOSE`); an allowance used up is a `warnings` entry (`QUOTA`), since
/// the plan may have changed since it was read. The user home is waited on
/// without the client.
///
/// # Errors
/// Returns `Error::Locked` while locked and `Error::NotFound` for an
/// unknown request.
pub async fn attest_readiness<H: Hold>(h: &H, id: Option<&str>) -> Result<Json> {
    let (account, purpose, ic, device_id) = h
        .with(|c| {
            c.keys()?;
            let purpose = match id {
                None => None,
                Some(id) => match c.approval_record(id)?.action {
                    Action::Attest { statement, .. } => Some(statement_purpose(&statement)),
                    Action::AttestAction { .. } => Some(dmsg_types::KeyPurpose::AppAction),
                    Action::Connect { operation, .. } => c.connect_purpose(&operation)?,
                    _ => None,
                },
            };
            let ic = c.session().ok().map(|s| s.ic.clone());
            Ok((
                c.account()?.clone(),
                purpose,
                ic,
                c.workspace.meta.device_id,
            ))
        })
        .await?;
    let formal = |capabilities: &[Capability]| capabilities.contains(&Capability::FormalApprove);
    // An answer the home could not give, or an older one than this device
    // has seen, leaves both unknown.
    let (formal_approve, policy) = match &ic {
        Some(ic) => {
            let state = match crate::account::read_account(ic, account.home, &account.id).await {
                Ok(state) => {
                    h.with(|c| c.observe(&account.id, &state.security).map(|()| state))
                        .await
                }
                Err(error) => Err(error),
            };
            match state {
                Ok(state) => (
                    state
                        .active(&device_id)
                        .ok()
                        .map(|d| formal(&d.input.capabilities)),
                    Some(state.info.sensitive_policy.clone()),
                ),
                Err(_) => (None, None),
            }
        }
        None => {
            let reader = h.reader().await?;
            let security =
                match crate::account::fetch_security(&reader, account.home, &account.id).await {
                    Ok(security) => {
                        h.with(|c| c.observe(&account.id, &security).map(|()| security))
                            .await
                    }
                    Err(error) => Err(error),
                };
            match security {
                Ok(security) => (
                    security
                        .active(&device_id)
                        .ok()
                        .map(|d| formal(&d.input.capabilities)),
                    None,
                ),
                Err(_) => (None, None),
            }
        }
    };
    let usage = h
        .with(|c| {
            let month = dmsg_protocol::billing::month_utc(now_ms())?;
            Ok(c.workspace
                .private::<(u64, ExecutionUsage)>(c.keys()?, USAGE_NOW)?
                .filter(|(_, u)| u.month_utc == month && u.account_id == account.id))
        })
        .await?;
    let login = ic.is_some();
    let mut blockers = Vec::new();
    if !login {
        blockers.push("AUTH_REQUIRED");
    }
    if formal_approve == Some(false) {
        blockers.push("FORMAL_APPROVE");
    }
    if let Some(policy) = &policy {
        if policy.frozen {
            blockers.push("FROZEN");
        }
        if purpose
            .as_ref()
            .is_some_and(|p| !policy.allowed_purposes.contains(p))
        {
            blockers.push("PURPOSE");
        }
    }
    let warnings: Vec<&str> = usage
        .as_ref()
        .filter(|(_, u)| u.charged_units >= u.allowed_units)
        .map(|_| "QUOTA")
        .into_iter()
        .collect();
    Ok(json!({
        "ready": blockers.is_empty(),
        "blockers": blockers,
        "warnings": warnings,
        "login": login,
        "formal_approve": formal_approve,
        "purpose": purpose.map(|p| format!("{p:?}")),
        "policy": policy.map(|p| json!({
            "frozen": p.frozen,
            "allowed_purposes": p.allowed_purposes.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
            "daily_executions": p.daily_executions,
        })),
        "usage": usage.map(|(at, u)| json!({
            "allowed": u.allowed_units,
            "charged": u.charged_units,
            "remaining": u.allowed_units.saturating_sub(u.charged_units),
            "read_at": at,
        })),
    }))
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
