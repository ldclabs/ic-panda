//! Account control against the user home (`docs/protocol/account_root_zh.md`):
//! certified account state, account creation, device-approved mutations and
//! device pairing.
//!
//! Every update is written to an encrypted journal before it is sent. A lost
//! reply leaves the journal `unknown`; the next call reconciles it by its
//! operation ID and never signs a second request for the same intent.

use crate::client::Client;
use crate::cloud::{Evidence, EvidenceEntry};
use crate::codec::{MAX_RECORD, b64, canonical, decode, digest, unb64};
use crate::config::Config;
use crate::crypto::{random_id, sign};
use crate::hold::Hold;
use crate::ic::Ic;
use crate::now_ms;
use crate::workspace::AccountRef;
use candid::{CandidType, Principal};
use dmsg_protocol::{approval_message, authentication::verify_certified_leaf, verify};
use dmsg_types::{
    AccountId, Approval, CertifiedBatch, Ed25519Signature, Error, Hash, Result, ensure,
    user::{
        AccountCommand, AccountInfo, AccountMutation, ControllerRole, CreateAccount, Device,
        OperationReceipt, SecuritySnapshot, UserInit,
    },
};
use futures_util::future::{join, join3, try_join_all};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A home accepts approvals at most five minutes ahead; leave a skew margin.
pub(crate) const APPROVAL_MS: u64 = 270_000;
/// A name authorization at most one minute ahead.
const HANDLE_APPROVAL_MS: u64 = 50_000;
const JOURNAL: &str = "journal";
const PAIRING: &str = "dmsg-device-request/2";

/// An account's certified security leaf and complete device map: what the
/// relay verifies before trusting a device. Both queries are public, so a
/// device reads it without a login.
#[derive(Clone, Debug)]
pub struct Security {
    /// The certified security leaf.
    pub snapshot: SecuritySnapshot,
    /// The device map the leaf commits to.
    pub devices: BTreeMap<Hash, Device>,
    /// The same proof for the relay.
    pub evidence: Evidence,
    /// Certificate time in Unix milliseconds.
    pub certified_at: u64,
}

impl Security {
    /// The certificate stops being fresh 60 seconds after its time.
    pub fn expires_at(&self) -> u64 {
        self.certified_at + 60_000
    }

    /// A device of the account, revoked or not.
    pub fn device(&self, id: &Hash) -> Option<&Device> {
        self.devices.get(id)
    }

    /// An active device.
    ///
    /// # Errors
    /// Returns `Error::DeviceNotApproved` for a missing or revoked device.
    pub fn active(&self, id: &Hash) -> Result<&Device> {
        self.device(id)
            .filter(|d| d.revoked_at.is_none())
            .ok_or(Error::DeviceNotApproved)
    }
}

/// Account state proven by an IC certificate: the owner's `get_account`
/// view checked against the certified [`Security`].
#[derive(Clone, Debug)]
pub struct AccountState {
    /// The account as `get_account` returned it, checked against the leaf.
    pub info: AccountInfo,
    /// The certified leaf and devices.
    pub security: Security,
}

impl AccountState {
    /// A device of the account, revoked or not.
    pub fn device(&self, id: &Hash) -> Option<&Device> {
        self.security.device(id)
    }

    /// An active device.
    ///
    /// # Errors
    /// Returns `Error::DeviceNotApproved` for a missing or revoked device.
    pub fn active(&self, id: &Hash) -> Result<&Device> {
        self.security.active(id)
    }
}

/// Check the certified leaf of `account` and the device map it commits to.
///
/// # Errors
/// Anything inconsistent returns `Error::IntegrityFailed`.
pub fn verify_security(
    home: Principal,
    account: &AccountId,
    batch: &CertifiedBatch,
    devices: BTreeMap<Hash, Device>,
    root_key: &[u8],
    now: u64,
) -> Result<Security> {
    let (value, certified_at) =
        verify_certified_leaf(batch, account.as_slice(), home, root_key, now)?;
    let snapshot: SecuritySnapshot = decode(&value, MAX_RECORD)?;
    ensure(
        snapshot.schema == 4
            && snapshot.account_id == *account
            && snapshot.home_user == home
            && snapshot.devices_root == digest("dmsg/devices/v1", &devices),
        Error::IntegrityFailed,
    )?;
    let entry = &batch.entries[0];
    let evidence = Evidence {
        schema: 1,
        canister: home.to_text(),
        certificate: b64(&batch.certificate),
        entries: vec![EvidenceEntry {
            account_id: account.to_string(),
            value: b64(&value),
            witness: b64(&entry.witness),
            devices: b64(&canonical(&devices)),
        }],
    };
    Ok(Security {
        snapshot,
        devices,
        evidence,
        certified_at,
    })
}

/// Check `get_account`, the certified leaf and the device map against each
/// other; any mismatch rejects instead of mixing two versions.
///
/// # Errors
/// A concurrent change returns `Error::PolicyStale`; anything inconsistent
/// returns `Error::IntegrityFailed`.
pub fn verify_account(
    home: Principal,
    account: &AccountId,
    info: AccountInfo,
    batch: &CertifiedBatch,
    devices: BTreeMap<Hash, Device>,
    root_key: &[u8],
    now: u64,
) -> Result<AccountState> {
    ensure(
        info.account_id == *account && info.home_user == home,
        Error::IntegrityFailed,
    )?;
    ensure(
        canonical(&info.devices) == canonical(&devices),
        Error::PolicyStale,
    )?;
    let security = verify_security(home, account, batch, devices, root_key, now)?;
    let snapshot = &security.snapshot;
    let pending = info
        .pending_recovery
        .as_ref()
        .map(|p| digest("dmsg/pending-recovery/v1", p));
    ensure(
        snapshot.issuer == info.issuer
            && snapshot.home_cose == info.home_cose
            && snapshot.account_version == info.account_version
            && snapshot.security_epoch == info.security_epoch
            && snapshot.vault_write_state == info.vault_write_state
            && snapshot.recovery_delay_ms == info.recovery_delay_ms
            && snapshot.content_root_generation
                == info.current_root.as_ref().map_or(0, |r| r.generation)
            && snapshot.content_root_digest == info.current_root.as_ref().map(|r| r.bundle_digest)
            && snapshot.pending_recovery_digest == pending,
        Error::IntegrityFailed,
    )?;
    Ok(AccountState { info, security })
}

/// One account update, journaled before dispatch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// `create_account`.
    Create(CreateAccount),
    /// `mutate_account`.
    Mutate(AccountMutation),
}

/// Where a journaled update stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Written, not yet sent.
    Prepared,
    /// Sent; the reply was not seen.
    Unknown,
    /// The home confirmed it.
    Confirmed,
    /// The home refused it.
    Rejected,
}

/// The encrypted journal entry of the latest account update.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Journal {
    /// The user home.
    pub home: Principal,
    /// The login that sends it.
    pub caller: Principal,
    /// The account, once known.
    pub account: Option<AccountId>,
    /// The signed request.
    pub request: Request,
    /// Progress.
    pub stage: Stage,
}

#[derive(Serialize, Deserialize)]
struct Pairing {
    format: String,
    home: Principal,
    request: AccountMutation,
}

/// A pairing request another device produced, after its checks.
#[derive(Clone, Debug)]
pub struct PairingRequest {
    /// The account the device asks to join.
    pub account: AccountId,
    /// The requesting device's enrollment record.
    pub device: dmsg_types::user::DeviceInput,
    /// The signed `AddDevice` mutation the administrator approves.
    pub mutation: AccountMutation,
}

/// What the client reads of the registry's `get_handle_config`. The other
/// fields, such as the revenue account, stay the registry's own: a registry
/// that adds or lacks one still answers.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct HandleConfig {
    /// Every user home; the authoritative list accounts are routed with.
    pub user_homes: Vec<Principal>,
    /// The user homes accepting new accounts.
    pub registration_homes: Vec<Principal>,
    /// The ledger of registration charges.
    pub ledger: Principal,
    /// The ledger fee in base units.
    pub ledger_fee: u128,
}

/// The account's certified security as its user home answers now, read
/// with the public queries and verified, without a client.
///
/// # Errors
/// As [`verify_security`].
pub(crate) async fn fetch_security(
    ic: &Ic,
    home: Principal,
    account: &AccountId,
) -> Result<Security> {
    let (batch, bundle) = Box::pin(join(
        security_batch(ic, home, account),
        device_bundle(ic, home, account),
    ))
    .await;
    verify_security(
        home,
        account,
        &batch??,
        bundle??.1,
        &ic.root_key(),
        now_ms(),
    )
}

/// The handle registry's user homes, checked against the configured pins.
///
/// # Errors
/// An inconsistent list returns `Error::IntegrityFailed`.
pub async fn user_homes(ic: &Ic, handle: Principal, pins: &[Principal]) -> Result<HandleConfig> {
    let config: HandleConfig = ic.query(handle, "get_handle_config", ()).await?;
    ensure(
        !config.user_homes.is_empty()
            && config
                .registration_homes
                .iter()
                .chain(pins)
                .all(|home| config.user_homes.contains(home)),
        Error::IntegrityFailed,
    )?;
    Ok(config)
}

/// The account of `ic`'s caller across `homes`, asking every home at once.
///
/// # Errors
/// Returns `Error::Unavailable` when any home cannot answer.
pub(crate) async fn find_account(
    ic: &Ic,
    homes: &[Principal],
) -> Result<Option<(Principal, AccountId)>> {
    let found = try_join_all(homes.iter().map(|home| async move {
        let found: Option<AccountId> = ic.query(*home, "my_account", ()).await?;
        Ok::<_, Error>(found.map(|account| (*home, account)))
    }))
    .await?;
    Ok(found.into_iter().flatten().next())
}

/// An account's certified security leaf at `home`.
pub(crate) async fn security_batch(
    ic: &Ic,
    home: Principal,
    account: &AccountId,
) -> Result<Result<CertifiedBatch>> {
    ic.query(home, "security_snapshot_batch", (vec![*account],))
        .await
}

/// An account's device map at `home`.
pub(crate) async fn device_bundle(
    ic: &Ic,
    home: Principal,
    account: &AccountId,
) -> Result<Result<(SecuritySnapshot, BTreeMap<Hash, Device>)>> {
    ic.query(home, "get_device_bundle", (account,)).await
}

/// Every canister a login delegation must reach: the registry and its
/// ledger, each user home with its COSE executor, payment, commerce and
/// membership canisters, the ledgers those settle on, and the configured
/// wallet hub's ICP ledger, so the login's Principal can also pay as the payer.
///
/// # Errors
/// Returns `Error::Unavailable` when a query fails and
/// `Error::QuotaExceeded` beyond the 32 targets one login may name.
pub async fn login_targets(config: &Config) -> Result<Vec<Principal>> {
    let ic = Ic::anonymous(config).await?;
    let homes = user_homes(&ic, config.handle, &config.user_homes).await?;
    let mut targets = vec![config.handle, homes.ledger];
    let mut add = |canister: Principal| {
        if !targets.contains(&canister) {
            targets.push(canister);
        }
    };
    for home in homes.user_homes {
        let init: UserInit = ic.query(home, "user_config", ()).await?;
        for canister in [
            home,
            init.home_cose,
            init.payment_canister,
            init.commerce_canister,
            init.membership_canister,
        ] {
            add(canister);
        }
        let payment: dmsg_types::payment::PaymentInit = ic
            .query(init.payment_canister, "payment_config", ())
            .await?;
        add(payment.ledger);
        let assets: Vec<dmsg_types::integration_billing::SettlementAssetView> = ic
            .query(init.commerce_canister, "settlement_assets", ())
            .await?;
        for asset in assets {
            add(asset.policy.ledger);
        }
    }
    if let Some(hub) = config.wallet.hub {
        let wallet: dmsg_types::wallet::HubInit = ic.query(hub, "get_hub_config", ()).await?;
        add(wallet.icp_ledger);
    }
    ensure(
        targets.len() <= crate::login::MAX_TARGETS,
        Error::QuotaExceeded,
    )?;
    Ok(targets)
}

/// A device as the account lists it.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct DeviceView {
    /// Device ID, hex.
    pub device_id: String,
    /// `Administrator` or `Member`.
    pub role: String,
    /// Capability names.
    pub capabilities: Vec<String>,
    /// Enrollment time in Unix milliseconds.
    pub added_at: u64,
    /// The approving device, hex.
    pub added_by: Option<String>,
    /// Revocation time in Unix milliseconds.
    pub revoked_at: Option<u64>,
    /// This device.
    pub current: bool,
}

/// The account as this device last verified it.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountView {
    /// Account ID.
    pub account: String,
    /// User home.
    pub home: String,
    /// Issuer URI.
    pub issuer: String,
    /// Account version.
    pub account_version: u64,
    /// Security epoch.
    pub security_epoch: u64,
    /// `Uninitialized`, `Ready` or `RekeyRequired`.
    pub vault_write_state: String,
    /// Committed root generation; 0 before the first.
    pub root_generation: u64,
    /// Every device, revoked ones included.
    pub devices: Vec<DeviceView>,
}

impl Client {
    /// The account's certified state, for display, from `state` already
    /// fetched, such as by a [`Self::warmup`] that ran without the client; no
    /// login needed.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` before the device has an account.
    pub fn account_view_of(&self, state: &Security) -> Result<AccountView> {
        let account = self.account()?.clone();
        ensure(
            state.snapshot.account_id == account.id,
            Error::Unavailable("the account changed".into()),
        )?;
        let devices = state
            .devices
            .iter()
            .map(|(id, d)| DeviceView {
                device_id: hex::encode(**id),
                role: format!("{:?}", d.input.role),
                capabilities: d
                    .input
                    .capabilities
                    .iter()
                    .map(|c| format!("{c:?}"))
                    .collect(),
                added_at: d.added_at,
                added_by: d.added_by.map(|h| hex::encode(*h)),
                revoked_at: d.revoked_at,
                current: *id == self.workspace.meta.device_id,
            })
            .collect();
        let snapshot = &state.snapshot;
        Ok(AccountView {
            account: account.id.to_string(),
            home: account.home.to_text(),
            issuer: snapshot.issuer.clone(),
            account_version: snapshot.account_version,
            security_epoch: snapshot.security_epoch,
            vault_write_state: format!("{:?}", snapshot.vault_write_state),
            root_generation: snapshot.content_root_generation,
            devices,
        })
    }

    /// The login's account across every user home. A failed query leaves the
    /// answer unknown and is an error, never a reason to register again.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a login and `Error::Unavailable`
    /// when any home cannot answer.
    pub async fn locate(&self) -> Result<Option<(Principal, AccountId)>> {
        // Read the homes afresh: a home added since must not hide an
        // account and lead to a second registration.
        let ic = &self.session()?.ic;
        let homes = user_homes(ic, self.config.handle, &self.config.user_homes).await?;
        find_account(ic, &homes.user_homes).await
    }

    /// Fetch and verify the account's current state with the login,
    /// refusing anything older than this device has already seen.
    ///
    /// # Errors
    /// As [`verify_account`]; an older answer returns `Error::PolicyStale`.
    pub async fn refresh(&mut self, home: Principal, account: &AccountId) -> Result<AccountState> {
        let ic = self.session()?.ic.clone();
        let state = read_account(&ic, home, account).await?;
        self.observe(account, &state.security)?;
        Ok(state)
    }

    /// Fetch and verify the account's certified security with the public
    /// queries; works with or without a login.
    ///
    /// # Errors
    /// As [`verify_security`]; an older answer returns `Error::PolicyStale`.
    pub async fn refresh_security(
        &mut self,
        home: Principal,
        account: &AccountId,
    ) -> Result<Security> {
        let ic = self.reader().await?;
        let security = fetch_security(&ic, home, account).await?;
        self.observe(account, &security)?;
        Ok(security)
    }

    /// Check this device's keys and the high-water mark, then keep `security`.
    pub(crate) fn observe(&mut self, account: &AccountId, security: &Security) -> Result<()> {
        if let Some(own) = security.device(&self.workspace.meta.device_id) {
            ensure(
                own.input.signing_pub == self.workspace.meta.signing_pub
                    && own.input.hpke_pub == self.workspace.meta.hpke_pub,
                Error::IntegrityFailed,
            )?;
        }
        let snapshot = &security.snapshot;
        let seen = (
            snapshot.account_version,
            snapshot.security_epoch,
            snapshot.content_root_generation,
        );
        let key = format!("observed:{account}");
        if let Some(previous) = self.workspace.store.meta::<(u64, u64, u64)>(&key)? {
            ensure(
                seen.0 >= previous.0 && seen.1 >= previous.1 && seen.2 >= previous.2,
                Error::PolicyStale,
            )?;
        }
        self.workspace.store.set_meta(&key, &seen)?;
        self.security = Some(security.clone());
        Ok(())
    }

    /// The pending journal entry, if the latest update is unsettled.
    ///
    /// # Errors
    /// Returns `Error::Locked` without unlocked keys.
    pub fn pending(&self) -> Result<Option<Journal>> {
        let journal: Option<Journal> = self.workspace.private(self.keys()?, JOURNAL)?;
        Ok(journal.filter(|j| matches!(j.stage, Stage::Prepared | Stage::Unknown)))
    }

    fn save_journal(&self, journal: &Journal) -> Result<()> {
        self.workspace.put_private(self.keys()?, JOURNAL, journal)
    }

    async fn dispatch(&mut self, mut journal: Journal) -> Result<Journal> {
        let login = self.session()?;
        ensure(journal.caller == login.principal, Error::AuthRequired)?;
        // Record `unknown` first: the home may commit before the reply is lost.
        journal.stage = Stage::Unknown;
        self.save_journal(&journal)?;
        let reply = match &journal.request {
            Request::Create(input) => login
                .ic
                .update::<_, Result<AccountId>>(journal.home, "create_account", (input,))
                .await
                .map(|r| r.map(Some)),
            Request::Mutate(mutation) => login
                .ic
                .update::<_, Result<OperationReceipt>>(journal.home, "mutate_account", (mutation,))
                .await
                .map(|r| {
                    r.and_then(|receipt| {
                        ensure(
                            receipt.id == mutation.approval.request_id
                                && receipt.digest == digest("dmsg/account-operation/v2", mutation),
                            Error::IntegrityFailed,
                        )
                        .map(|_| None)
                    })
                }),
        }?;
        match reply {
            Ok(account) => {
                if account.is_some() {
                    journal.account = account;
                }
                journal.stage = Stage::Confirmed;
                self.save_journal(&journal)?;
                Ok(journal)
            }
            Err(error) => {
                // An expired or pruned receipt after an earlier dispatch is no
                // proof the request never committed; keep it for reconciling.
                if !matches!(error, Error::ResultExpired | Error::Expired)
                    || matches!(journal.request, Request::Create(_))
                {
                    journal.stage = Stage::Rejected;
                    self.save_journal(&journal)?;
                }
                Err(error)
            }
        }
    }

    /// Settle an unsettled update by its operation ID before anything new.
    ///
    /// # Errors
    /// Returns the home's verdict; `Error::ExecutionUnknown` when it is still unknown.
    pub async fn resume(&mut self) -> Result<Option<Journal>> {
        let Some(mut journal) = self.pending()? else {
            return Ok(None);
        };
        match &journal.request {
            Request::Create(_) => {
                if let Some((home, account)) = self.locate().await? {
                    ensure(home == journal.home, Error::IntegrityFailed)?;
                    journal.account = Some(account);
                    journal.stage = Stage::Confirmed;
                    self.save_journal(&journal)?;
                    return Ok(Some(journal));
                }
            }
            Request::Mutate(mutation) => {
                let found: Result<OperationReceipt> = self
                    .session()?
                    .ic
                    .query(
                        journal.home,
                        "get_operation",
                        (&mutation.account_id, &mutation.approval.request_id),
                    )
                    .await?;
                match found {
                    Ok(receipt) => {
                        ensure(
                            receipt.digest == digest("dmsg/account-operation/v2", mutation),
                            Error::IntegrityFailed,
                        )?;
                        journal.stage = Stage::Confirmed;
                        self.save_journal(&journal)?;
                        return Ok(Some(journal));
                    }
                    Err(Error::ResultExpired) => {
                        let mutation = mutation.clone();
                        let state = self.refresh(journal.home, &mutation.account_id).await?;
                        let device = state.device(&mutation.approval.device_id);
                        // A certified time past the deadline with the version and
                        // sequence unchanged proves the request can never apply.
                        if state.security.certified_at >= mutation.approval.expires_at
                            && state.info.account_version == mutation.expected_version
                            && device.map(|d| d.next_sequence) == Some(mutation.approval.sequence)
                        {
                            journal.stage = Stage::Rejected;
                            self.save_journal(&journal)?;
                            return Err(Error::Expired);
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        self.dispatch(journal).await.map(Some)
    }

    /// Create an account, or finish its interrupted initial root setup.
    /// Repeating this after setup opens the committed root without rotating it.
    ///
    /// # Errors
    /// As [`Self::create_account`], plus network, verification or device
    /// authorization failures while opening or committing the initial root.
    /// An existing account must already be bound to this unlocked device.
    pub async fn initialize_account(&mut self) -> Result<(AccountRef, u64)> {
        self.keys()?;
        let account = if self.workspace.provisional() {
            self.create_account().await?
        } else {
            self.account()?.clone()
        };
        let generation = self.initialize_root().await?;
        Ok((account, generation))
    }

    /// Create the login's account with this provisional device as its first
    /// administrator, then bind the workspace to it.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the workspace already has an account
    /// and `Error::IdempotencyConflict` when the login already owns one.
    pub async fn create_account(&mut self) -> Result<AccountRef> {
        ensure(self.workspace.provisional(), Error::Forbidden)?;
        if let Some(journal) = self.resume().await?
            && let (Request::Create(_), Some(account)) = (&journal.request, journal.account)
        {
            return self.join(journal.home, account).await;
        }
        ensure(self.locate().await?.is_none(), Error::IdempotencyConflict)?;
        let login = self.session()?;
        let homes = user_homes(&login.ic, self.config.handle, &self.config.user_homes).await?;
        let index = usize::from(random_id()[0]) % homes.registration_homes.len().max(1);
        let home = *homes
            .registration_homes
            .get(index)
            .ok_or_else(|| Error::Unavailable("no registration home".into()))?;
        let init: UserInit = login.ic.query(home, "user_config", ()).await?;
        let admission = match init.admission_key {
            Some(_) => Some(self.cloud.admission(home, login.principal).await?),
            None => None,
        };
        let caller = login.principal;
        let now = now_ms();
        let mut input = CreateAccount {
            device: self.workspace.device_input(ControllerRole::Administrator),
            op_id: Hash::new(random_id()),
            expires_at: now + APPROVAL_MS,
            proof: Ed25519Signature::new([0; 64]),
            admission,
        };
        let message = digest(
            "dmsg/create-account/v1",
            &(home, caller, &input.device, input.op_id, input.expires_at),
        );
        input.proof = Ed25519Signature::new(sign(&self.keys()?.signing, message.as_slice()));
        let journal = Journal {
            home,
            caller,
            account: None,
            request: Request::Create(input),
            stage: Stage::Prepared,
        };
        self.save_journal(&journal)?;
        let journal = self.dispatch(journal).await?;
        self.join(home, journal.account.ok_or(Error::IntegrityFailed)?)
            .await
    }

    /// Bind a provisional workspace whose device the account has approved.
    ///
    /// # Errors
    /// Returns `Error::DeviceNotApproved` until an administrator approves it.
    pub async fn join(&mut self, home: Principal, account: AccountId) -> Result<AccountRef> {
        let state = self.refresh(home, &account).await?;
        state.active(&self.workspace.meta.device_id)?;
        let secret = self.unlock_secret(home, &account).await?;
        let reference = AccountRef {
            id: account,
            home,
            issuer: state.info.issuer.clone(),
        };
        let keys = self.keys.take().ok_or(Error::Locked)?;
        let bound = self.workspace.bind(&keys, reference.clone(), &secret);
        self.keys = Some(keys);
        bound?;
        self.workspace.meta.login_unlocked_at = Some(now_ms());
        self.workspace.save_meta()?;
        self.located = None;
        // A login's own workspace becomes the account's.
        self.adopt(&account)?;
        self.after_login_unlock()?;
        Ok(reference)
    }

    /// The user home's unlock secret of this device.
    ///
    /// # Errors
    /// A revoked or unknown device returns `Error::DeviceNotApproved`.
    pub async fn unlock_secret(
        &self,
        home: Principal,
        account: &AccountId,
    ) -> Result<zeroize::Zeroizing<crate::crypto::Key>> {
        let secret: Result<Hash> = self
            .session()?
            .ic
            .query(
                home,
                "unlock_secret",
                (account, self.workspace.meta.device_id),
            )
            .await?;
        Ok(zeroize::Zeroizing::new(*secret?))
    }

    /// Apply a device-approved mutation with this administrator device.
    /// `fixed` carries the request ID and version a pairing request bound.
    ///
    /// # Errors
    /// Returns `Error::Pending` while an earlier update is unsettled and
    /// `Error::Forbidden` for a device without administrator rights.
    pub async fn mutate(
        &mut self,
        command: AccountCommand,
        fixed: Option<(Hash, u64)>,
    ) -> Result<AccountState> {
        ensure(self.pending()?.is_none(), Error::Pending)?;
        let account = self.account()?.clone();
        let state = self.refresh(account.home, &account.id).await?;
        let device = state.active(&self.workspace.meta.device_id)?;
        ensure(
            matches!(command, AccountCommand::DisputeRecovery { .. })
                || (device.input.role == ControllerRole::Administrator
                    && device
                        .input
                        .capabilities
                        .contains(&dmsg_types::user::Capability::RootManage)),
            Error::Forbidden,
        )?;
        let ttl = match command {
            AccountCommand::AuthorizeHandle { .. } => HANDLE_APPROVAL_MS,
            _ => APPROVAL_MS,
        };
        let (request_id, version) =
            fixed.unwrap_or((Hash::new(random_id()), state.info.account_version));
        ensure(
            version == state.info.account_version,
            Error::VersionConflict,
        )?;
        let mut mutation = AccountMutation {
            account_id: account.id,
            expected_version: version,
            command,
            approval: Approval {
                device_id: self.workspace.meta.device_id,
                security_epoch: state.info.security_epoch,
                sequence: device.next_sequence,
                request_id,
                expires_at: now_ms() + ttl,
                signature: Ed25519Signature::new([0; 64]),
            },
        };
        let message = approval_message(
            account.home,
            &account.id,
            "dmsg/account/v2",
            &(mutation.expected_version, &mutation.command),
            &mutation.approval,
        );
        mutation.approval.signature =
            Ed25519Signature::new(sign(&self.keys()?.signing, message.as_slice()));
        let journal = Journal {
            home: account.home,
            caller: self.session()?.principal,
            account: Some(account.id),
            request: Request::Mutate(mutation),
            stage: Stage::Prepared,
        };
        self.save_journal(&journal)?;
        self.dispatch(journal).await?;
        self.refresh(account.home, &account.id).await
    }

    /// A pairing request for the login's account, signed by this provisional
    /// device: a base64url token an administrator device approves.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when the login has no account.
    pub async fn pair_request(&mut self, role: ControllerRole) -> Result<String> {
        ensure(self.workspace.provisional(), Error::Forbidden)?;
        let (home, account) = self.locate().await?.ok_or(Error::NotFound)?;
        let state = self.refresh(home, &account).await?;
        ensure(
            state.device(&self.workspace.meta.device_id).is_none(),
            Error::IdempotencyConflict,
        )?;
        let device = self.workspace.device_input(role);
        let request_id = Hash::new(random_id());
        let message = digest(
            "dmsg/add-device/v1",
            &(
                home,
                &account,
                &device,
                state.info.account_version,
                request_id,
            ),
        );
        let proof = Ed25519Signature::new(sign(&self.keys()?.signing, message.as_slice()));
        let mutation = AccountMutation {
            account_id: account,
            expected_version: state.info.account_version,
            command: AccountCommand::AddDevice { device, proof },
            approval: Approval {
                device_id: self.workspace.meta.device_id,
                security_epoch: state.info.security_epoch,
                sequence: 0,
                request_id,
                expires_at: now_ms() + APPROVAL_MS,
                signature: Ed25519Signature::new([0; 64]),
            },
        };
        self.located = Some((home, account));
        Ok(b64(&canonical(&Pairing {
            format: PAIRING.into(),
            home,
            request: mutation,
        })))
    }

    /// Check a pairing token for this administrator's account.
    ///
    /// # Errors
    /// A token for another home or account, an expired request or a bad
    /// proof of possession is refused.
    pub fn inspect_pairing(&self, token: &str) -> Result<PairingRequest> {
        ensure(token.len() <= 4096, Error::QuotaExceeded)?;
        let packet: Pairing = decode(&unb64(token)?, 4096)?;
        let account = self.account()?;
        let request = packet.request;
        ensure(
            packet.format == PAIRING
                && packet.home == account.home
                && request.account_id == account.id,
            Error::IntegrityFailed,
        )?;
        let now = now_ms();
        ensure(
            request.approval.expires_at > now && request.approval.expires_at <= now + 300_000,
            Error::Expired,
        )?;
        let AccountCommand::AddDevice { device, proof } = &request.command else {
            return Err(Error::IntegrityFailed);
        };
        let message = digest(
            "dmsg/add-device/v1",
            &(
                packet.home,
                &request.account_id,
                device,
                request.expected_version,
                request.approval.request_id,
            ),
        );
        verify(&device.signing_pub, message.as_slice(), proof.as_slice())?;
        Ok(PairingRequest {
            account: request.account_id,
            device: device.clone(),
            mutation: request,
        })
    }

    /// Approve a pairing request: enroll the device with the request ID and
    /// version it signed. The caller then rotates the root to include it.
    ///
    /// # Errors
    /// As [`Self::mutate`]; a changed account returns `Error::VersionConflict`
    /// and the new device must ask again.
    pub async fn approve_pairing(&mut self, request: &PairingRequest) -> Result<AccountState> {
        self.mutate(
            request.mutation.command.clone(),
            Some((
                request.mutation.approval.request_id,
                request.mutation.expected_version,
            )),
        )
        .await
    }
}

/// The account's current state, fetched with the login's agent `ic` and
/// verified; [`Client::refresh`] keeps it.
///
/// # Errors
/// As [`verify_account`].
pub(crate) async fn read_account(
    ic: &Ic,
    home: Principal,
    account: &AccountId,
) -> Result<AccountState> {
    // Boxed: the queries side by side would otherwise sit inside every
    // future that refreshes, on the runtime's small stacks.
    let (info, batch, bundle) = Box::pin(join3(
        ic.query::<_, Result<AccountInfo>>(home, "get_account", (account,)),
        security_batch(ic, home, account),
        device_bundle(ic, home, account),
    ))
    .await;
    verify_account(
        home,
        account,
        info??,
        &batch??,
        bundle??.1,
        &ic.root_key(),
        now_ms(),
    )
}

/// [`Client::refresh`] of the selected account, its user home waited on
/// without the client.
///
/// # Errors
/// Returns `Error::AuthRequired` without a login, and as [`Client::refresh`].
pub(crate) async fn refreshed<H: Hold>(h: &H) -> Result<AccountState> {
    let (ic, account) = h
        .with(|c| Ok((c.session()?.ic.clone(), c.account()?.clone())))
        .await?;
    let state = read_account(&ic, account.home, &account.id).await?;
    h.with(|c| c.observe(&account.id, &state.security)).await?;
    Ok(state)
}

/// [`Client::refresh_security`] of the selected account, its user home
/// waited on without the client.
///
/// # Errors
/// As [`Client::refresh_security`].
pub(crate) async fn refreshed_security<H: Hold>(h: &H) -> Result<Security> {
    let account = h.with(|c| c.account().cloned()).await?;
    let ic = h.reader().await?;
    let security = fetch_security(&ic, account.home, &account.id).await?;
    h.with(|c| c.observe(&account.id, &security)).await?;
    Ok(security)
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
