//! The approval center (plan §6.6): every operation this device signs on its
//! owner's behalf waits here until a person decides, in the desktop approval
//! window or an interactive terminal. Requests come from the desktop, the
//! CLI, agents or web applications (dmsg-connect/1); none of them can
//! answer one.
//!
//! A request's content is sealed under the LDK; only its ID, state, source
//! class and times stay in plain text, so a locked device can count what
//! waits without learning what it is. Answering checks the digest the person
//! was shown, the deadline and the state, then executes once: a lost reply
//! is reconciled by its execution ID, never signed again.
//!
//! Requests that move money, give a name away, add or revoke a device, change
//! a device's capabilities or change who controls the account are high-risk
//! ([`high_risk`]): clients hold to
//! approve them and, where this device can ask, the person confirms with
//! Touch ID or the login password first (plan S03). A device with a
//! registered approval key answers through the key instead, which also
//! signs the approvals the account protects ([`crate::approval_key`]), so
//! the person is asked once.

use crate::client::Client;
use crate::hold::{Hold, Unlocked};
use crate::now_ms;
use crate::secure::{ApprovalKeys, Authorized, PresenceKey};
use crate::store::ApprovalRow;
use dmsg_protocol::digest;
use dmsg_types::{
    DAY, Error, Hash, MINUTE, P256Signature, Result, Statement, StatementContent,
    app_action::AppAction,
    ensure,
    user::{AccountCommand, Capability, HardwarePurpose},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::sync::Arc;

/// How long a request from this device's own clients waits.
pub const APPROVAL_TTL_MS: u64 = 10 * MINUTE;
/// How long a high-risk request, such as an account change, waits.
pub const HIGH_RISK_TTL_MS: u64 = 5 * MINUTE;
/// How long an agent's request for capabilities waits: its owner may be
/// away for a while, but each agent's waiting requests hold part of the
/// queue the owner's own requests need ([`MAX_PER_SOURCE`]).
pub const AGENT_REQUEST_TTL_MS: u64 = 60 * MINUTE;
/// Most requests waiting for a decision at once.
pub const MAX_PENDING: usize = 20;
/// Most requests one source other than the owner's own clients (an agent,
/// a client or a web origin) has waiting at once, so none fills the queue.
pub const MAX_PER_SOURCE: usize = 5;
/// Finished requests, and the web operations behind them, are kept this
/// long after they were made.
pub const RETAIN_MS: u64 = 30 * DAY;

const PREFIX: &str = "approval:";

/// Where a request is in its life.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Waiting for a person.
    AwaitingUser,
    /// The person refused, or the device refused it before showing it.
    Rejected,
    /// Its deadline passed undecided.
    Expired,
    /// Its source withdrew it.
    Cancelled,
    /// Approved and running.
    Executing,
    /// Approved; the outcome is not known yet and is being reconciled.
    Unknown,
    /// Executed.
    Done,
    /// Approved, and the execution definitely failed.
    Failed,
    /// Another device of the account took it.
    Elsewhere,
    /// Approved and perhaps executed, but its outcome can no longer be
    /// read: the home keeps a result only a day past its approval.
    Lost,
}

impl State {
    /// The name the index and clients use.
    pub fn name(self) -> &'static str {
        match self {
            State::AwaitingUser => "awaiting_user",
            State::Rejected => "rejected",
            State::Expired => "expired",
            State::Cancelled => "cancelled",
            State::Executing => "executing",
            State::Unknown => "unknown",
            State::Done => "done",
            State::Failed => "failed",
            State::Elsewhere => "elsewhere",
            State::Lost => "lost",
        }
    }

    /// No further change.
    pub fn terminal(self) -> bool {
        !matches!(
            self,
            State::AwaitingUser | State::Executing | State::Unknown
        )
    }

    /// The names of the states with no further change.
    const TERMINAL: [&'static str; 7] = [
        "rejected",
        "expired",
        "cancelled",
        "done",
        "failed",
        "elsewhere",
        "lost",
    ];
}

/// The file a statement's digest names, as it was when hashed: a local
/// file, hashed again on approval, or a version in the file library,
/// checked against the library.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AttestFile {
    /// A local file's absolute path.
    pub path: Option<String>,
    /// A library file and its version, both IDs.
    pub library: Option<(String, String)>,
    /// Its name, as shown.
    pub name: String,
    /// Its size in bytes.
    pub size: u64,
    /// A local file's modification time when hashed, Unix milliseconds.
    pub modified: Option<u64>,
}

/// A file a person compared with a request's digest before deciding.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FileCheck {
    /// Its name.
    pub name: String,
    /// Its size in bytes.
    pub size: u64,
    /// Whether its SHA-256 is the statement's.
    pub matches: bool,
    /// Unix milliseconds.
    pub checked_at: u64,
}

/// A local file's size and modification time, which tell whether it
/// changed since it was hashed.
pub fn file_stamp(path: &std::path::Path) -> Result<(u64, Option<u64>)> {
    let meta = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::NotFound,
        _ => Error::Unavailable(format!("file: {e}")),
    })?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    Ok((meta.len(), modified))
}

/// A path's last component, as shown.
pub fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What a request asks the device to do.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Import exact explicitly scanned files, without deleting or editing them.
    SecretDiscoveryImport {
        /// Selected source identities and full-file digests.
        plan: Box<crate::secrets::discovery::ImportPlan>,
    },
    /// Import a validated received snapshot as new Vault/Files objects.
    SecretPackageImport {
        /// Exact package digest and explicit selected fields/attachments.
        selection: Box<crate::secrets::receiving::Selection>,
    },
    /// Publish one explicitly selected secret snapshot.
    SecretShare {
        /// Device-local immutable preparation.
        id: String,
        /// Commitment to every selected source and publication limit.
        digest: String,
    },
    /// Configure a read-only, device-local virtual mapping.
    SecretMountConfig {
        /// OS-validated paths and fixed authorization mode.
        config: Box<crate::secrets::mounts::Config>,
    },
    /// Authorize one handle or one fixed file window.
    SecretMountRead {
        /// No source values or decryption keys.
        plan: Box<crate::secrets::mounts::ReadPlan>,
    },
    /// Enable an owner-selected SSH key.
    SecretSshBinding {
        /// Local key and allowed purposes.
        binding: Box<crate::secrets::ssh::KeyBinding>,
    },
    /// Sign once for the original socket connection.
    SecretSsh {
        /// Exact validated wire message commitment.
        plan: Box<crate::secrets::ssh::SignPlan>,
    },
    /// One-time delivery to an initiating owner CLI.
    SecretProcess {
        /// Pinned program, sources, and claim commitment.
        plan: Box<crate::secrets::process::RunPlan>,
    },
    /// A device-local policy configuration, approved independently of its uses.
    SecretPolicy {
        /// Configuration the owner reviews.
        change: crate::secrets::PolicyChange,
    },
    /// One immutable use plan; never replayed by reconciliation.
    SecretUse {
        /// Immutable operation and its source/version bindings.
        plan: Box<crate::secrets::SecretUsePlan>,
    },
    /// Attest a statement with this device; `origin` is the checked
    /// application origin the execution receipt records.
    Attest {
        /// The statement, issuer included.
        statement: Statement,
        /// `https://dmsg.net` for this device's own clients.
        origin: String,
        /// The file a file statement or digest describes, checked again on
        /// approval.
        file: Option<AttestFile>,
    },
    /// Attest an application action its product prepared. It is kept as
    /// its canonical CBOR: a candid Principal inside this internally
    /// tagged enum would not decode again.
    AttestAction {
        /// The action's canonical CBOR.
        action: serde_bytes::ByteBuf,
        /// The canonical CBOR of the certified registration's schema, which
        /// renders the command for review.
        schema: Option<serde_bytes::ByteBuf>,
    },
    /// Replace a device's capabilities.
    DeviceCapabilities {
        /// The device.
        device_id: Hash,
        /// The complete new set.
        capabilities: Vec<Capability>,
    },
    /// Revoke a device, then rotate the content root without it.
    DeviceRevoke {
        /// The device.
        device_id: Hash,
    },
    /// Enroll the device of a pairing request, then rotate the content
    /// root to include it.
    DevicePair {
        /// The pairing token as the new device produced it, checked again
        /// on approval.
        token: String,
        /// The device it enrolls, as shown.
        device: dmsg_types::user::DeviceInput,
        /// A succession token: the device succeeds this one, which then
        /// approves only the handover.
        #[serde(default)]
        succession: bool,
    },
    /// A web request (dmsg-connect/1).
    Connect {
        /// Key of the operation record.
        operation: String,
        /// The operation's commitment.
        digest: Hash,
        /// A checkout's quoted terms: the journal ID and its commitment.
        /// The person quotes before approving, so the exact amount is shown.
        checkout: Option<(String, Hash)>,
    },
    /// Approve a quoted checkout for the payer and pay it; with `fresh`,
    /// advance a qualified PANDA claim.
    Checkout {
        /// Order or claim ID.
        id: String,
        /// The terms' commitment.
        digest: Hash,
        /// Advance the PANDA claim instead of opening it.
        fresh: bool,
    },
    /// Pay for a quoted letter from the payer.
    Letter {
        /// The recipient.
        recipient: String,
        /// Order ID.
        order: String,
        /// The quote's commitment.
        digest: Hash,
        /// What the payer pays, fees and refundable reserve included.
        amount: String,
        /// The ledger.
        ledger: String,
    },
    /// Authorize and run a name operation.
    Handle {
        /// Operation ID.
        id: String,
        /// The intent's commitment.
        digest: Hash,
    },
    /// Change the account's security: bind or remove a login, the
    /// sensitive-execution policy, the recovery delay, or dispute a
    /// pending recovery.
    Account {
        /// The change.
        change: crate::security::AccountChange,
    },
    /// Change the account's Agent Delegation principal.
    Principal {
        /// The change.
        change: crate::agent::PrincipalChange,
    },
    /// Grant one of this device's Agent IDs the capabilities it asked for.
    Grant {
        /// The Agent ID, `did:agent:…`.
        grantee: String,
        /// What it asked for.
        capabilities: Vec<String>,
        /// Lifetime in days, or until revoked.
        days: Option<u64>,
    },
    /// Sign a grant or revocation with a controller key and submit it.
    Delegation {
        /// Journal ID.
        job: String,
        /// The event's commitment.
        digest: Hash,
    },
    /// Create or join a team wallet, decide on its proposal, or cosign
    /// one of its transactions.
    Wallet {
        /// Journal ID.
        job: String,
        /// The job's commitment.
        digest: Hash,
    },
    /// Register a device's new approval key.
    ApprovalKey {
        /// The device the key belongs to.
        device_id: Hash,
        /// SEC1 compressed P-256 public key.
        public_key: serde_bytes::ByteBuf,
        /// Another device's proof of possession, as its request carried
        /// it; this device's own key proves itself on approval.
        proof: Option<P256Signature>,
    },
    /// Retire a device's approval key.
    ApprovalKeyRetire {
        /// The device.
        device_id: Hash,
    },
    /// Require approval keys for these purposes, or no longer (empty).
    HardwarePolicy {
        /// The new purposes.
        purposes: Vec<HardwarePurpose>,
        /// The purposes they replace, as read when asked.
        previous: Vec<HardwarePurpose>,
        /// The active devices without a registered key when asked, which
        /// the requirement shuts out.
        without_keys: Vec<Hash>,
    },
}

impl Action {
    fn class(&self) -> &'static str {
        match self {
            Action::SecretDiscoveryImport { .. }
            | Action::SecretPackageImport { .. }
            | Action::SecretShare { .. }
            | Action::SecretMountConfig { .. }
            | Action::SecretMountRead { .. }
            | Action::SecretSsh { .. }
            | Action::SecretSshBinding { .. }
            | Action::SecretProcess { .. }
            | Action::SecretPolicy { .. }
            | Action::SecretUse { .. } => "secret",
            Action::Attest { .. } | Action::AttestAction { .. } => "attest",
            Action::DeviceCapabilities { .. }
            | Action::DeviceRevoke { .. }
            | Action::DevicePair { .. }
            | Action::Account { .. }
            | Action::Principal { .. }
            | Action::ApprovalKey { .. }
            | Action::ApprovalKeyRetire { .. }
            | Action::HardwarePolicy { .. } => "account",
            Action::Delegation { .. } | Action::Grant { .. } => "agent",
            Action::Wallet { .. } => "wallet",
            Action::Connect { .. } => "connect",
            Action::Checkout { .. } | Action::Letter { .. } | Action::Handle { .. } => "payment",
        }
    }
}

/// A request in the approval center, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// Random request ID, hex.
    pub id: String,
    /// `gui`, `cli`, `agent:<did>`, `client:<id>` or `connect:<origin>`.
    pub source: String,
    /// The operation.
    pub action: Action,
    /// Where it is.
    pub state: State,
    /// Unix milliseconds.
    pub created_at: u64,
    /// The decision deadline.
    pub expires_at: u64,
    /// Last change.
    pub updated_at: u64,
    /// Execution ID (attestations) once approved, hex.
    pub execution: Option<String>,
    /// Error code of a refusal or failure.
    pub error: Option<String>,
    /// Past the point where it may have executed: a later failure is final.
    pub started: bool,
    /// When a person decided it, Unix milliseconds.
    pub decided_at: Option<u64>,
    /// Where the person decided: `gui` or `cli`.
    pub decided_via: Option<String>,
    /// A file the person compared with its digest; not part of what is
    /// approved.
    pub file_check: Option<FileCheck>,
    /// Filtered secret-use result, sealed with this request.
    #[serde(default)]
    pub secret_result: Option<crate::secrets::github::IssueResult>,
    /// Manual review of a lost result, without changing its outcome.
    #[serde(default)]
    pub reviewed_at: Option<u64>,
    /// Optional CLI observation; never changes delivery into a replayable action.
    #[serde(default)]
    pub delivery_report: Option<Json>,
}

impl Request {
    /// The commitment a person approves: the request as shown.
    pub fn digest(&self) -> Hash {
        digest("dmsg/approval/v1", &(&self.id, &self.source, &self.action))
    }
}

/// What a decision leaves to run.
pub enum Decision {
    /// Decided, and executed when approved: the request's view.
    Done(Json),
    /// An approved attestation, to run in steps.
    Attest(Box<crate::attest::AttestRun>),
    /// An approved wallet job, whose network waits release the client.
    Wallet(Box<WalletRun>),
    /// One non-idempotent use, executed outside the client lock.
    SecretUse(Box<crate::secrets::SecretRun>),
    /// Snapshot encryption/upload/publication, executed outside the client lock.
    SecretShare(Box<crate::secrets::shares::ShareRun>),
    /// Received content is staged outside the client lock, then committed once.
    SecretPackageImport(Box<crate::secrets::receiving::ImportRun>),
    /// Source filesystem work delegated to the host, not performed under Client.
    SecretDiscoveryImport(Box<crate::secrets::discovery::ImportWork>),
}

/// How long an attestation running in steps keeps the reconciler away;
/// one left behind longer, such as by a dropped connection, is
/// reconciled as any other.
const RUN_MS: u64 = 5 * MINUTE;

/// Whether approving moves money, gives a name away, adds or revokes a
/// device, changes what a device may do or changes who controls the account
/// or its Agent Delegation principal, from
/// the request and its `detail` view. Such a request is held to approve, and
/// confirmed with the person present where the device can ask.
pub fn high_risk(request: &Request, detail: &Json) -> bool {
    match &request.action {
        Action::SecretDiscoveryImport { .. }
        | Action::SecretPackageImport { .. }
        | Action::SecretShare { .. }
        | Action::SecretMountConfig { .. }
        | Action::SecretMountRead { .. }
        | Action::SecretSsh { .. }
        | Action::SecretSshBinding { .. }
        | Action::SecretProcess { .. }
        | Action::SecretPolicy { .. }
        | Action::SecretUse { .. } => true,
        Action::Checkout { .. }
        | Action::Letter { .. }
        | Action::DeviceRevoke { .. }
        | Action::DevicePair { .. }
        | Action::DeviceCapabilities { .. }
        | Action::ApprovalKey { .. }
        | Action::ApprovalKeyRetire { .. }
        | Action::HardwarePolicy { .. } => true,
        Action::Principal { .. } => true,
        Action::Grant { capabilities, .. } => {
            capabilities.iter().any(|c| crate::agents::sensitive(c))
        }
        // A name is worth what it cost: paying for one or giving one away.
        Action::Handle { .. } => !detail["total"].is_null() || detail["action"] == "Transfer",
        Action::Connect { .. } => detail["method"] == "checkout",
        Action::Account { change } => !matches!(
            change,
            crate::security::AccountChange::DisputeRecovery { .. }
        ),
        Action::Wallet { .. } => {
            let kind = detail["proposal"]["intent"]["kind"].as_str().unwrap_or("");
            matches!(detail["action"].as_str(), Some("create" | "cosign"))
                || (detail["action"] == "decide"
                    && detail["decision"] == "approve"
                    && ["transfer", "batch", "canister_call", "refuel"].contains(&kind))
        }
        Action::Attest { .. } | Action::AttestAction { .. } | Action::Delegation { .. } => false,
    }
}

/// The purpose an account's hardware policy may protect a request under,
/// from the request and its `detail` view.
pub fn request_purpose(request: &Request, detail: &Json) -> Option<HardwarePurpose> {
    use crate::agent::PrincipalChange;
    use crate::security::AccountChange;
    match &request.action {
        Action::DeviceCapabilities { .. }
        | Action::DeviceRevoke { .. }
        | Action::DevicePair { .. } => Some(HardwarePurpose::DeviceControl),
        Action::ApprovalKey { .. }
        | Action::ApprovalKeyRetire { .. }
        | Action::HardwarePolicy { .. } => Some(HardwarePurpose::AccountControl),
        Action::Account { change } => (!matches!(change, AccountChange::DisputeRecovery { .. }))
            .then_some(HardwarePurpose::AccountControl),
        Action::Principal { change } => matches!(
            change,
            PrincipalChange::Enable { .. }
                | PrincipalChange::Register { .. }
                | PrincipalChange::Retire { .. }
        )
        .then_some(HardwarePurpose::PrincipalControl),
        Action::Checkout { .. } | Action::Handle { .. } => Some(HardwarePurpose::Payment),
        Action::Attest { .. } | Action::AttestAction { .. } => Some(HardwarePurpose::Attestation),
        Action::Connect { .. } => match detail["method"].as_str() {
            Some("checkout") => Some(HardwarePurpose::Payment),
            Some("authenticate" | "signDocument" | "signAction") => {
                Some(HardwarePurpose::Attestation)
            }
            _ => None,
        },
        _ => None,
    }
}

/// How the person approves a request on this device.
pub enum Ask {
    /// Confirm with the presence hardware.
    Presence(Arc<dyn PresenceKey>),
    /// Answer through this device's approval key, whose answer also signs:
    /// the hardware and whether the key takes only fingerprints.
    Hardware(Arc<dyn ApprovalKeys>, bool),
}

/// Check a source string.
///
/// # Errors
/// Returns `Error::InvalidInput` for anything else.
pub fn validate_source(source: &str) -> Result<()> {
    let ok = match source.split_once(':') {
        None => matches!(source, "gui" | "cli"),
        Some(("agent", rest)) => rest.starts_with("did:agent:") && rest.len() <= 200,
        Some(("client", rest)) => {
            !rest.is_empty()
                && rest.len() <= 64
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        }
        Some(("mount", id)) => crate::secrets::hex_id(id, 64),
        Some(("connect", origin)) => origin.starts_with("http"),
        _ => false,
    };
    ensure(
        ok && !source.chars().any(char::is_control),
        dmsg_types::invalid("source"),
    )
}

impl Client {
    fn approval_key(id: &str) -> String {
        format!("{PREFIX}{id}")
    }

    /// One request, as stored.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` for an unknown ID.
    pub(crate) fn approval_record(&self, id: &str) -> Result<Request> {
        self.workspace
            .private(self.keys()?, &Self::approval_key(id))?
            .ok_or(Error::NotFound)
    }

    /// Store a request and its index row together.
    pub(crate) fn save_approval(&self, request: &Request) -> Result<()> {
        self.workspace
            .store
            .atomic(|_| self.write_approval(request))
    }

    /// Write inside the caller's existing workspace transaction.
    pub(crate) fn write_approval(&self, request: &Request) -> Result<()> {
        self.workspace.store.put_approval(&ApprovalRow {
            id: request.id.clone(),
            state: request.state.name().into(),
            class: request.action.class().into(),
            created_at: request.created_at,
            expires_at: request.expires_at,
        })?;
        self.workspace
            .put_private(self.keys()?, &Self::approval_key(&request.id), request)?;
        self.project_approval_notification(request)
    }

    pub(crate) fn project_approval_notification(&self, request: &Request) -> Result<()> {
        use crate::notifications::{self, Item, Target};
        let Some(account) = &self.workspace.meta.account else {
            return Ok(());
        };
        let mut item = Item::new(
            account.id.to_string(),
            Target::Approval(request.id.clone()),
            request.updated_at.max(1),
            request.updated_at,
        );
        item.first_at = request.created_at;
        item.group = hex::encode(*digest(
            "dmsg/notification/source/v1",
            &(&item.account, &request.source),
        ));
        item.open = request.state == State::AwaitingUser;
        item.actionable = item.open;
        item.deadline = Some(request.expires_at);
        notifications::upsert(&self.workspace.store, item)?;
        Ok(())
    }

    /// Add a request from `source`, waiting until `expires_at`.
    ///
    /// # Errors
    /// Returns `Error::QuotaExceeded` beyond [`MAX_PENDING`] waiting requests,
    /// or [`MAX_PER_SOURCE`] from one agent, client or web origin.
    pub(crate) fn approval_create(
        &mut self,
        source: &str,
        action: Action,
        expires_at: u64,
    ) -> Result<Request> {
        self.approval_create_at(source, action, now_ms(), expires_at)
    }

    /// [`Self::approval_create`] with the execution's time already read, so a
    /// derived deadline and `created_at` use the same instant.
    ///
    /// # Errors
    /// As [`Self::approval_create`].
    pub(crate) fn approval_create_at(
        &mut self,
        source: &str,
        action: Action,
        now: u64,
        expires_at: u64,
    ) -> Result<Request> {
        validate_source(source)?;
        self.expire_approvals(now)?;
        let waiting: Vec<Request> = self
            .workspace
            .store
            .approvals(&[State::AwaitingUser.name()])?
            .iter()
            .map(|row| self.approval_record(&row.id))
            .collect::<Result<_>>()?;
        // The owner's own clients share the whole queue; any other source
        // keeps room for them.
        ensure(
            waiting.len() < MAX_PENDING
                && (!matches!(action, Action::SecretMountRead { .. })
                    || waiting
                        .iter()
                        .filter(|r| matches!(r.action, Action::SecretMountRead { .. }))
                        .count()
                        < MAX_PER_SOURCE)
                && (matches!(source, "gui" | "cli")
                    || waiting.iter().filter(|r| r.source == source).count() < MAX_PER_SOURCE),
            Error::QuotaExceeded,
        )?;
        let request = Request {
            id: hex::encode(crate::crypto::random_id()),
            source: source.into(),
            action,
            state: State::AwaitingUser,
            created_at: now,
            expires_at,
            updated_at: now,
            execution: None,
            error: None,
            started: false,
            decided_at: None,
            decided_via: None,
            file_check: None,
            secret_result: None,
            reviewed_at: None,
            delivery_report: None,
        };
        self.save_approval(&request)?;
        Ok(request)
    }

    /// Mark undecided requests past their deadline expired, and forget
    /// finished ones past [`RETAIN_MS`].
    pub(crate) fn expire_approvals(&mut self, now: u64) -> Result<()> {
        for id in self
            .workspace
            .store
            .prune_approvals(&State::TERMINAL, now.saturating_sub(RETAIN_MS))?
        {
            self.workspace.delete_private(&Self::approval_key(&id))?;
        }
        for row in self
            .workspace
            .store
            .approvals(&[State::AwaitingUser.name()])?
        {
            if row.expires_at > now && row.class == "secret" {
                let mut request = self.approval_record(&row.id)?;
                if let Action::SecretMountRead { plan } = &request.action
                    && let Err(error) = self.secret_mount_check(plan)
                {
                    request.state = State::Failed;
                    request.error = Some(crate::error_code(&error));
                    request.updated_at = now;
                    self.save_approval(&request)?;
                }
                if let Action::SecretSsh { plan } = &request.action
                    && let Err(error) = self.secret_ssh_check(plan)
                {
                    request.state = State::Failed;
                    request.error = Some(crate::error_code(&error));
                    request.updated_at = now;
                    self.save_approval(&request)?;
                }
                if let Action::SecretProcess { plan } = &request.action
                    && let Err(error) = self.secret_process_check(plan)
                {
                    request.state = State::Failed;
                    request.error = Some(crate::error_code(&error));
                    request.updated_at = now;
                    self.save_approval(&request)?;
                }
                if let Action::SecretUse { plan } = &request.action
                    && let Err(error) = self.secret_plan_check(plan, now)
                {
                    request.state = State::Failed;
                    request.error = Some(crate::error_code(&error));
                    request.updated_at = now;
                    self.save_approval(&request)?;
                }
            }
            if row.expires_at <= now {
                let mut request = self.approval_record(&row.id)?;
                request.state = State::Expired;
                request.updated_at = now;
                self.save_approval(&request)?;
                if let Action::Connect { operation, .. } = &request.action {
                    self.connect_expire(operation)?;
                }
            }
        }
        Ok(())
    }

    /// Requests waiting for a decision, or every request with `all`.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn approvals(&mut self, all: bool) -> Result<Vec<Json>> {
        self.keys()?;
        self.expire_approvals(now_ms())?;
        let states: &[&str] = if all {
            &[]
        } else {
            &["awaiting_user", "executing", "unknown"]
        };
        self.workspace
            .store
            .approvals(states)?
            .iter()
            .map(|row| self.approval_view_with_files(&self.approval_record(&row.id)?, false))
            .collect()
    }

    /// The full ID of the one request that starts with `prefix`, at least
    /// six characters, from the index alone.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when none matches and `Error::InvalidInput`
    /// for a shorter prefix or one that matches several.
    pub fn approval_id(&self, prefix: &str) -> Result<String> {
        let ids = self.workspace.store.approval_ids(prefix)?;
        let mut found = ids.into_iter();
        match (found.next(), found.next()) {
            (None, _) => Err(Error::NotFound),
            (Some(id), None) if prefix.len() >= 6 => Ok(id),
            _ => Err(Error::InvalidInput("give more of the request ID".into())),
        }
    }

    /// Approved requests whose outcome is not settled; readable while locked.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn approvals_unsettled(&self) -> Result<u64> {
        self.workspace
            .store
            .count_approvals(&[State::Executing.name(), State::Unknown.name()], 0)
    }

    /// Requests waiting for a decision; readable while locked.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn approvals_waiting(&self) -> Result<u64> {
        self.workspace
            .store
            .count_approvals(&[State::AwaitingUser.name()], now_ms())
    }

    /// One request with everything a person needs to decide.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown ID.
    pub fn approval(&mut self, id: &str) -> Result<Json> {
        self.expire_approvals(now_ms())?;
        self.approval_view(&self.approval_record(id)?)
    }

    /// Withdraw a request this device's own clients made, before a decision.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a web request and `Error::Expired`
    /// once it is decided.
    pub fn approval_cancel(&mut self, id: &str) -> Result<Json> {
        let now = now_ms();
        self.expire_approvals(now)?;
        let mut request = self.approval_record(id)?;
        ensure(
            !matches!(request.action, Action::Connect { .. }),
            Error::Forbidden,
        )?;
        ensure(
            request.state == State::AwaitingUser && now < request.expires_at,
            Error::Expired,
        )?;
        request.state = State::Cancelled;
        request.updated_at = now;
        self.save_approval(&request)?;
        self.approval_view(&request)
    }

    /// Clear only the interrupted runner's in-memory marker; its journal remains.
    pub fn abandon_approval_runtime(&mut self, id: &str) {
        self.running.remove(id);
    }

    /// How the person must approve `id` first, and the request's type for
    /// the question: through this device's approval key where it answers
    /// the request ([`Client::approval_answer`]), else with the presence
    /// hardware for a high-risk request on a device that can ask. `None`
    /// when no check is needed or possible.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` for an
    /// unknown ID.
    pub fn approval_presence(&mut self, id: &str) -> Result<Option<(Ask, String)>> {
        let request = self.approval_record(id)?;
        let view = self.approval_view(&request)?;
        let high = view["risk"] == "high";
        let what = view["type"].as_str().unwrap_or_default().to_string();
        let purpose = request_purpose(&request, &view["detail"]);
        // This device's new key proves itself with the answer, so it asks.
        let answer = match &request.action {
            Action::ApprovalKey {
                public_key,
                proof: None,
                ..
            } => self.new_key_answer(public_key)?,
            // Key maintenance must remain available with a lost key when
            // the policy is off; use ordinary presence confirmation then.
            Action::ApprovalKey { .. } | Action::ApprovalKeyRetire { .. } => {
                self.approval_answer(purpose, false)?
            }
            _ => self.approval_answer(purpose, high)?,
        };
        if let Some((keys, biometry_only)) = answer {
            return Ok(Some((Ask::Hardware(keys, biometry_only), what)));
        }
        let Some(key) = self.presence.clone().filter(|key| key.presence().is_some()) else {
            return Ok(None);
        };
        Ok(high.then(|| (Ask::Presence(key), what)))
    }

    /// Note on a waiting request that the person did not confirm approving
    /// it: `error` is the presence check's.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown ID.
    pub fn approval_unconfirmed(&mut self, id: &str, error: &Error) -> Result<()> {
        let mut request = self.approval_record(id)?;
        if request.state == State::AwaitingUser {
            request.error = Some(match error {
                Error::Forbidden => "PRESENCE_DECLINED".into(),
                _ => "PRESENCE_UNAVAILABLE".into(),
            });
            request.updated_at = now_ms();
            self.save_approval(&request)?;
        }
        Ok(())
    }

    /// Record a file the person compared with the digest of a waiting
    /// request: its name, size and SHA-256, hashed by the caller. Nothing
    /// that is approved changes.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for a request without a digest and
    /// `Error::Expired` once it is decided.
    pub fn approval_file_checked(
        &mut self,
        id: &str,
        name: &str,
        size: u64,
        sha256: Hash,
    ) -> Result<Json> {
        let now = now_ms();
        self.expire_approvals(now)?;
        let mut request = self.approval_record(id)?;
        ensure(
            request.state == State::AwaitingUser && now < request.expires_at,
            Error::Expired,
        )?;
        let digest = match &request.action {
            Action::Attest { statement, .. } => statement_digest(statement),
            Action::Connect { operation, .. } => self.connect_document_digest(operation)?,
            _ => None,
        };
        let expected = digest.ok_or_else(|| Error::InvalidInput("no digest".into()))?;
        request.file_check = Some(FileCheck {
            name: name.into(),
            size,
            matches: expected == sha256,
            checked_at: now,
        });
        request.updated_at = now;
        self.save_approval(&request)?;
        self.approval_view(&request)
    }

    /// A person's decision on `id`, after seeing the request whose digest
    /// is `shown`, in the desktop approval window or an interactive
    /// terminal (`via`). Approving executes the operation once. A
    /// high-risk request on a device that can check the person needs
    /// `confirmed` ([`Self::approval_presence`]).
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` when the request changed since it was
    /// shown, `Error::Expired` past its deadline or once decided,
    /// `Error::Forbidden` for an unconfirmed high-risk approval, and the
    /// execution's errors.
    pub async fn approval_respond(
        &mut self,
        id: &str,
        shown: &str,
        approve: bool,
        via: &str,
        confirmed: bool,
    ) -> Result<Json> {
        match self
            .approval_decide(id, shown, approve, via, confirmed)
            .await?
        {
            Decision::Done(view) => Ok(view),
            Decision::Attest(run) => run.run(&self.hold()).await,
            Decision::Wallet(run) => run.run(&self.hold()).await,
            Decision::SecretUse(run) => run.run(&self.hold()).await,
            Decision::SecretShare(run) => run.run(&self.hold()).await,
            Decision::SecretPackageImport(run) => run.run(&self.hold()).await,
            Decision::SecretDiscoveryImport(_) => Err(Error::UnsupportedProtocol),
        }
    }

    /// [`Self::approval_respond`] up to an approved attestation, which is
    /// returned to run in steps so that its network waits can happen
    /// outside a lock on the client ([`crate::attest::AttestRun`]); every
    /// other decision is executed here.
    ///
    /// # Errors
    /// As [`Self::approval_respond`].
    pub async fn approval_decide(
        &mut self,
        id: &str,
        shown: &str,
        approve: bool,
        via: &str,
        confirmed: bool,
    ) -> Result<Decision> {
        self.approval_decide_with(id, shown, approve, via, confirmed, None)
            .await
    }

    /// [`Self::approval_decide`] with the person's answer through this
    /// device's approval key ([`Ask::Hardware`]), which confirms the
    /// decision and signs for it only.
    ///
    /// # Errors
    /// As [`Self::approval_respond`].
    pub async fn approval_decide_with(
        &mut self,
        id: &str,
        shown: &str,
        approve: bool,
        via: &str,
        confirmed: bool,
        authorized: Option<Box<dyn Authorized>>,
    ) -> Result<Decision> {
        self.authorized = authorized;
        let decision = self.decide(id, shown, approve, via, confirmed).await;
        self.authorized = None;
        decision
    }

    async fn decide(
        &mut self,
        id: &str,
        shown: &str,
        approve: bool,
        via: &str,
        confirmed: bool,
    ) -> Result<Decision> {
        let now = now_ms();
        self.expire_approvals(now)?;
        let mut request = self.approval_record(id)?;
        ensure(
            hex::encode(*request.digest()) == shown,
            Error::IntegrityFailed,
        )?;
        ensure(
            request.state == State::AwaitingUser && now < request.expires_at,
            Error::Expired,
        )?;
        if approve && !confirmed && self.approval_presence(id)?.is_some() {
            request.error = Some("PRESENCE_REQUIRED".into());
            self.save_approval(&request)?;
            return Err(Error::Forbidden);
        }
        if approve
            && matches!(
                request.action,
                Action::SecretDiscoveryImport { .. }
                    | Action::SecretPackageImport { .. }
                    | Action::SecretShare { .. }
                    | Action::SecretUse { .. }
                    | Action::SecretProcess { .. }
                    | Action::SecretSsh { .. }
                    | Action::SecretMountRead { .. }
            )
        {
            ensure(
                via == "gui"
                    || (via == "cli"
                        && confirmed
                        && self.presence.as_ref().and_then(|p| p.presence()).is_some()),
                Error::Forbidden,
            )?;
        }
        request.decided_at = Some(now);
        request.decided_via = Some(via.into());
        if let Action::Connect {
            operation,
            checkout: None,
            ..
        } = &request.action
        {
            // A checkout is approved at the amount shown, never unquoted.
            ensure(
                !approve || self.connect_method(operation)? != "checkout",
                Error::InvalidInput("quote the checkout first".into()),
            )?;
        }
        if !approve {
            request.state = State::Rejected;
            request.updated_at = now;
            self.save_approval(&request)?;
            if let Action::Connect { operation, .. } = &request.action {
                self.connect_reject(operation).await?;
            }
            return self.approval_view(&request).map(Decision::Done);
        }
        request.state = State::Executing;
        request.updated_at = now;
        self.save_approval(&request)?;
        if matches!(
            request.action,
            Action::Attest { .. } | Action::AttestAction { .. }
        ) {
            return match self.attest_run(&request) {
                Ok(mut run) => {
                    run.authorized = self.authorized.take();
                    self.running.insert(request.id.clone(), now);
                    Ok(Decision::Attest(Box::new(run)))
                }
                Err(error) => self
                    .finish_approval(&mut request, Err(error))
                    .map(Decision::Done),
            };
        }
        if matches!(request.action, Action::SecretDiscoveryImport { .. }) {
            return match self.secret_discovery_work(&request) {
                Ok(work) => Ok(Decision::SecretDiscoveryImport(Box::new(work))),
                Err(error) => self
                    .secret_fail_before_send(&request.id, error)
                    .map(Decision::Done),
            };
        }
        if matches!(request.action, Action::SecretPackageImport { .. }) {
            return match self.secret_package_run(&mut request, now) {
                Ok(run) => Ok(Decision::SecretPackageImport(Box::new(run))),
                Err(error) => self
                    .secret_fail_before_send(&request.id, error)
                    .map(Decision::Done),
            };
        }
        if matches!(request.action, Action::SecretShare { .. }) {
            return match self.secret_share_run(&mut request, now) {
                Ok(run) => Ok(Decision::SecretShare(Box::new(run))),
                Err(error) => self
                    .secret_fail_before_send(&request.id, error)
                    .map(Decision::Done),
            };
        }
        if matches!(request.action, Action::SecretMountRead { .. }) {
            return match self.secret_mount_approve(&mut request, now) {
                Ok(view) => Ok(Decision::Done(view)),
                Err(error) => self
                    .secret_fail_before_send(&request.id, error)
                    .map(Decision::Done),
            };
        }
        if matches!(request.action, Action::SecretSsh { .. }) {
            return match self.secret_ssh_approve(&request) {
                Ok(view) => Ok(Decision::Done(view)),
                Err(error) => self
                    .secret_fail_before_send(&request.id, error)
                    .map(Decision::Done),
            };
        }
        if matches!(request.action, Action::SecretProcess { .. }) {
            return match self.secret_process_approve(&request) {
                Ok(view) => Ok(Decision::Done(view)),
                Err(error) => self
                    .secret_fail_before_send(&request.id, error)
                    .map(Decision::Done),
            };
        }
        if matches!(request.action, Action::SecretUse { .. }) {
            return match self.secret_run(&mut request, now) {
                Ok(run) => Ok(Decision::SecretUse(Box::new(run))),
                Err(error) => self
                    .secret_fail_before_send(&request.id, error)
                    .map(Decision::Done),
            };
        }
        if let Action::Wallet { job, digest } = &request.action {
            ensure(
                self.wallet_job_digest(job)? == *digest,
                Error::IntegrityFailed,
            )?;
            request.started = true;
            self.save_approval(&request)?;
            self.running.insert(request.id.clone(), now);
            return Ok(Decision::Wallet(Box::new(WalletRun { request })));
        }
        let outcome = self.execute_approval(&mut request).await;
        self.finish_approval(&mut request, outcome)
            .map(Decision::Done)
    }

    /// Execute an approved request other than an attestation, which runs
    /// in steps ([`crate::attest::AttestRun`]).
    async fn execute_approval(&mut self, request: &mut Request) -> Result<()> {
        match request.action.clone() {
            Action::SecretShare { .. }
            | Action::SecretPackageImport { .. }
            | Action::SecretDiscoveryImport { .. } => Err(Error::Forbidden),
            Action::SecretMountConfig { config } => self.secret_mount_apply(&config, &request.id),
            Action::SecretSshBinding { binding } => self.secret_ssh_apply(&binding, &request.id),
            Action::SecretPolicy { change } => self.secret_policy_apply(&change, &request.id),
            Action::SecretUse { .. }
            | Action::SecretProcess { .. }
            | Action::SecretSsh { .. }
            | Action::SecretMountRead { .. } => Err(Error::Forbidden),
            Action::Attest { .. } | Action::AttestAction { .. } => {
                Err(Error::InvalidInput("an attestation runs in steps".into()))
            }
            Action::DeviceCapabilities {
                device_id,
                capabilities,
            } => self
                .mutate(
                    AccountCommand::SetDeviceCapabilities {
                        device_id,
                        capabilities,
                    },
                    None,
                )
                .await
                .map(|_| ()),
            Action::DeviceRevoke { device_id } => {
                self.mutate(AccountCommand::RevokeDevice { device_id }, None)
                    .await?;
                // A new root no revoked device can open (AT21).
                request.started = true;
                self.rotate_root().await.map(|_| ())
            }
            Action::DevicePair {
                token,
                device,
                succession,
            } => {
                let pairing = match succession {
                    true => self.inspect_succession(&token)?.0,
                    false => self.inspect_pairing(&token)?,
                };
                ensure(pairing.device == device, Error::IntegrityFailed)?;
                self.approve_pairing(&pairing).await?;
                // A new root the new device opens.
                request.started = true;
                self.rotate_root().await.map(|_| ())
            }
            Action::Connect { operation, .. } => self.connect_approve(&operation, request).await,
            Action::Checkout { id, digest, fresh } => {
                ensure(
                    self.plan_checkout(&id)?.digest() == digest,
                    Error::IntegrityFailed,
                )?;
                request.started = true;
                self.save_approval(request)?;
                match fresh {
                    true => self.plan_advance(&id).await.map(|_| ()),
                    false => self.plan_execute(&id).await.map(|_| ()),
                }
            }
            Action::Letter {
                recipient,
                order,
                digest,
                ..
            } => {
                let recipient = recipient.parse().map_err(|_| Error::IntegrityFailed)?;
                ensure(
                    self.letter_digest(&recipient, &order).await?.0 == digest,
                    Error::IntegrityFailed,
                )?;
                request.started = true;
                self.save_approval(request)?;
                self.inbox_pay(&recipient, &order).await.map(|_| ())
            }
            Action::Handle { id, digest } => {
                ensure(
                    self.handle_operation(&id)?.digest() == digest,
                    Error::IntegrityFailed,
                )?;
                request.started = true;
                self.save_approval(request)?;
                self.handle_execute(&id).await.map(|_| ())
            }
            Action::Account { change } => self.account_execute(&change).await,
            Action::Principal { change } => self.principal_execute(&change).await,
            // Under the request's own ID, so running it again grants once.
            Action::Grant {
                grantee,
                capabilities,
                days,
            } => {
                let via = request.decided_via.clone().unwrap_or_else(|| "gui".into());
                self.capability_grant_as(
                    &via,
                    request.id[..32].into(),
                    &grantee,
                    capabilities,
                    days,
                )
                .map(|_| ())
            }
            // The job's journal decides: approving again only resubmits
            // what was signed, so nothing here is past the point of no return.
            Action::Delegation { job, digest } => {
                ensure(
                    self.delegation(&job)?.digest() == digest,
                    Error::IntegrityFailed,
                )?;
                self.delegation_execute(&job).await.map(|_| ())
            }
            Action::Wallet { .. } => {
                Err(Error::InvalidInput("wallet approvals run in steps".into()))
            }
            Action::ApprovalKey {
                device_id,
                public_key,
                proof,
            } => {
                self.approval_key_execute(device_id, public_key, proof)
                    .await
            }
            Action::ApprovalKeyRetire { device_id } => self
                .mutate(
                    AccountCommand::SetApprovalKey {
                        device_id,
                        key: None,
                    },
                    None,
                )
                .await
                .map(|_| ()),
            Action::HardwarePolicy { purposes, .. } => self
                .mutate(AccountCommand::SetHardwarePolicy { purposes }, None)
                .await
                .map(|_| ()),
        }
    }

    /// Settle an executed request by `outcome`: done, failed, unknown and
    /// reconciled later, or back to waiting when nothing ran.
    pub(crate) fn finish_approval(
        &mut self,
        request: &mut Request,
        outcome: Result<()>,
    ) -> Result<Json> {
        let now = now_ms();
        request.updated_at = now;
        let result = match outcome {
            // An executor may settle the request itself, such as a web
            // request another device took first.
            Ok(()) if !matches!(request.state, State::Executing | State::Unknown) => Ok(()),
            Ok(()) => {
                request.state = State::Done;
                request.error = None;
                Ok(())
            }
            Err(Error::ExecutionUnknown) => {
                request.state = State::Unknown;
                Err(Error::ExecutionUnknown)
            }
            // It may have run, and nothing can tell any more.
            Err(Error::ResultExpired) => {
                request.state = State::Lost;
                request.error = Some(crate::error_code(&Error::ResultExpired));
                Err(Error::ResultExpired)
            }
            // The service is still working through a step; retry within the
            // request's window so a transient check finishes on its own.
            Err(Error::Pending) if now < request.expires_at => {
                request.state = State::Executing;
                request.error = Some(crate::error_code(&Error::Pending));
                Err(Error::Pending)
            }
            // Past the window, the order's reconcile remains for the person.
            Err(Error::Pending) => {
                request.state = State::Failed;
                request.error = Some(crate::error_code(&Error::Pending));
                Err(Error::Pending)
            }
            // Nothing ran: no login, no network or stale certified data.
            // The request keeps waiting for the person to try again.
            Err(error @ (Error::AuthRequired | Error::Unavailable(_) | Error::PolicyStale))
                if !request.started =>
            {
                request.state = State::AwaitingUser;
                request.error = Some(crate::error_code(&error));
                Err(error)
            }
            Err(error) => {
                request.state = State::Failed;
                request.error = Some(crate::error_code(&error));
                Err(error)
            }
        };
        self.save_approval(request)?;
        result?;
        self.approval_view(request)
    }

    /// The approved requests whose outcome is unknown and that are not
    /// running in steps now: the attestations, or the others.
    fn unsettled(&mut self, attestations: bool) -> Result<Vec<Request>> {
        let now = now_ms();
        self.running.retain(|_, at| now < *at + RUN_MS);
        let rows: Vec<ApprovalRow> = self
            .workspace
            .store
            .approvals(&[State::Executing.name(), State::Unknown.name()])?;
        let mut found = Vec::new();
        for row in rows {
            if self.running.contains_key(&row.id) {
                continue;
            }
            let request = self.approval_record(&row.id)?;
            let attestation = matches!(
                request.action,
                Action::Attest { .. } | Action::AttestAction { .. }
            );
            if attestation == attestations {
                found.push(request);
            }
        }
        Ok(found)
    }

    /// Keep what reconciling a request found. A step that could not run
    /// says nothing about the outcome: the request stays where it is for
    /// the next round.
    fn reconciled(&mut self, request: &mut Request, outcome: Result<()>) -> Result<()> {
        match outcome {
            Err(
                error @ (Error::ExecutionUnknown
                | Error::Unavailable(_)
                | Error::AuthRequired
                | Error::PolicyStale
                | Error::Locked),
            ) => {
                request.error = Some(crate::error_code(&error));
                self.save_approval(request)
            }
            outcome => {
                let _ = self.finish_approval(request, outcome);
                Ok(())
            }
        }
    }

    /// Reconcile every approved request whose outcome is unknown, other
    /// than attestations ([`reconcile_attestations`]); returns how many
    /// there were.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked; each request's own failure is
    /// kept on it.
    pub async fn approvals_reconcile(&mut self) -> Result<usize> {
        let wallets = reconcile_wallets(&self.hold()).await?;
        Ok(wallets + self.approvals_reconcile_others().await?)
    }

    /// Reconcile requests not handled by the wallet or attestation step runners.
    /// The daemon runs the separate runners outside its client lock.
    pub async fn approvals_reconcile_others(&mut self) -> Result<usize> {
        let requests = self
            .unsettled(false)?
            .into_iter()
            .filter(|r| !matches!(r.action, Action::Wallet { .. }))
            .collect::<Vec<_>>();
        let count = requests.len();
        for mut request in requests {
            let outcome = match request.action.clone() {
                Action::SecretPackageImport { .. } | Action::SecretDiscoveryImport { .. } => {
                    self.secret_reconcile(&mut request)?;
                    continue;
                }
                Action::SecretShare { .. } => {
                    self.secret_share_interrupted(&mut request)?;
                    continue;
                }
                Action::SecretMountConfig { .. }
                | Action::SecretMountRead { .. }
                | Action::SecretSsh { .. }
                | Action::SecretSshBinding { .. }
                | Action::SecretProcess { .. }
                | Action::SecretUse { .. }
                | Action::SecretPolicy { .. } => {
                    self.secret_reconcile(&mut request)?;
                    continue;
                }
                // Account changes reconcile through the account journal.
                Action::DeviceCapabilities { .. } => self.resume().await.map(|_| ()),
                Action::DeviceRevoke { .. } | Action::DevicePair { .. } => {
                    match self.resume().await {
                        Ok(_) => self.rotate_root().await.map(|_| ()),
                        Err(error) => Err(error),
                    }
                }
                Action::Connect { operation, .. } => self.connect_reconcile(&operation).await,
                Action::Checkout { id, fresh, .. } => match fresh {
                    true => self.plan_advance(&id).await.map(|_| ()),
                    false => self.plan_execute(&id).await.map(|_| ()),
                },
                Action::Letter {
                    recipient, order, ..
                } => match recipient.parse() {
                    Ok(recipient) => self.inbox_pay(&recipient, &order).await.map(|_| ()),
                    Err(_) => Err(Error::IntegrityFailed),
                },
                Action::Handle { id, .. } => self.handle_execute(&id).await.map(|_| ()),
                Action::Account { change } => self.account_reconcile(&change).await,
                Action::Principal { .. }
                | Action::ApprovalKey { .. }
                | Action::ApprovalKeyRetire { .. }
                | Action::HardwarePolicy { .. } => self.resume().await.map(|_| ()),
                Action::Grant {
                    grantee,
                    capabilities,
                    days,
                } => {
                    let via = request.decided_via.clone().unwrap_or_else(|| "gui".into());
                    let id = request.id[..32].to_string();
                    self.capability_grant_as(&via, id, &grantee, capabilities, days)
                        .map(|_| ())
                }
                Action::Delegation { job, .. } => self.delegation_execute(&job).await.map(|_| ()),
                Action::Wallet { .. } => continue,
                Action::Attest { .. } | Action::AttestAction { .. } => continue,
            };
            self.reconciled(&mut request, outcome)?;
        }
        Ok(count)
    }

    /// Quote a web application's checkout waiting for approval: cash on the
    /// ledger `selection`, or a PANDA waiver backed by the neuron
    /// `selection`, paid or controlled by the payer. The request then
    /// commits to those terms; its digest changes, so the person approves
    /// what was quoted.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a request that is not a checkout and
    /// `Error::Expired` once it is decided.
    pub async fn approval_quote(&mut self, id: &str, selection: &str) -> Result<Json> {
        let mut request = self.approval_record(id)?;
        ensure(
            request.state == State::AwaitingUser && now_ms() < request.expires_at,
            Error::Expired,
        )?;
        let Action::Connect {
            operation,
            checkout,
            ..
        } = &mut request.action
        else {
            return Err(Error::Forbidden);
        };
        let job = self.connect_quote(operation, selection).await?;
        *checkout = Some((job.id.clone(), job.digest()));
        request.updated_at = now_ms();
        self.save_approval(&request)?;
        self.approval_view(&request)
    }

    /// Ask to approve a quoted checkout and pay it from the payer; with
    /// `fresh`, to advance a qualified PANDA claim. A cash
    /// checkout the payer cannot fund now gets no request.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown checkout, the client's
    /// `INSUFFICIENT_FUNDS` code when the payer cannot cover it, and the
    /// payer's or ledger's failure when either cannot be read.
    pub async fn checkout_request(&mut self, source: &str, id: &str, fresh: bool) -> Result<Json> {
        crate::commerce::checkout_request(&self.hold(), source, id, fresh).await
    }

    /// Ask to pay for a quoted letter from the payer.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an order without a quote.
    pub async fn letter_request(
        &mut self,
        source: &str,
        recipient: &dmsg_types::AccountId,
        order: &str,
    ) -> Result<Json> {
        crate::inbox::request(&self.hold(), source, recipient, order).await
    }

    /// Ask to authorize and run a prepared name operation.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown operation, or
    /// `INSUFFICIENT_FUNDS` when a registration cannot be funded.
    pub async fn handle_request(&mut self, source: &str, id: &str) -> Result<Json> {
        crate::handle::request(&self.hold(), source, id).await
    }

    /// Read the operation behind a request for display.
    fn approval_detail(&self, action: &Action, check_files: bool) -> Result<Json> {
        Ok(match action {
            Action::SecretDiscoveryImport { plan } => json!(plan),
            Action::SecretPackageImport { selection } => self.secret_package_detail(selection)?,
            Action::SecretShare { id, digest } => self.secret_share_detail(id, digest)?,
            Action::SecretMountConfig { config } => json!(config),
            Action::SecretMountRead { plan } => json!(plan),
            Action::SecretSsh { plan } => json!(plan),
            Action::SecretSshBinding { binding } => json!(binding),
            Action::SecretProcess { plan } => plan.view(),
            Action::SecretPolicy { change } => json!(change),
            Action::SecretUse { plan } => plan.view(),
            Action::Attest {
                statement,
                origin,
                file,
            } => json!({
                "statement": crate::attest::statement_view(statement),
                "purpose": format!("{:?}", dmsg_protocol::statement_purpose(statement)),
                "origin": origin,
                "file": file.as_ref().map(|f| json!({
                    "name": f.name,
                    "size": f.size,
                    "path": f.path,
                    "library": f.library.as_ref().map(|(file, version)| json!({"file": file, "version": version})),
                })),
                // A local file is compared by size and time here, and hashed
                // again on approval; a library version by its record.
                "file_checked": file.as_ref().and_then(|f| check_files.then(|| self.attest_file_unchanged(statement, f))),
            }),
            Action::AttestAction { action, schema } => {
                let action: AppAction = dmsg_protocol::decode_canonical(action)?;
                let origin = action.origin.clone();
                let rendered =
                    crate::attest::render_action(&action, schema.as_ref().map(|s| s.as_slice()));
                let statement = Statement {
                    issuer: self.account()?.issuer.clone(),
                    subject: None,
                    issued_at: None,
                    content: StatementContent::AppAction(Box::new(action)),
                };
                json!({
                    "statement": crate::attest::statement_view(&statement),
                    "purpose": "AppAction",
                    "action": rendered,
                    "origin": origin,
                    "file": null,
                    "file_checked": null,
                })
            }
            Action::DeviceCapabilities {
                device_id,
                capabilities,
            } => json!({
                "device_id": hex::encode(**device_id),
                "capabilities": capabilities.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
                "this_device": *device_id == self.workspace.meta.device_id,
            }),
            Action::DeviceRevoke { device_id } => json!({
                "device_id": hex::encode(**device_id),
                "this_device": *device_id == self.workspace.meta.device_id,
            }),
            Action::DevicePair {
                device, succession, ..
            } => json!({
                "succession": succession,
                "device_id": hex::encode(*device.device_id),
                "role": format!("{:?}", device.role),
                "capabilities": device.capabilities.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
            }),
            Action::Connect {
                operation,
                checkout,
                ..
            } => {
                let mut view = self.connect_view(operation)?;
                if let Some((id, _)) = checkout {
                    view["checkout"] = self.plan_checkout(id)?.view();
                }
                view
            }
            Action::Checkout { id, fresh, .. } => {
                let mut view = self.plan_checkout(id)?.view();
                view["fresh"] = (*fresh).into();
                view
            }
            Action::Letter {
                recipient,
                order,
                amount,
                ledger,
                ..
            } => {
                json!({"recipient": recipient, "order": order, "amount": amount, "ledger": ledger})
            }
            Action::Handle { id, .. } => self.handle_operation(id)?.view(),
            Action::Account { change } => change.view(),
            Action::Principal { change } => json!(change),
            Action::Grant {
                grantee,
                capabilities,
                days,
            } => json!({"grantee": grantee, "capabilities": capabilities, "days": days}),
            Action::Delegation { job, .. } => self.delegation(job)?.view(),
            Action::Wallet { job, .. } => self.wallet_job_view(job)?,
            Action::ApprovalKey { .. }
            | Action::ApprovalKeyRetire { .. }
            | Action::HardwarePolicy { .. } => {
                crate::approval_key::detail(self, action).ok_or(Error::IntegrityFailed)?
            }
        })
    }

    /// The view a client renders: the request, its digest and a summary of
    /// what approving does. A finished request outlives a discarded checkout,
    /// forgotten name operation or expired web operation; its history remains
    /// readable with null detail. Unsettled requests still require the detail.
    pub(crate) fn approval_view(&self, request: &Request) -> Result<Json> {
        self.approval_view_with_files(request, true)
    }

    fn approval_view_with_files(&self, request: &Request, check_files: bool) -> Result<Json> {
        let detail = match self.approval_detail(&request.action, check_files) {
            Err(Error::NotFound) if request.state.terminal() => Json::Null,
            detail => detail?,
        };
        Ok(json!({
            "id": request.id,
            "source": request.source,
            "kind": request.action.class(),
            "type": match &request.action {
                Action::SecretDiscoveryImport { .. } => "secret_discovery_import",
                Action::SecretPackageImport { .. } => "secret_package_import",
                Action::SecretShare { .. } => "secret_share",
                Action::SecretMountConfig { .. } => "secret_mount_config",
                Action::SecretMountRead { .. } => "secret_mount_read",
                Action::SecretSsh { .. } => "secret_ssh",
                Action::SecretSshBinding { .. } => "secret_ssh_binding",
                Action::SecretProcess { .. } => "secret_process",
                Action::SecretPolicy { .. } => "secret_policy",
                Action::SecretUse { .. } => "secret_use",
                Action::Attest { .. } | Action::AttestAction { .. } => "attest",
                Action::DeviceCapabilities { .. } => "device_capabilities",
                Action::DeviceRevoke { .. } => "device_revoke",
                Action::DevicePair { .. } => "device_pair",
                Action::Connect { .. } => "connect",
                Action::Checkout { .. } => "checkout",
                Action::Letter { .. } => "letter",
                Action::Handle { .. } => "handle",
                Action::Account { .. } => "account",
                Action::Principal { .. } => "principal",
                Action::Delegation { .. } => "delegation",
                Action::Grant { .. } => "grant",
                Action::Wallet { .. } => "wallet",
                Action::ApprovalKey { .. } => "approval_key",
                Action::ApprovalKeyRetire { .. } => "approval_key_retire",
                Action::HardwarePolicy { .. } => "hardware_policy",
            },
            "state": request.state.name(),
            "risk": if high_risk(request, &detail) { "high" } else { "normal" },
            "created_at": request.created_at,
            "expires_at": request.expires_at,
            "updated_at": request.updated_at,
            "decided_at": request.decided_at,
            "decided_via": request.decided_via,
            "digest": hex::encode(*request.digest()),
            "execution": request.execution,
            "error": request.error,
            "file_check": request.file_check,
            "result": request.secret_result,
            "delivery_report": request.delivery_report,
            "reviewed_at": request.reviewed_at,
            "detail": detail,
        }))
    }
}

/// Read requests in short local steps. Lists omit filesystem checks; the
/// full detail and approval execution still check the selected file.
/// # Errors
/// As [`Client::approvals`]; a lock interrupts the read.
pub async fn list<H: Hold>(h: &H, all: bool) -> Result<Vec<Json>> {
    let h = Unlocked::new(h).await?;
    let rows = h
        .with(|c| {
            c.expire_approvals(now_ms())?;
            c.workspace.store.approvals(if all {
                &[]
            } else {
                &["awaiting_user", "executing", "unknown"]
            })
        })
        .await?;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        h.with(|c| {
            let request = c.approval_record(&row.id)?;
            if all || !request.state.terminal() {
                views.push(c.approval_view_with_files(&request, false)?);
            }
            Ok(())
        })
        .await?;
    }
    h.with(|_| Ok(())).await?;
    Ok(views)
}

/// The other requests waiting for a decision, from the index alone.
/// # Errors
/// Returns `Error::Locked` while locked.
pub async fn waiting_ids<H: Hold>(h: &H) -> Result<Vec<String>> {
    h.with(|c| {
        c.keys()?;
        c.expire_approvals(now_ms())?;
        Ok(c.workspace
            .store
            .approvals(&[State::AwaitingUser.name()])?
            .into_iter()
            .map(|r| r.id)
            .collect())
    })
    .await
}

/// Reconcile every approved attestation whose outcome is unknown, its user
/// home waited on without the client; returns how many there were. The
/// other requests reconcile in [`Client::approvals_reconcile`].
///
/// # Errors
/// Returns `Error::Locked` while locked; each request's own failure is kept
/// on it.
pub async fn reconcile_attestations<H: Hold>(h: &H) -> Result<usize> {
    let requests = h
        .with(|c| {
            // Finished attestations the account does not keep yet, such as
            // one signed while the root was being replaced.
            let _ = c.attest_backfill();
            c.unsettled(true)
        })
        .await?;
    let count = requests.len();
    for request in requests {
        let outcome = match &request.execution {
            Some(execution) => crate::attest::reconcile(h, execution).await.map(|_| ()),
            // Approved, but the attestation was never prepared.
            None => Err(Error::Expired),
        };
        h.with(|c| {
            let mut request = c.approval_record(&request.id)?;
            c.reconciled(&mut request, outcome)
        })
        .await?;
    }
    Ok(count)
}

/// Ask to replace a device's capabilities, such as turning on
/// `FormalApprove` for this device; an administrator device holding
/// `RootManage` approves it, its own included. The user home is waited on
/// without the client.
///
/// # Errors
/// Returns `Error::Forbidden` on a device that cannot approve account
/// changes and `Error::DeviceNotApproved` for an unknown or revoked device.
pub async fn capabilities_request<H: Hold>(
    h: &H,
    source: &str,
    device_id: Hash,
    capabilities: Vec<Capability>,
) -> Result<Json> {
    h.with(|c| c.keys().map(|_| ())).await?;
    let security = crate::account::refreshed_security(h).await?;
    h.with(|c| {
        let own = security.active(&c.workspace.meta.device_id)?;
        ensure(
            own.input.role == dmsg_types::user::ControllerRole::Administrator
                && own.input.capabilities.contains(&Capability::RootManage),
            Error::Forbidden,
        )?;
        let mut input = security.active(&device_id)?.input.clone();
        input.capabilities = capabilities.clone();
        dmsg_protocol::DeviceInputExt::validate(&input)?;
        let request = c.approval_create(
            source,
            Action::DeviceCapabilities {
                device_id,
                capabilities,
            },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}

/// Ask to revoke another device of the account; approving also rotates the
/// content root, so the revoked device cannot read what comes next. The
/// user home is waited on without the client.
///
/// # Errors
/// Returns `Error::Forbidden` on a device that cannot approve account
/// changes or for this device itself.
pub async fn revoke_request<H: Hold>(h: &H, source: &str, device_id: Hash) -> Result<Json> {
    h.with(|c| c.keys().map(|_| ())).await?;
    let security = crate::account::refreshed_security(h).await?;
    h.with(|c| {
        let own = security.active(&c.workspace.meta.device_id)?;
        ensure(
            own.input.role == dmsg_types::user::ControllerRole::Administrator
                && own.input.capabilities.contains(&Capability::RootManage)
                && device_id != c.workspace.meta.device_id,
            Error::Forbidden,
        )?;
        security.active(&device_id)?;
        let request = c.approval_create(
            source,
            Action::DeviceRevoke { device_id },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}

/// Ask to enroll the device of a pairing `token`; `device` is the ID the new
/// device shows, which the person names. Approving also rotates the content
/// root to include it. The request waits no longer than the token.
///
/// # Errors
/// Returns `Error::Forbidden` on a device that cannot approve account
/// changes, `Error::InvalidInput` when `device` is not the token's, and
/// refuses a token of another account, an expired one or a bad proof of
/// possession.
pub async fn add_device_request<H: Hold>(
    h: &H,
    source: &str,
    token: &str,
    device: &str,
) -> Result<Json> {
    h.with(|c| c.keys().map(|_| ())).await?;
    let security = crate::account::refreshed_security(h).await?;
    h.with(|c| {
        let own = security.active(&c.workspace.meta.device_id)?;
        ensure(
            own.input.role == dmsg_types::user::ControllerRole::Administrator
                && own.input.capabilities.contains(&Capability::RootManage),
            Error::Forbidden,
        )?;
        let pairing = c.inspect_pairing(token)?;
        ensure(
            hex::encode(*pairing.device.device_id) == device,
            Error::InvalidInput("device does not match the request".into()),
        )?;
        let expires_at = pairing
            .mutation
            .approval
            .expires_at
            .min(now_ms() + HIGH_RISK_TTL_MS);
        let request = c.approval_create(
            source,
            Action::DevicePair {
                token: token.into(),
                device: pairing.device,
                succession: false,
            },
            expires_at,
        )?;
        c.approval_view(&request)
    })
    .await
}

/// This device's capabilities with `FormalApprove` turned on or off.
///
/// # Errors
/// As [`capabilities_request`].
pub async fn formal_approve_request<H: Hold>(h: &H, source: &str, enable: bool) -> Result<Json> {
    let security = crate::account::refreshed_security(h).await?;
    let device_id = h.with(|c| Ok(c.workspace.meta.device_id)).await?;
    let mut capabilities = security.active(&device_id)?.input.capabilities.clone();
    capabilities.retain(|c| *c != Capability::FormalApprove);
    if enable {
        capabilities.push(Capability::FormalApprove);
    }
    capabilities_request(h, source, device_id, capabilities).await
}

/// The SHA-256 a document statement names, if any.
pub(crate) fn statement_digest(statement: &Statement) -> Option<Hash> {
    match &statement.content {
        StatementContent::Digest { sha256, .. }
        | StatementContent::FileStatement { sha256, .. } => Some(*sha256),
        _ => None,
    }
}

/// An approved wallet job. Only the approval center constructs one.
pub struct WalletRun {
    request: Request,
}

impl WalletRun {
    /// Run without a client lock and record the outcome on the same request.
    pub async fn run<H: Hold>(self, h: &H) -> Result<Json> {
        let Action::Wallet { job, digest } = &self.request.action else {
            return Err(Error::IntegrityFailed);
        };
        h.with(|c| ensure(c.wallet_job_digest(job)? == *digest, Error::IntegrityFailed))
            .await?;
        let outcome = crate::wallet::ops::job_execute(h, job).await;
        h.with(|c| {
            c.running.remove(&self.request.id);
            let mut request = c.approval_record(&self.request.id)?;
            c.finish_approval(&mut request, outcome)
        })
        .await
    }
}

/// Resume wallet jobs independently of unrelated local operations.
pub async fn reconcile_wallets<H: Hold>(h: &H) -> Result<usize> {
    let requests = h
        .with(|c| {
            Ok(c.unsettled(false)?
                .into_iter()
                .filter(|r| matches!(r.action, Action::Wallet { .. }))
                .collect::<Vec<_>>())
        })
        .await?;
    let count = requests.len();
    for request in requests {
        let Action::Wallet { job, .. } = &request.action else {
            continue;
        };
        h.with(|c| {
            c.running.insert(request.id.clone(), now_ms());
            Ok(())
        })
        .await?;
        let outcome = crate::wallet::ops::job_execute(h, job).await;
        h.with(|c| {
            c.running.remove(&request.id);
            let mut request = c.approval_record(&request.id)?;
            c.reconciled(&mut request, outcome)
        })
        .await?;
    }
    Ok(count)
}
