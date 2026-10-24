//! dMsg wallet hub: quotes and creates paid team wallets, keeps the wallet
//! modules governance registered, installs the version a wallet accepted,
//! removes its controllers when it locks its code, and indexes members.
//! It never moves a wallet's funds or decides for it.
mod stable_codec;
mod store;

use candid::{Nat, Principal};
use dmsg_protocol::{
    agent::{account_home, check_user_home, validate_user_homes},
    authenticated, digest, sha256, validate_namespace,
    wallet::*,
};
use dmsg_runtime::{
    admin::{self, Validation, hex, validation},
    ledger::{MEMO_TOP_UP, Notified, Sent, cmc_subaccount, fee, notify_top_up, send},
};
use dmsg_types::{
    billing::PlanId,
    integration::{ApplicationAuthorization, ApprovalPurpose},
    membership::Beneficiary,
    wallet::*,
    *,
};
use ic_cdk_management_canister::{
    CanisterIdRecord, CanisterInstallMode, CanisterSettings, CanisterStatusType, ChunkHash,
    CreateCanisterArgs, InstallChunkedCodeArgs, LogVisibility, UpdateSettingsArgs, UploadChunkArgs,
    canister_status, clear_chunk_store, create_canister_with_extra_cycles, delete_canister,
    install_chunked_code, start_canister, stop_canister, update_settings, upload_chunk,
};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{Memo, TransferArg},
};
use std::{cell::RefCell, collections::BTreeSet};
use store::{Hub, Payout, PayoutTransfer, STABLE_SCHEMA};

/// A wallet stays frozen, not deleted, for a year without cycles.
const FREEZING_THRESHOLD_SECONDS: u64 = 365 * 24 * 60 * 60;
const CHUNK: usize = 1024 * 1024;
/// Most entries one listing returns.
const MAX_PAGE: u16 = 100;

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn check_admin(caller: Principal) -> Result<()> {
    admin::check_admin(caller, store::with(|h| h.init.governance))
}

thread_local! {
    /// Activations, payouts and wallets with a call in flight; heap only, so
    /// an upgrade clears it.
    static BUSY: RefCell<BTreeSet<Hash>> = const { RefCell::new(BTreeSet::new()) };
}

/// Holds an activation, payout or wallet until dropped.
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
    let distinct = p
        .quotas
        .iter()
        .enumerate()
        .all(|(i, q)| p.quotas[..i].iter().all(|x| x.plan != q.plan));
    ensure_valid(
        p.price_e8s > p.cycles_share_e8s
            && p.cycles_share_e8s > 0
            && p.cycles_to_wallet >= 1_000_000_000_000
            && p.quotas.len() <= 4
            && distinct,
        "pricing",
    )
}

/// Revenue goes outside the hub: nothing pays out of the hub's own accounts.
fn check_revenue(account: &Account, me: Principal) -> Result<()> {
    ensure_valid(
        authenticated(account.owner).is_ok() && account.owner != me,
        "revenue account",
    )
}

#[ic_cdk::init]
fn init(init: HubInit) {
    validate_namespace(&init.issuer_namespace).expect("issuer namespace");
    validate_user_homes(&init.environment, &init.issuer_namespace, &init.user_homes)
        .expect("user homes");
    for p in [init.governance, init.icp_ledger, init.cmc, init.commerce] {
        authenticated(p).expect("principal");
    }
    check_pricing(&init.pricing).expect("pricing");
    check_revenue(&init.revenue, ic_cdk::api::canister_self()).expect("revenue account");
    store::save(Hub {
        schema: STABLE_SCHEMA,
        init,
        versions: vec![],
        current: None,
        next_version: 1,
        uploader: None,
    });
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    assert_eq!(
        store::with(|h| h.schema),
        STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    store::initialize_version_uses();
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
        uncollected: store::uncollected_count(),
        versions: h.versions.len() as u32,
        current_version: h.current,
        cycles: ic_cdk::api::canister_cycle_balance(),
    })
}

/// Activation terms this account would get now, without storing a quote: the
/// price, version and remaining wallets by plan. The client reads its plan and
/// the ledger fee itself, so this is a local query and a person sees a plan's
/// limit before asking for a quote.
#[ic_cdk::query]
fn quote_activation_preview(account_id: AccountId) -> Result<ActivationPreview> {
    store::with(|h| {
        ensure(!h.init.pricing.paused, Error::Locked)?;
        let version = h.current.ok_or_else(no_version)?;
        // An account with no home cannot create a wallet.
        home_of(h, &account_id)?;
        let used = store::quota_used(&account_id);
        Ok(ActivationPreview {
            price_e8s: h.init.pricing.price_e8s,
            cycles_to_wallet: h.init.pricing.cycles_to_wallet,
            icp_ledger: h.init.icp_ledger,
            version,
            plans: h
                .init
                .pricing
                .quotas
                .iter()
                .map(|q| PlanAvailability {
                    plan: q.plan.clone(),
                    remaining: q.wallets.saturating_sub(used),
                })
                .collect(),
        })
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

/// Wallet records by canister ID after `after`, at most 100, for
/// maintenance such as cycles top-ups.
#[ic_cdk::query]
fn list_wallets(after: Option<Principal>, take: u16) -> Vec<WalletRecord> {
    store::wallets_after(after, usize::from(take.min(MAX_PAGE)))
}

/// Quote IDs of ready activations whose payment is not yet revenue, after
/// `after`, at most 100. Maintenance collects each with `collect_revenue`.
#[ic_cdk::query]
fn collectable_activations(after: Option<Hash>, take: u16) -> Vec<Hash> {
    store::uncollected(after, usize::from(take.min(MAX_PAGE)))
}

fn plan_quota(h: &Hub, plan: &PlanId) -> u32 {
    h.init
        .pricing
        .quotas
        .iter()
        .find(|q| q.plan == *plan)
        .map_or(0, |q| q.wallets)
}

fn no_version() -> Error {
    Error::Unavailable("no wallet version".into())
}

/// The user home of `account`, named by its ID's fingerprint.
fn home_of(h: &Hub, account: &AccountId) -> Result<Principal> {
    account_home(
        &h.init.environment,
        &h.init.issuer_namespace,
        &h.init.user_homes,
        account,
    )
    .ok_or(Error::NotFound)
}

/// The account's base plan from commerce; an account that never bought
/// anything is on Free.
async fn plan_of(commerce: Principal, home: Principal, account: &AccountId) -> Result<PlanId> {
    let beneficiary = Beneficiary {
        product_id: "dmsg".into(),
        authority_canister: home,
        subject_schema: "dmsg-account-v1".into(),
        subject_bytes: account.as_slice().to_vec().into(),
    };
    let reply: Result<dmsg_types::billing::EntitlementView> =
        dmsg_runtime::call(commerce, "refresh_entitlement", (beneficiary,)).await?;
    match reply {
        Ok(view) => Ok(view.plan_snapshot.plan_id),
        Err(Error::NotFound) => Ok(PlanId::Free),
        Err(e) => Err(e),
    }
}

/// Price and deposit account for the calling payer to create a wallet for
/// `account_id`; `remaining` shows what the account's plan still allows.
#[ic_cdk::update]
async fn quote_activation(account_id: AccountId) -> Result<ActivationQuote> {
    let caller = ic_cdk::api::msg_caller();
    let me = ic_cdk::api::canister_self();
    authenticated(caller)?;
    let (home, commerce, ledger) = store::with(|h| {
        ensure(!h.init.pricing.paused, Error::Locked)?;
        ensure(h.current.is_some(), no_version())?;
        Ok::<_, Error>((home_of(h, &account_id)?, h.init.commerce, h.init.icp_ledger))
    })?;
    // Both reads answer independently; one round of calls, not two.
    let (plan, fee) =
        futures_util::future::join(plan_of(commerce, home, &account_id), fee(ledger)).await;
    let plan = plan?;
    let fee = fee?;
    // Pricing, versions and quota use are reread after the calls.
    let at = now();
    let quote = store::with(|h| {
        let p = &h.init.pricing;
        ensure(!p.paused, Error::Locked)?;
        let version = h.current.ok_or_else(no_version)?;
        // Both transfers of the revenue must stay payable from the price.
        let share = u128::from(p.cycles_share_e8s);
        ensure(
            share > fee && u128::from(p.price_e8s) > share + 2 * fee,
            Error::Unavailable("pricing below the ledger fee".into()),
        )?;
        let quota = plan_quota(h, &plan);
        let used = store::quota_used(&account_id);
        // A plan with no wallet left still gets a quote, so a person reads the
        // limit and the way to change it; `create_wallet` refuses this quote.
        let quote_id = digest(
            "dmsg-wallet/quote/v1",
            &(me, &account_id, caller, store::quote_count(), at),
        );
        Ok::<_, Error>(ActivationQuote {
            quote_id,
            account_id,
            payer: caller,
            plan,
            remaining: quota.saturating_sub(used),
            price_e8s: p.price_e8s,
            ledger_fee_e8s: fee.try_into().map_err(|_| Error::IntegrityFailed)?,
            cycles_to_wallet: p.cycles_to_wallet,
            deposit: Account {
                owner: me,
                subaccount: Some(deposit_subaccount(me, &quote_id).into_array()),
            },
            version,
            issued_at_ms: at,
            expires_at_ms: at + QUOTE_TTL_MS,
        })
    })?;
    store::save_quote(&quote);
    Ok(quote)
}

async fn balance(ledger: Principal, account: Account) -> Result<u128> {
    let n: Nat = dmsg_runtime::call(ledger, "icrc1_balance_of", (account,)).await?;
    dmsg_runtime::ledger::token_amount(n)
}

/// Create the wallet of a paid quote, or continue its interrupted
/// activation. The creator's device approval names the quote and the exact
/// wallet; the payment must be in the deposit.
#[ic_cdk::update]
async fn create_wallet(req: CreateWallet) -> Result<ActivationOrder> {
    let caller = ic_cdk::api::msg_caller();
    let me = ic_cdk::api::canister_self();
    let at = now();
    let id = req.quote_id;
    let quote = store::quote(&id).ok_or(Error::NotFound)?;
    ensure(quote.payer == caller, Error::Forbidden)?;
    if let Some(order) = store::order(&id) {
        ensure(order.spec == req.spec, Error::IdempotencyConflict)?;
        return activate(id, me).await;
    }
    ensure(at < quote.expires_at_ms, Error::Expired)?;
    ensure(store::payout(&id).is_none(), Error::Forbidden)?;
    let creator = MemberId::Account(quote.account_id);
    validate_spec(&req.spec, &creator)?;
    let (home, ledger) = store::with(|h| {
        ensure(!h.init.pricing.paused, Error::Locked)?;
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
        Ok::<_, Error>((home_of(h, &quote.account_id)?, h.init.icp_ledger))
    })?;
    // No other creation of this quote runs until the order is saved.
    let busy = Busy::take(id)?;
    let verified: Result<ApplicationAuthorization> = dmsg_runtime::call(
        home,
        "verify_application_authorization",
        (req.approval_id, req.approval.clone()),
    )
    .await?;
    verified?;
    let paid = balance(ledger, quote.deposit).await?;
    ensure(paid >= u128::from(quote.price_e8s), Error::FeeBlocked)?;
    // The deadline, a refund claimed meanwhile, the pause and the quota are
    // rechecked.
    let at = now();
    ensure(at < quote.expires_at_ms, Error::Expired)?;
    ensure(store::payout(&id).is_none(), Error::Forbidden)?;
    ensure(store::module(quote.version).is_some(), Error::NotFound)?;
    let quota = store::with(|h| {
        ensure(!h.init.pricing.paused, Error::Locked)?;
        Ok::<_, Error>(plan_quota(h, &quote.plan))
    })?;
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
    store::hold_quota(&order.quote.account_id, &id);
    store::save_order(&order);
    drop(busy);
    activate(id, me).await
}

/// Continue an activation after an interrupted creation or install.
#[ic_cdk::update]
async fn reconcile_activation(id: Hash) -> Result<ActivationOrder> {
    activate(id, ic_cdk::api::canister_self()).await
}

fn wallet_init(h: &Hub, order: &ActivationOrder, me: Principal) -> WalletInit {
    WalletInit {
        environment: h.init.environment.clone(),
        hub: me,
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

async fn activate(id: Hash, me: Principal) -> Result<ActivationOrder> {
    let _busy = Busy::take(id)?;
    loop {
        let order = store::order(&id).ok_or(Error::NotFound)?;
        match order.status {
            ActivationStatus::Paid => {
                ensure(
                    store::module(order.quote.version).is_some(),
                    Error::NotFound,
                )?;
                let created = create_canister_with_extra_cycles(
                    &CreateCanisterArgs {
                        settings: Some(CanisterSettings {
                            controllers: Some(vec![me]),
                            freezing_threshold: Some(Nat::from(FREEZING_THRESHOLD_SECONDS)),
                            log_visibility: Some(LogVisibility::Public),
                            ..Default::default()
                        }),
                    },
                    order.quote.cycles_to_wallet,
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
                let wallet = order.wallet.expect("created");
                let version = order.quote.version;
                let module = store::module(version).ok_or(Error::NotFound)?;
                let arg = store::with(|h| candid::encode_one(wallet_init(h, &order, me)))
                    .map_err(unavailable)?;
                // Only this activation can install code before the wallet is
                // indexed. Recover an install whose successful reply was lost.
                let status = canister_status(&CanisterIdRecord {
                    canister_id: wallet,
                })
                .await
                .map_err(unavailable)?;
                if status.module_hash.is_none() {
                    install(wallet, &module, CanisterInstallMode::Install, arg).await?;
                }
                let at = now();
                let mut order = store::order(&id).ok_or(Error::NotFound)?;
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

fn transfer_arg(
    me: Principal,
    from: &Hash,
    t: &PayoutTransfer,
    created_at_time: u64,
    fee: Option<u128>,
) -> TransferArg {
    TransferArg {
        from_subaccount: Some(deposit_subaccount(me, from).into_array()),
        to: t.to,
        fee: fee.map(Nat::from),
        created_at_time: Some(created_at_time),
        memo: t.memo.clone().map(Memo),
        amount: Nat::from(t.amount),
    }
}

/// Send a payout's transfers in order, then, for revenue, notify the CMC of
/// the hub's top-up.
async fn pay_out(id: Hash, me: Principal, refund: bool) -> Result<Payout> {
    // The caller owns the quote's Busy guard through reads and writes.
    let read = || {
        (if refund {
            store::refund(&id)
        } else {
            store::payout(&id)
        })
        .ok_or(Error::NotFound)
    };
    let save = |p: &Payout| {
        if refund {
            store::save_refund(&id, p)
        } else {
            store::save_payout(&id, p)
        }
    };
    let (ledger, cmc) = store::with(|h| (h.init.icp_ledger, h.init.cmc));
    loop {
        let mut payout = read()?;
        let Some(i) = payout.transfers.iter().position(|t| t.block.is_none()) else {
            if payout.refund || payout.cycles.is_some() {
                return Ok(payout);
            }
            let block = payout.transfers[0].block.expect("sent");
            let cycles = match notify_top_up(cmc, block, me).await {
                Notified::Cycles(cycles) => cycles,
                // The CMC returned the ICP to the deposit and minted nothing.
                Notified::Refunded(_) => 0,
                Notified::Pending => return Err(Error::Pending),
            };
            payout = read()?;
            payout.cycles = Some(cycles);
            save(&payout);
            return Ok(payout);
        };
        let arg = transfer_arg(
            me,
            &id,
            &payout.transfers[i],
            payout.created_at_time,
            payout.fee,
        );
        let block = match send(ledger, arg).await {
            Sent::Block(block) => block,
            Sent::Refused(reason) => return Err(Error::InvalidInput(reason)),
            Sent::Unknown => return Err(Error::ExecutionUnknown),
        };
        payout = read()?;
        payout.transfers[i].block = Some(block);
        save(&payout);
    }
}

/// Money still owed to activation revenue. Surplus is never revenue, and
/// partially collected revenue reserves only its unsent legs.
fn activation_reserve(id: &Hash, at: u64) -> Result<u128> {
    let quote = store::quote(id).ok_or(Error::NotFound)?;
    if let Some(order) = store::order(id) {
        if matches!(
            order.status,
            ActivationStatus::Failed(_) | ActivationStatus::Refunded { .. }
        ) {
            return Ok(0);
        }
        if let Some(payout) = store::payout(id).filter(|p| !p.refund) {
            let fee = payout.fee.unwrap_or(u128::from(quote.ledger_fee_e8s));
            return Ok(payout
                .transfers
                .iter()
                .filter(|t| t.block.is_none())
                .map(|t| t.amount.saturating_add(fee))
                .sum());
        }
        return Ok(if order.collected {
            0
        } else {
            u128::from(quote.price_e8s)
        });
    }
    Ok(if at < quote.expires_at_ms {
        u128::from(quote.price_e8s)
    } else {
        0
    })
}

/// Reserved activation funds; clients subtract this from the deposit balance
/// when displaying surplus and late-payment refunds.
#[ic_cdk::query]
fn get_activation_reserve(id: Hash) -> Result<u128> {
    activation_reserve(&id, now())
}

/// Claim only unreserved funds. The quote caller chooses the destination;
/// an unfinished refund keeps its exact arguments and destination on retry.
/// A later deposit creates a separate refund after the previous one completed.
#[ic_cdk::update]
async fn claim_activation_refund(id: Hash, to: Account) -> Result<u64> {
    let caller = ic_cdk::api::msg_caller();
    let me = ic_cdk::api::canister_self();
    let quote = store::quote(&id).ok_or(Error::NotFound)?;
    ensure(quote.payer == caller, Error::Forbidden)?;
    ensure_valid(
        authenticated(to.owner).is_ok() && to.owner != me,
        "refund account",
    )?;
    let _busy = Busy::take(id)?;
    let previous = store::refund(&id);
    let unfinished = previous
        .as_ref()
        .is_some_and(|p| p.transfers.iter().any(|t| t.block.is_none()));
    if !unfinished {
        let ledger = store::with(|h| h.init.icp_ledger);
        let (paid, fee) =
            futures_util::future::try_join(balance(ledger, quote.deposit), fee(ledger)).await?;
        let at = ic_cdk::api::time();
        let reserved = activation_reserve(&id, nanos_to_millis(at))?;
        let available = paid.saturating_sub(reserved);
        if available <= fee {
            if let Some(block) = previous.as_ref().and_then(|p| p.transfers[0].block) {
                return Ok(block);
            }
            return Err(
                if reserved > 0
                    || store::order(&id).is_some_and(|o| o.status == ActivationStatus::Ready)
                {
                    Error::Forbidden
                } else {
                    Error::NotFound
                },
            );
        }
        // Even two refunds in one IC timestamp have distinct deduplication keys.
        let created_at_time = previous
            .as_ref()
            .map_or(at, |p| at.max(p.created_at_time + 1));
        store::save_refund(
            &id,
            &Payout {
                refund: true,
                created_at_time,
                fee: Some(fee),
                transfers: vec![PayoutTransfer {
                    to,
                    amount: available - fee,
                    memo: Some(
                        digest("dmsg-wallet/refund/v1", &(id, created_at_time))
                            .to_vec()
                            .into(),
                    ),
                    block: None,
                }],
                cycles: None,
            },
        );
    }
    let payout = pay_out(id, me, true).await?;
    let block = payout.transfers[0].block.expect("sent");
    if let Some(mut order) = store::order(&id)
        && matches!(
            order.status,
            ActivationStatus::Failed(_) | ActivationStatus::Refunded { .. }
        )
    {
        order.status = ActivationStatus::Refunded { block };
        order.updated_at_ms = now();
        store::save_order(&order);
    }
    Ok(block)
}

/// Stop an activation that has not installed any code. An installed wallet
/// cannot be refunded through this path, including after a lost install reply.
#[ic_cdk::update]
async fn abort_activation(id: Hash) -> Result<ActivationOrder> {
    let caller = ic_cdk::api::msg_caller();
    let _busy = Busy::take(id)?;
    let mut order = store::order(&id).ok_or(Error::NotFound)?;
    ensure(order.quote.payer == caller, Error::Forbidden)?;
    ensure(
        matches!(
            order.status,
            ActivationStatus::Paid | ActivationStatus::Created
        ),
        Error::VersionConflict,
    )?;
    if let Some(wallet) = order.wallet {
        let canister = CanisterIdRecord {
            canister_id: wallet,
        };
        let status = canister_status(&canister).await.map_err(unavailable)?;
        ensure(status.module_hash.is_none(), Error::Forbidden)?;
        stop_canister(&canister).await.map_err(unavailable)?;
        if let Err(error) = delete_canister(&canister).await {
            let _ = start_canister(&canister).await;
            return Err(unavailable(error));
        }
    }
    order.status = ActivationStatus::Failed("activation aborted before installation".into());
    order.updated_at_ms = now();
    store::release_quota(&order.quote.account_id, &id);
    store::save_order(&order);
    Ok(order)
}

/// Collect exactly the quoted activation price. Extra and late deposits stay
/// claimable by the quote caller and never increase the DAO's revenue.
#[ic_cdk::update]
async fn collect_revenue(id: Hash) -> Result<ActivationOrder> {
    let me = ic_cdk::api::canister_self();
    let _busy = Busy::take(id)?;
    let order = store::order(&id).ok_or(Error::NotFound)?;
    ensure(order.status == ActivationStatus::Ready, Error::Forbidden)?;
    if order.collected {
        return Ok(order);
    }
    if store::payout(&id).is_none() {
        let ledger = store::with(|h| h.init.icp_ledger);
        let (paid, fee) =
            futures_util::future::try_join(balance(ledger, order.quote.deposit), fee(ledger))
                .await?;
        let payout = store::with(|h| {
            let price = u128::from(order.quote.price_e8s);
            let share = u128::from(h.init.pricing.cycles_share_e8s);
            ensure(paid >= price && price > share + 2 * fee, Error::FeeBlocked)?;
            Ok::<_, Error>(Payout {
                refund: false,
                created_at_time: ic_cdk::api::time(),
                fee: Some(fee),
                transfers: vec![
                    PayoutTransfer {
                        to: Account {
                            owner: h.init.cmc,
                            subaccount: Some(cmc_subaccount(me)),
                        },
                        amount: share,
                        memo: Some(MEMO_TOP_UP.to_le_bytes().to_vec().into()),
                        block: None,
                    },
                    PayoutTransfer {
                        to: h.init.revenue,
                        amount: price - share - 2 * fee,
                        memo: None,
                        block: None,
                    },
                ],
                cycles: None,
            })
        })?;
        store::save_payout(&id, &payout);
    }
    pay_out(id, me, false).await?;
    let mut order = store::order(&id).ok_or(Error::NotFound)?;
    order.collected = true;
    order.updated_at_ms = now();
    store::save_order(&order);
    Ok(order)
}

fn wallet_busy(wallet: Principal) -> Hash {
    sha256(wallet.as_slice())
}

async fn release_code(wallet: Principal) {
    let _: Result<Result<()>> = dmsg_runtime::call(wallet, "release_code", ()).await;
}

/// Install the version a wallet accepted. Anyone may call it.
#[ic_cdk::update]
async fn upgrade_wallet(wallet: Principal) -> Result<WalletRecord> {
    let record = store::wallet(wallet).ok_or(Error::NotFound)?;
    ensure(!record.locked, Error::Locked)?;
    let _busy = Busy::take(wallet_busy(wallet))?;
    let id = CanisterIdRecord {
        canister_id: wallet,
    };
    let accepted: CodeAcceptance = match dmsg_runtime::call(wallet, "accepted_upgrade", ()).await {
        Ok(accepted) => accepted,
        Err(e) => {
            // A wallet an earlier upgrade could not restart answers nothing;
            // starting a running wallet changes nothing.
            let _ = start_canister(&id).await;
            return Err(e);
        }
    };
    let CodeAcceptance::Upgrade {
        version,
        module_hash,
    } = accepted
    else {
        return Err(Error::NotFound);
    };
    let registered = store::with(|h| {
        h.versions
            .iter()
            .find(|v| v.version == version)
            .map(|v| v.module_hash)
    })
    .ok_or(Error::NotFound)?;
    ensure(registered == module_hash, Error::IntegrityFailed)?;
    ensure(
        store::wallet(wallet).is_some_and(|r| r.version != version),
        Error::IdempotencyConflict,
    )?;
    let module = store::module(version).ok_or(Error::NotFound)?;
    let claimed: Result<()> = dmsg_runtime::call(
        wallet,
        "claim_code",
        (CodeAcceptance::Upgrade {
            version,
            module_hash,
        },),
    )
    .await?;
    claimed?;
    // The wallet has no calls to the hub in flight, so stopping it ends. A
    // stop that fails, such as one timing out on a call still in flight,
    // leaves it stopping: it runs again, since this method could not reach
    // a stopped wallet to start it later.
    if let Err(e) = stop_canister(&id).await {
        if start_canister(&id).await.is_ok() {
            release_code(wallet).await;
        }
        return Err(unavailable(e));
    }
    let arg = candid::encode_one(Some(version)).map_err(unavailable)?;
    let installed = install(wallet, &module, CanisterInstallMode::Upgrade(None), arg).await;
    // The installed version is recorded even if the start below fails.
    if installed.is_ok() {
        let mut record = store::wallet(wallet).ok_or(Error::NotFound)?;
        let old = record.clone();
        record.version = version;
        store::save_wallet(Some(&old), &record);
    }
    start_canister(&id).await.map_err(unavailable)?;
    release_code(wallet).await;
    installed?;
    store::wallet(wallet).ok_or(Error::NotFound)
}

/// Remove every controller of a wallet that accepted locking its code.
#[ic_cdk::update]
async fn lock_wallet(wallet: Principal) -> Result<WalletRecord> {
    let _busy = Busy::take(wallet_busy(wallet))?;
    let record = store::wallet(wallet).ok_or(Error::NotFound)?;
    if record.locked {
        return Ok(record);
    }
    let accepted: CodeAcceptance = dmsg_runtime::call(wallet, "accepted_upgrade", ()).await?;
    ensure(accepted == CodeAcceptance::Lock, Error::Forbidden)?;
    let claimed: Result<()> =
        dmsg_runtime::call(wallet, "claim_code", (CodeAcceptance::Lock,)).await?;
    claimed?;
    let locked = async {
        let status = canister_status(&CanisterIdRecord {
            canister_id: wallet,
        })
        .await
        .map_err(unavailable)?;
        ensure(status.status == CanisterStatusType::Running, Error::Pending)?;
        update_settings(&UpdateSettingsArgs {
            canister_id: wallet,
            settings: CanisterSettings {
                controllers: Some(vec![]),
                ..Default::default()
            },
        })
        .await
        .map_err(unavailable)
    }
    .await;
    release_code(wallet).await;
    locked?;
    let mut record = store::wallet(wallet).ok_or(Error::NotFound)?;
    let old = record.clone();
    record.locked = true;
    store::save_wallet(Some(&old), &record);
    Ok(record)
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
    if record.members != members {
        let old = record.clone();
        record.members = members;
        store::save_wallet(Some(&old), &record);
    }
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

/// Register the staged module as the next version new wallets get. The
/// staging must stay untouched until the proposal executes.
#[ic_cdk::update]
fn admin_register_version(module_hash: Hash, notes: String) -> Result<WalletVersion> {
    check_admin(ic_cdk::api::msg_caller())?;
    let at = now();
    let mut h = store::hub();
    check_register(&h, &module_hash, &notes)?;
    let bytes = store::staged();
    let v = WalletVersion {
        version: h.next_version,
        module_hash,
        size: bytes.len() as u64,
        notes,
        registered_at_ms: at,
        retired: false,
    };
    store::save_module(v.version, bytes);
    store::clear_staging();
    h.next_version += 1;
    h.current = Some(v.version);
    h.versions.push(v.clone());
    store::save(h);
    Ok(v)
}

#[ic_cdk::query]
fn validate_admin_register_version(module_hash: Hash, notes: String) -> Validation {
    validation(store::with(|h| {
        check_register(h, &module_hash, &notes).map(|()| {
            format!(
                "Register wallet module {} ({} bytes) as version {}, the one new wallets get: {notes}",
                hex(&module_hash[..]),
                store::staged_size(),
                h.next_version,
            )
        })
    }))
}

/// The version new wallets get once `version` is retired.
fn current_after_retiring(h: &Hub, version: u32) -> Option<u32> {
    h.versions
        .iter()
        .rev()
        .find(|v| !v.retired && v.version != version)
        .map(|v| v.version)
}

/// Stop giving a version to new wallets; installed wallets keep it and may
/// still accept it.
#[ic_cdk::update]
fn admin_retire_version(version: u32) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    let mut h = store::hub();
    h.current = current_after_retiring(&h, version);
    h.versions
        .iter_mut()
        .find(|v| v.version == version)
        .ok_or(Error::NotFound)?
        .retired = true;
    store::save(h);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_retire_version(version: u32) -> Validation {
    validation(store::with(|h| {
        let v = h
            .versions
            .iter()
            .find(|v| v.version == version)
            .ok_or(Error::NotFound)?;
        Ok(format!(
            "Retire wallet version {version} ({}); new wallets then get {}.{}",
            hex(&v.module_hash[..]),
            current_after_retiring(h, version)
                .map_or("no version, so activations stop".into(), |n| format!(
                    "version {n}"
                )),
            admin::unchanged(!v.retired, "Already retired"),
        ))
    }))
}

fn check_remove(h: &Hub, version: u32, at: u64) -> Result<&WalletVersion> {
    let v = h
        .versions
        .iter()
        .find(|v| v.version == version)
        .ok_or(Error::NotFound)?;
    ensure_valid(v.retired, "version in use by new wallets")?;
    ensure_valid(
        !store::version_in_use(version, at),
        "version retained by a quote or activation",
    )?;
    Ok(v)
}

/// Delete a retired version and its module, freeing a registration slot;
/// wallets can no longer install it.
#[ic_cdk::update]
fn admin_remove_version(version: u32) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    let at = now();
    let mut h = store::hub();
    check_remove(&h, version, at)?;
    h.versions.retain(|v| v.version != version);
    store::remove_module(version);
    store::save(h);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_remove_version(version: u32) -> Validation {
    let at = now();
    validation(store::with(|h| {
        check_remove(h, version, at).map(|v| {
            format!(
                "Remove retired wallet version {version} ({}, {} bytes); wallets can no longer install it.",
                hex(&v.module_hash[..]),
                v.size,
            )
        })
    }))
}

fn show_pricing(p: &HubPricing) -> String {
    let quotas: Vec<String> = p
        .quotas
        .iter()
        .map(|q| format!("{:?} {}", q.plan, q.wallets))
        .collect();
    format!(
        "{} e8s ICP of which {} e8s become hub cycles, {} cycles per wallet, wallets per plan [{}], {}",
        p.price_e8s,
        p.cycles_share_e8s,
        p.cycles_to_wallet,
        quotas.join(", "),
        if p.paused { "paused" } else { "open" },
    )
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
    let current = store::with(|h| h.init.pricing.clone());
    validation(check_pricing(&pricing).map(|()| {
        format!(
            "Set wallet activation to {} (currently {}).{}",
            show_pricing(&pricing),
            show_pricing(&current),
            admin::unchanged(pricing != current, "Same pricing"),
        )
    }))
}

#[ic_cdk::update]
fn admin_set_revenue(account: Account) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    check_revenue(&account, ic_cdk::api::canister_self())?;
    let mut h = store::hub();
    h.init.revenue = account;
    store::save(h);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_revenue(account: Account) -> Validation {
    let current = store::with(|h| h.init.revenue);
    validation(
        check_revenue(&account, ic_cdk::api::canister_self()).map(|()| {
            format!(
                "Send wallet activation revenue to {account} (currently {current}).{}",
                admin::unchanged(account != current, "Same account"),
            )
        }),
    )
}

fn show_uploader(uploader: Option<Principal>) -> String {
    uploader.map_or("administrators only".into(), |p| {
        format!("{p} and administrators")
    })
}

#[ic_cdk::update]
fn admin_set_uploader(uploader: Option<Principal>) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    uploader.map_or(Ok(()), authenticated)?;
    let mut h = store::hub();
    h.uploader = uploader;
    store::save(h);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_uploader(uploader: Option<Principal>) -> Validation {
    let current = store::with(|h| h.uploader);
    validation(uploader.map_or(Ok(()), authenticated).map(|()| {
        format!(
            "Let {} stage wallet modules for registration (currently {}).{}",
            show_uploader(uploader),
            show_uploader(current),
            admin::unchanged(uploader != current, "Same uploader"),
        )
    }))
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
/// them, module staging unless the caller is the uploader or an
/// administrator, and every other method for the anonymous Principal.
#[ic_cdk::inspect_message]
fn inspect_message() {
    let caller = ic_cdk::api::msg_caller();
    let method = ic_cdk::api::msg_method_name();
    let allowed = if method.starts_with("admin_") {
        check_admin(caller).is_ok()
    } else if method == "upload_wasm_chunk" || method == "clear_wasm_staging" {
        check_uploader(caller).is_ok()
    } else {
        authenticated(caller).is_ok()
    };
    if allowed {
        ic_cdk::api::accept_message();
    }
}

ic_cdk::export_candid!();
