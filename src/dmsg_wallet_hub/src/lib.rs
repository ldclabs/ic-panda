//! dMsg wallet hub: quotes and creates paid team wallets, keeps the wallet
//! modules governance registered, installs the version a wallet accepted,
//! removes its controllers when it locks its code, and indexes members.
//! It never moves a wallet's funds or decides for it.
mod store;

use candid::{Nat, Principal};
use dmsg_protocol::{
    agent::{account_home, check_user_home, validate_user_homes},
    authenticated, digest, sha256, validate_namespace,
    wallet::*,
};
use dmsg_runtime::admin::{self, hex, validation, Validation};
use dmsg_types::{
    billing::PlanId,
    integration::{ApplicationAuthorization, ApprovalPurpose},
    membership::Beneficiary,
    wallet::*,
    *,
};
use ic_cdk_management_canister::{
    canister_status, clear_chunk_store, create_canister_with_extra_cycles, install_chunked_code,
    start_canister, stop_canister, update_settings, upload_chunk, CanisterIdRecord,
    CanisterInstallMode, CanisterSettings, CanisterStatusType, ChunkHash, CreateCanisterArgs,
    InstallChunkedCodeArgs, LogVisibility, UpdateSettingsArgs, UploadChunkArgs,
};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{Memo, TransferArg, TransferError},
};
use std::{cell::RefCell, collections::BTreeSet};
use store::{Hub, Payout, PayoutTransfer};

/// A wallet stays frozen, not deleted, for a year without cycles.
const FREEZING_THRESHOLD_SECONDS: u64 = 365 * 24 * 60 * 60;
/// The cycles minting canister's top-up memo, as ICRC-1 memo bytes.
const MEMO_TOP_UP: u64 = 0x5055_5054;
const CHUNK: usize = 1024 * 1024;

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn check_admin(caller: Principal) -> Result<()> {
    admin::check_admin(caller, store::with(|h| h.init.governance))
}

thread_local! {
    /// Activations and payouts with a call in flight; heap only, so an
    /// upgrade clears it.
    static BUSY: RefCell<BTreeSet<Hash>> = const { RefCell::new(BTreeSet::new()) };
}

/// Holds an activation or payout until dropped.
struct Busy(Hash);

impl Busy {
    fn take(id: Hash) -> Result<Self> {
        BUSY.with_borrow_mut(|b| b.insert(id))
            .then_some(Self(id))
            .ok_or(Error::Pending)
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.with_borrow_mut(|b| b.remove(&self.0));
    }
}

fn check_pricing(p: &HubPricing) -> Result<()> {
    ensure_valid(
        p.price_e8s > p.cycles_share_e8s
            && p.cycles_to_wallet >= 1_000_000_000_000
            && p.quotas.len() <= 4,
        "pricing",
    )
}

#[ic_cdk::init]
fn init(init: HubInit) {
    validate_namespace(&init.issuer_namespace).expect("issuer namespace");
    validate_user_homes(&init.environment, &init.issuer_namespace, &init.user_homes)
        .expect("user homes");
    for p in [
        init.governance,
        init.icp_ledger,
        init.cmc,
        init.commerce,
        init.revenue.owner,
    ] {
        authenticated(p).expect("principal");
    }
    check_pricing(&init.pricing).expect("pricing");
    store::save(Hub {
        init,
        versions: vec![],
        current: None,
        uploader: None,
        quotes_issued: 0,
    });
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    store::with(|_| ());
}

#[ic_cdk::query]
fn get_hub_config() -> HubInit {
    store::with(|h| h.init.clone())
}

#[ic_cdk::query]
fn get_user_homes() -> Vec<Principal> {
    store::with(|h| h.init.user_homes.clone())
}

#[ic_cdk::query]
fn list_versions() -> Vec<WalletVersion> {
    store::with(|h| h.versions.clone())
}

#[ic_cdk::query]
fn hub_stats() -> HubStats {
    store::with(|h| HubStats {
        wallets: store::wallet_count(),
        orders: store::order_count(),
        versions: h.versions.len() as u32,
        current_version: h.current,
        cycles: ic_cdk::api::canister_cycle_balance(),
    })
}

#[ic_cdk::query]
fn get_quote(id: Hash) -> Option<ActivationQuote> {
    store::quote(&id)
}

#[ic_cdk::query]
fn get_order(id: Hash) -> Option<ActivationOrder> {
    store::order(&id)
}

#[ic_cdk::query]
fn get_wallet_record(wallet: Principal) -> Option<WalletRecord> {
    store::wallet(wallet)
}

#[ic_cdk::query]
fn list_wallets_of(member: MemberId) -> Vec<WalletRecord> {
    store::wallets_of(&member)
}

fn plan_quota(h: &Hub, plan: &PlanId) -> u32 {
    h.init
        .pricing
        .quotas
        .iter()
        .find(|q| q.plan == *plan)
        .map_or(0, |q| q.wallets)
}

/// The account's base plan from commerce; an account that never bought
/// anything is on Free.
async fn plan_of(h: &Hub, account: &AccountId) -> Result<PlanId> {
    let home = account_home(
        &h.init.environment,
        &h.init.issuer_namespace,
        &h.init.user_homes,
        account,
    )
    .ok_or(Error::NotFound)?;
    let beneficiary = Beneficiary {
        product_id: "dmsg".into(),
        authority_canister: home,
        subject_schema: "dmsg-account-v1".into(),
        subject_bytes: account.as_slice().to_vec().into(),
    };
    let reply: Result<dmsg_types::billing::EntitlementView> =
        dmsg_runtime::call(h.init.commerce, "refresh_entitlement", (beneficiary,)).await?;
    match reply {
        Ok(view) => Ok(view.plan_snapshot.plan_id),
        Err(Error::NotFound) => Ok(PlanId::Free),
        Err(e) => Err(e),
    }
}

/// Price and deposit account for the calling payer to create a wallet for
/// `account_id`, within the account's plan quota.
#[ic_cdk::update]
async fn quote_activation(account_id: AccountId) -> Result<ActivationQuote> {
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    let h = store::hub();
    ensure(!h.init.pricing.paused, Error::Locked)?;
    ensure(
        h.current.is_some(),
        Error::Unavailable("no wallet version".into()),
    )?;
    let plan = plan_of(&h, &account_id).await?;
    let fee: Nat = dmsg_runtime::call(h.init.icp_ledger, "icrc1_fee", ()).await?;
    // Pricing, versions and quota use are reread after the calls.
    let at = now();
    let mut h = store::hub();
    ensure(!h.init.pricing.paused, Error::Locked)?;
    let version = h
        .current
        .ok_or(Error::Unavailable("no wallet version".into()))?;
    let quota = plan_quota(&h, &plan);
    let used = store::quota_used(&account_id);
    ensure(used < quota, Error::QuotaExceeded)?;
    let me = ic_cdk::api::canister_self();
    h.quotes_issued += 1;
    let quote_id = digest(
        "dmsg-wallet/quote/v1",
        &(me, &account_id, caller, h.quotes_issued, at),
    );
    let quote = ActivationQuote {
        quote_id,
        account_id,
        payer: caller,
        plan,
        remaining: quota - used,
        price_e8s: h.init.pricing.price_e8s,
        ledger_fee_e8s: dmsg_runtime::ledger::token_amount(fee)?
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?,
        cycles_to_wallet: h.init.pricing.cycles_to_wallet,
        deposit: Account {
            owner: me,
            subaccount: Some(deposit_subaccount(me, &quote_id).into_array()),
        },
        version,
        issued_at_ms: at,
        expires_at_ms: at + QUOTE_TTL_MS,
    };
    store::save_quote(&quote);
    store::save(h);
    Ok(quote)
}

async fn balance(ledger: Principal, account: Account) -> Result<u128> {
    let n: Nat = dmsg_runtime::call(ledger, "icrc1_balance_of", (account,)).await?;
    dmsg_runtime::ledger::token_amount(n)
}

/// Create the wallet of a paid quote. The creator's device approval names
/// the quote and the exact wallet; the payment must be in the deposit.
#[ic_cdk::update]
async fn create_wallet(req: CreateWallet) -> Result<ActivationOrder> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    let quote = store::quote(&req.quote_id).ok_or(Error::NotFound)?;
    ensure(quote.payer == caller, Error::Forbidden)?;
    if let Some(order) = store::order(&req.quote_id) {
        ensure(order.spec == req.spec, Error::IdempotencyConflict)?;
        return Ok(order);
    }
    ensure(at < quote.expires_at_ms, Error::Expired)?;
    ensure(store::payout(&req.quote_id).is_none(), Error::Forbidden)?;
    let creator = MemberId::Account(quote.account_id);
    validate_spec(&req.spec, &creator)?;
    let me = ic_cdk::api::canister_self();
    let h = store::hub();
    check_wallet_approval(
        &req.approval,
        &ExpectedApproval {
            environment: &h.init.environment,
            hub: me,
            service: me,
            purpose: ApprovalPurpose::WalletCreate,
            action_digest: activation_digest(&quote, &req.spec),
            account: &quote.account_id,
            actor: caller,
        },
        at,
    )?;
    let home = account_home(
        &h.init.environment,
        &h.init.issuer_namespace,
        &h.init.user_homes,
        &quote.account_id,
    )
    .ok_or(Error::NotFound)?;
    let _busy = Busy::take(req.quote_id)?;
    let verified: Result<ApplicationAuthorization> = dmsg_runtime::call(
        home,
        "verify_application_authorization",
        (req.approval_id, req.approval.clone()),
    )
    .await?;
    verified?;
    let paid = balance(h.init.icp_ledger, quote.deposit).await?;
    ensure(paid >= u128::from(quote.price_e8s), Error::FeeBlocked)?;
    // The quote may have been used meanwhile; quota is rechecked too.
    let at = now();
    if let Some(order) = store::order(&req.quote_id) {
        ensure(order.spec == req.spec, Error::IdempotencyConflict)?;
        return Ok(order);
    }
    ensure(store::payout(&req.quote_id).is_none(), Error::Forbidden)?;
    let h = store::hub();
    let quota = plan_quota(&h, &quote.plan);
    ensure(
        store::quota_used(&quote.account_id) < quota,
        Error::QuotaExceeded,
    )?;
    let order = ActivationOrder {
        quote,
        spec: req.spec,
        status: ActivationStatus::Paid,
        wallet: None,
        collected: false,
        updated_at_ms: at,
    };
    store::hold_quota(&order.quote.account_id, &order.quote.quote_id);
    store::save_order(&order);
    drop(_busy);
    activate(order.quote.quote_id).await
}

/// Continue an activation after an interrupted creation or install.
#[ic_cdk::update]
async fn reconcile_activation(id: Hash) -> Result<ActivationOrder> {
    activate(id).await
}

fn wallet_init(h: &Hub, order: &ActivationOrder) -> WalletInit {
    WalletInit {
        environment: h.init.environment.clone(),
        hub: ic_cdk::api::canister_self(),
        key_name: h.init.key_name.clone(),
        btc_network: h.init.btc_network,
        icp_ledger: h.init.icp_ledger,
        cmc: h.init.cmc,
        issuer_namespace: h.init.issuer_namespace.clone(),
        user_homes: h.init.user_homes.clone(),
        name: order.spec.name.clone(),
        creator: MemberId::Account(order.quote.account_id),
        creator_principal: order.quote.payer,
        members: order.spec.members.clone(),
        policy: order.spec.policy.clone(),
        channel_hint: order.spec.channel_hint.clone(),
    }
}

fn unavailable(e: impl std::fmt::Display) -> Error {
    Error::Unavailable(e.to_string())
}

/// Install `module` on `target` through its chunk store.
async fn install(
    target: Principal,
    module: &[u8],
    mode: CanisterInstallMode,
    arg: Vec<u8>,
) -> Result<()> {
    clear_chunk_store(&CanisterIdRecord {
        canister_id: target,
    })
    .await
    .map_err(unavailable)?;
    let mut hashes = vec![];
    for chunk in module.chunks(CHUNK) {
        let hash = upload_chunk(&UploadChunkArgs {
            canister_id: target,
            chunk: chunk.to_vec(),
        })
        .await
        .map_err(unavailable)?;
        hashes.push(ChunkHash { hash: hash.hash });
    }
    let installed = install_chunked_code(&InstallChunkedCodeArgs {
        mode,
        target_canister: target,
        store_canister: None,
        chunk_hashes_list: hashes,
        wasm_module_hash: sha256(module).to_vec(),
        arg,
    })
    .await
    .map_err(unavailable);
    // The chunks would otherwise stay in the wallet's memory.
    let _ = clear_chunk_store(&CanisterIdRecord {
        canister_id: target,
    })
    .await;
    installed
}

async fn activate(id: Hash) -> Result<ActivationOrder> {
    let _busy = Busy::take(id)?;
    loop {
        let mut order = store::order(&id).ok_or(Error::NotFound)?;
        match order.status {
            ActivationStatus::Paid => {
                let cycles = order.quote.cycles_to_wallet;
                let created = create_canister_with_extra_cycles(
                    &CreateCanisterArgs {
                        settings: Some(CanisterSettings {
                            controllers: Some(vec![ic_cdk::api::canister_self()]),
                            freezing_threshold: Some(Nat::from(FREEZING_THRESHOLD_SECONDS)),
                            log_visibility: Some(LogVisibility::Public),
                            ..Default::default()
                        }),
                    },
                    cycles,
                )
                .await;
                let at = now();
                let mut order = store::order(&id).ok_or(Error::NotFound)?;
                match created {
                    Ok(record) => {
                        order.wallet = Some(record.canister_id);
                        order.status = ActivationStatus::Created;
                    }
                    Err(e) => {
                        order.status = ActivationStatus::Failed(e.to_string());
                        store::release_quota(&order.quote.account_id, &id);
                    }
                }
                order.updated_at_ms = at;
                store::save_order(&order);
            }
            ActivationStatus::Created => {
                let h = store::hub();
                let wallet = order.wallet.expect("created");
                let version = order.quote.version;
                let module = store::module(version).ok_or(Error::NotFound)?;
                let arg = candid::encode_one(wallet_init(&h, &order)).map_err(unavailable)?;
                install(wallet, &module, CanisterInstallMode::Install, arg).await?;
                let at = now();
                order = store::order(&id).ok_or(Error::NotFound)?;
                order.status = ActivationStatus::Ready;
                order.updated_at_ms = at;
                let mut members = vec![MemberId::Account(order.quote.account_id)];
                members.extend(order.spec.members.iter().map(|m| m.id.clone()));
                store::save_wallet(
                    None,
                    &WalletRecord {
                        wallet,
                        creator: order.quote.account_id,
                        name: order.spec.name.clone(),
                        version,
                        locked: false,
                        members,
                        created_at_ms: at,
                    },
                );
                store::save_order(&order);
                return Ok(order);
            }
            _ => return Ok(order),
        }
    }
}

fn transfer_arg(from: &Hash, t: &PayoutTransfer, created_at_time: u64) -> TransferArg {
    TransferArg {
        from_subaccount: Some(deposit_subaccount(ic_cdk::api::canister_self(), from).into_array()),
        to: t.to,
        fee: None,
        created_at_time: Some(created_at_time),
        memo: t.memo.clone().map(Memo),
        amount: Nat::from(t.amount),
    }
}

/// Send a payout's transfers in order, then notify the CMC of a top-up.
async fn pay_out(id: Hash) -> Result<Payout> {
    let _busy = Busy::take(id)?;
    let (ledger, cmc) = store::with(|h| (h.init.icp_ledger, h.init.cmc));
    let me = ic_cdk::api::canister_self();
    loop {
        let mut payout = store::payout(&id).ok_or(Error::NotFound)?;
        let Some(i) = payout.transfers.iter().position(|t| t.block.is_none()) else {
            if !payout.refund && payout.cycles.is_none() {
                let block = payout.transfers[0].block.expect("sent");
                let reply: std::result::Result<std::result::Result<Nat, candid::Reserved>, _> =
                    dmsg_runtime::call_classified(
                        cmc,
                        "notify_top_up",
                        (NotifyTopUp {
                            block_index: block,
                            canister_id: me,
                        },),
                    )
                    .await;
                let Ok(Ok(cycles)) = reply else {
                    return Err(Error::Pending);
                };
                payout = store::payout(&id).ok_or(Error::NotFound)?;
                payout.cycles = Some(dmsg_runtime::ledger::token_amount(cycles)?);
                store::save_payout(&id, &payout);
            }
            return Ok(payout);
        };
        let arg = transfer_arg(&id, &payout.transfers[i], payout.created_at_time);
        let reply: std::result::Result<std::result::Result<Nat, TransferError>, _> =
            dmsg_runtime::call_classified(ledger, "icrc1_transfer", (arg,)).await;
        let block = match reply {
            Ok(Ok(n)) | Ok(Err(TransferError::Duplicate { duplicate_of: n })) => {
                dmsg_runtime::ledger::block_index(n)?
            }
            Ok(Err(e)) => return Err(Error::InvalidInput(format!("{e:?}"))),
            Err(_) => return Err(Error::ExecutionUnknown),
        };
        payout = store::payout(&id).ok_or(Error::NotFound)?;
        payout.transfers[i].block = Some(block);
        store::save_payout(&id, &payout);
    }
}

#[derive(candid::CandidType, serde::Deserialize)]
struct NotifyTopUp {
    block_index: u64,
    canister_id: Principal,
}

fn cmc_subaccount(canister: Principal) -> [u8; 32] {
    let bytes = canister.as_slice();
    let mut sub = [0; 32];
    sub[0] = bytes.len() as u8;
    sub[1..=bytes.len()].copy_from_slice(bytes);
    sub
}

/// Return the deposit of a failed activation, or of a quote that expired
/// without one, to its payer less the ledger fee; returns the ledger block.
#[ic_cdk::update]
async fn claim_activation_refund(id: Hash) -> Result<u64> {
    let caller = ic_cdk::api::msg_caller();
    let quote = store::quote(&id).ok_or(Error::NotFound)?;
    ensure(quote.payer == caller, Error::Forbidden)?;
    let refundable = |at: u64| match store::order(&id) {
        Some(o) => matches!(o.status, ActivationStatus::Failed(_)),
        None => at >= quote.expires_at_ms,
    };
    if store::payout(&id).is_none() {
        ensure(refundable(now()), Error::Forbidden)?;
        let ledger = store::with(|h| h.init.icp_ledger);
        let paid = balance(ledger, quote.deposit).await?;
        let fee = u128::from(quote.ledger_fee_e8s);
        ensure(paid > fee, Error::NotFound)?;
        // An order may have started during the balance read.
        if store::payout(&id).is_none() {
            ensure(refundable(now()), Error::Forbidden)?;
            store::save_payout(
                &id,
                &Payout {
                    refund: true,
                    created_at_time: ic_cdk::api::time(),
                    transfers: vec![PayoutTransfer {
                        to: Account {
                            owner: quote.payer,
                            subaccount: None,
                        },
                        amount: paid - fee,
                        memo: None,
                        block: None,
                    }],
                    cycles: None,
                },
            );
        }
    }
    let payout = pay_out(id).await?;
    ensure(payout.refund, Error::Forbidden)?;
    let block = payout.transfers[0].block.expect("sent");
    if let Some(mut order) = store::order(&id) {
        order.status = ActivationStatus::Refunded { block };
        order.updated_at_ms = now();
        store::save_order(&order);
    }
    Ok(block)
}

/// Move a ready activation's payment: the cycles share to the CMC for the
/// hub's cycles, the rest to the revenue account. Anyone may call it.
#[ic_cdk::update]
async fn collect_revenue(id: Hash) -> Result<ActivationOrder> {
    let order = store::order(&id).ok_or(Error::NotFound)?;
    ensure(order.status == ActivationStatus::Ready, Error::Forbidden)?;
    if order.collected {
        return Ok(order);
    }
    if store::payout(&id).is_none() {
        let h = store::hub();
        let paid = balance(h.init.icp_ledger, order.quote.deposit).await?;
        if store::payout(&id).is_none() {
            let fee = u128::from(order.quote.ledger_fee_e8s);
            let share = u128::from(h.init.pricing.cycles_share_e8s);
            ensure(paid > share + 2 * fee, Error::FeeBlocked)?;
            store::save_payout(
                &id,
                &Payout {
                    refund: false,
                    created_at_time: ic_cdk::api::time(),
                    transfers: vec![
                        PayoutTransfer {
                            to: Account {
                                owner: h.init.cmc,
                                subaccount: Some(cmc_subaccount(ic_cdk::api::canister_self())),
                            },
                            amount: share,
                            memo: Some(MEMO_TOP_UP.to_le_bytes().to_vec().into()),
                            block: None,
                        },
                        PayoutTransfer {
                            to: h.init.revenue,
                            amount: paid - share - 2 * fee,
                            memo: None,
                            block: None,
                        },
                    ],
                    cycles: None,
                },
            );
        }
    }
    pay_out(id).await?;
    let mut order = store::order(&id).ok_or(Error::NotFound)?;
    order.collected = true;
    order.updated_at_ms = now();
    store::save_order(&order);
    Ok(order)
}

/// Install the version a wallet accepted. Anyone may call it.
#[ic_cdk::update]
async fn upgrade_wallet(wallet: Principal) -> Result<WalletRecord> {
    let record = store::wallet(wallet).ok_or(Error::NotFound)?;
    ensure(!record.locked, Error::Locked)?;
    let _busy = Busy::take(sha256(wallet.as_slice()))?;
    let accepted: CodeAcceptance = dmsg_runtime::call(wallet, "accepted_upgrade", ()).await?;
    let CodeAcceptance::Upgrade {
        version,
        module_hash,
    } = accepted
    else {
        return Err(Error::NotFound);
    };
    let h = store::hub();
    let registered = h
        .versions
        .iter()
        .find(|v| v.version == version)
        .ok_or(Error::NotFound)?;
    ensure(
        registered.module_hash == module_hash,
        Error::IntegrityFailed,
    )?;
    ensure(
        store::wallet(wallet).is_some_and(|r| r.version != version),
        Error::IdempotencyConflict,
    )?;
    let module = store::module(version).ok_or(Error::NotFound)?;
    // The wallet has no calls to the hub in flight, so stopping it ends.
    stop_canister(&CanisterIdRecord {
        canister_id: wallet,
    })
    .await
    .map_err(unavailable)?;
    let arg = candid::encode_one(Some(version)).map_err(unavailable)?;
    let installed = install(wallet, &module, CanisterInstallMode::Upgrade(None), arg).await;
    start_canister(&CanisterIdRecord {
        canister_id: wallet,
    })
    .await
    .map_err(unavailable)?;
    installed?;
    let mut record = store::wallet(wallet).ok_or(Error::NotFound)?;
    let old = record.clone();
    record.version = version;
    store::save_wallet(Some(&old), &record);
    Ok(record)
}

/// Remove every controller of a wallet that accepted locking its code.
#[ic_cdk::update]
async fn lock_wallet(wallet: Principal) -> Result<WalletRecord> {
    let record = store::wallet(wallet).ok_or(Error::NotFound)?;
    if record.locked {
        return Ok(record);
    }
    let accepted: CodeAcceptance = dmsg_runtime::call(wallet, "accepted_upgrade", ()).await?;
    ensure(accepted == CodeAcceptance::Lock, Error::Forbidden)?;
    update_settings(&UpdateSettingsArgs {
        canister_id: wallet,
        settings: CanisterSettings {
            controllers: Some(vec![]),
            ..Default::default()
        },
    })
    .await
    .map_err(unavailable)?;
    let mut record = store::wallet(wallet).ok_or(Error::NotFound)?;
    let old = record.clone();
    record.locked = true;
    store::save_wallet(Some(&old), &record);
    Ok(record)
}

/// Ask a wallet to top itself up under its refuel rule.
#[ic_cdk::update]
async fn refuel_wallet(wallet: Principal) -> Result<Option<u128>> {
    store::wallet(wallet).ok_or(Error::NotFound)?;
    let reply: Result<Option<u128>> = dmsg_runtime::call(wallet, "refuel", ()).await?;
    reply
}

#[ic_cdk::update]
async fn wallet_health(wallet: Principal) -> Result<WalletHealth> {
    let record = store::wallet(wallet).ok_or(Error::NotFound)?;
    ensure(!record.locked, Error::Locked)?;
    let s = canister_status(&CanisterIdRecord {
        canister_id: wallet,
    })
    .await
    .map_err(unavailable)?;
    let n = |n: Nat| dmsg_runtime::ledger::token_amount(n).unwrap_or(u128::MAX);
    Ok(WalletHealth {
        cycles: n(s.cycles),
        memory_bytes: n(s.memory_size),
        idle_cycles_per_day: n(s.idle_cycles_burned_per_day),
        running: s.status == CanisterStatusType::Running,
    })
}

/// Refresh the member index from the wallet. Anyone may call it.
#[ic_cdk::update]
async fn sync_wallet(wallet: Principal) -> Result<WalletRecord> {
    store::wallet(wallet).ok_or(Error::NotFound)?;
    let members: Result<Vec<MemberId>> = dmsg_runtime::call(wallet, "member_ids", ()).await?;
    let members = members?;
    ensure(members.len() <= MAX_MEMBERS, Error::IntegrityFailed)?;
    let mut record = store::wallet(wallet).ok_or(Error::NotFound)?;
    let old = record.clone();
    record.members = members;
    store::save_wallet(Some(&old), &record);
    Ok(record)
}

fn check_uploader(caller: Principal) -> Result<()> {
    if store::with(|h| h.uploader == Some(caller)) {
        return Ok(());
    }
    check_admin(caller)
}

/// Append bytes to the module staged for registration.
#[ic_cdk::update]
fn upload_wasm_chunk(chunk: serde_bytes::ByteBuf) -> Result<StagedModule> {
    check_uploader(ic_cdk::api::msg_caller())?;
    ensure(
        !chunk.is_empty() && store::staged_size() + chunk.len() <= store::MAX_MODULE,
        Error::QuotaExceeded,
    )?;
    store::stage(chunk.into_vec());
    Ok(staged_module())
}

#[ic_cdk::update]
fn clear_wasm_staging() -> Result<()> {
    check_uploader(ic_cdk::api::msg_caller())?;
    store::clear_staging();
    Ok(())
}

fn staged_module() -> StagedModule {
    let bytes = store::staged();
    StagedModule {
        size: bytes.len() as u64,
        module_hash: sha256(&bytes),
    }
}

#[ic_cdk::query]
fn staged_wasm() -> StagedModule {
    staged_module()
}

fn check_register(h: &Hub, module_hash: &Hash, notes: &str) -> Result<()> {
    ensure(h.versions.len() < store::MAX_VERSIONS, Error::QuotaExceeded)?;
    ensure_valid(notes.len() <= 1_024, "notes")?;
    ensure(
        staged_module().module_hash == *module_hash && store::staged_size() > 0,
        Error::IntegrityFailed,
    )?;
    ensure(
        h.versions.iter().all(|v| v.module_hash != *module_hash),
        Error::IdempotencyConflict,
    )
}

/// Register the staged module as the next version new wallets get.
#[ic_cdk::update]
fn admin_register_version(module_hash: Hash, notes: String) -> Result<WalletVersion> {
    check_admin(ic_cdk::api::msg_caller())?;
    let at = now();
    let mut h = store::hub();
    check_register(&h, &module_hash, &notes)?;
    let bytes = store::staged();
    let version = h.versions.last().map_or(1, |v| v.version + 1);
    let v = WalletVersion {
        version,
        module_hash,
        size: bytes.len() as u64,
        notes,
        registered_at_ms: at,
        retired: false,
    };
    store::save_module(version, bytes);
    store::clear_staging();
    h.versions.push(v.clone());
    h.current = Some(version);
    store::save(h);
    Ok(v)
}

#[ic_cdk::query]
fn validate_admin_register_version(module_hash: Hash, notes: String) -> Validation {
    validation(store::with(|h| {
        check_register(h, &module_hash, &notes).map(|()| {
            format!(
                "Register wallet module {} ({} bytes) as version {}: {notes}",
                hex(&module_hash[..]),
                store::staged_size(),
                h.versions.last().map_or(1, |v| v.version + 1),
            )
        })
    }))
}

/// Stop giving a version to new wallets; installed wallets keep it.
#[ic_cdk::update]
fn admin_retire_version(version: u32) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    let mut h = store::hub();
    let v = h
        .versions
        .iter_mut()
        .find(|v| v.version == version)
        .ok_or(Error::NotFound)?;
    v.retired = true;
    h.current = h
        .versions
        .iter()
        .rev()
        .find(|v| !v.retired)
        .map(|v| v.version);
    store::save(h);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_retire_version(version: u32) -> Validation {
    validation(store::with(|h| {
        h.versions
            .iter()
            .find(|v| v.version == version)
            .ok_or(Error::NotFound)
            .map(|v| {
                format!(
                    "Retire wallet version {version} ({}).",
                    hex(&v.module_hash[..])
                )
            })
    }))
}

#[ic_cdk::update]
fn admin_set_pricing(pricing: HubPricing) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    check_pricing(&pricing)?;
    let mut h = store::hub();
    h.init.pricing = pricing;
    store::save(h);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_pricing(pricing: HubPricing) -> Validation {
    validation(check_pricing(&pricing).map(|()| {
        format!(
            "Set wallet activation to {} e8s ICP, {} e8s of it converted to hub cycles, {} cycles per wallet, quotas {:?}, paused {}.",
            pricing.price_e8s, pricing.cycles_share_e8s, pricing.cycles_to_wallet, pricing.quotas, pricing.paused
        )
    }))
}

#[ic_cdk::update]
fn admin_set_revenue(account: Account) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    authenticated(account.owner)?;
    let mut h = store::hub();
    h.init.revenue = account;
    store::save(h);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_revenue(account: Account) -> Validation {
    validation(
        authenticated(account.owner)
            .map(|()| format!("Send wallet activation revenue to {account}.")),
    )
}

#[ic_cdk::update]
fn admin_set_uploader(uploader: Option<Principal>) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    let mut h = store::hub();
    h.uploader = uploader;
    store::save(h);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_uploader(uploader: Option<Principal>) -> Validation {
    Ok(match uploader {
        Some(p) => format!("Let {p} stage wallet modules for registration."),
        None => "Stop module staging by anyone but administrators.".into(),
    })
}

fn check_home(h: &Hub, home: Principal) -> Result<bool> {
    check_user_home(
        &h.init.environment,
        &h.init.issuer_namespace,
        &h.init.user_homes,
        home,
    )
}

#[ic_cdk::update]
fn admin_add_user_home(home: Principal) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    let mut h = store::hub();
    if check_home(&h, home)? {
        h.init.user_homes.push(home);
        store::save(h);
    }
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_add_user_home(home: Principal) -> Validation {
    validation(store::with(|h| {
        Ok(admin::user_home_payload(
            &h.init.environment,
            &h.init.issuer_namespace,
            home,
            check_home(h, home)?,
        ))
    }))
}

/// Admin methods are refused before execution unless the caller may run
/// them; creation needs an authenticated caller.
#[ic_cdk::inspect_message]
fn inspect_message() {
    let caller = ic_cdk::api::msg_caller();
    let method = ic_cdk::api::msg_method_name();
    let allowed = if method.starts_with("admin_") {
        check_admin(caller).is_ok()
    } else if method == "upload_wasm_chunk" || method == "clear_wasm_staging" {
        check_uploader(caller).is_ok()
    } else {
        caller != Principal::anonymous()
    };
    if allowed {
        ic_cdk::api::accept_message();
    }
}

ic_cdk::export_candid!();
