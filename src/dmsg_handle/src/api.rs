use crate::store::*;
use candid::{Nat, Principal};
use dmsg_protocol::*;
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_runtime::{self as stable};
use dmsg_types::{handle::*, *};
use icrc_ledger_types::{
    icrc1::account::Account,
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};

const SNAPSHOT_KEY: &[u8] = b"_legacy_snapshot";

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn controller() -> Result<()> {
    ensure(
        ic_cdk::api::is_controller(&ic_cdk::api::msg_caller()),
        Error::Forbidden,
    )
}

fn check_intent(i: &HandleIntent, action: HandleAction, canister_id: Principal) -> Result<()> {
    ensure(
        i.handle_canister == canister_id
            && i.action == action
            && normalize_handle(&i.handle)? == i.handle,
        Error::IntegrityFailed,
    )?;
    nonzero(i.op_id.as_slice())?;
    nonzero(i.account_id.as_slice())
}

async fn consume(i: &HandleIntent) -> Result<()> {
    let r: Result<()> =
        stable::call(cfg().init.home_user, "consume_handle_authorization", (i,)).await?;
    r
}

#[ic_cdk::init]
fn init(args: HandleInit) {
    authenticated(args.home_user).expect("home user");
    authenticated(args.ledger).expect("ledger");
    assert!(
        args.ledger_fee < MIN_HANDLE_PRICE && args.max_pending > 0 && args.max_pending <= 10_000
    );
    let c = Config {
        schema: STABLE_SCHEMA,
        init: args,
        progress: SnapshotProgress {
            snapshot: None,
            imported: 0,
            rolling_digest: Hash::new([0; 32]),
            last_handle: None,
            sealed: false,
        },
        event_tip: Hash::new([0; 32]),
        pending: 0,
    };
    save_cfg(&c);
    certify_snapshot(&c.progress);
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    let c = cfg();
    assert_eq!(
        c.schema, STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    // Publish once, after all name leaves and the snapshot leaf are restored.
    CERT.with_borrow_mut(|cert| {
        NAMES.with_borrow(|t| {
            cert.extend(
                t.iter()
                    .map(|e| (e.key().clone(), canonical(&e.value().into_inner())))
                    .chain([(SNAPSHOT_KEY.to_vec(), canonical(&c.progress))]),
            )
        })
    });
}

#[ic_cdk::query]
fn get_handle_config() -> HandleInit {
    cfg().init
}

#[ic_cdk::update]
fn update_ledger_fee(fee: u128) -> Result<()> {
    controller()?;
    ensure(fee < MIN_HANDLE_PRICE, Error::FeeBlocked)?;
    let mut c = cfg();
    if c.init.ledger_fee != fee {
        c.init.ledger_fee = fee;
        save_cfg(&c);
    }
    Ok(())
}

fn certify_snapshot(progress: &SnapshotProgress) {
    CERT.with_borrow_mut(|c| c.put(SNAPSHOT_KEY.to_vec(), progress));
}

#[ic_cdk::update]
fn begin_legacy_snapshot(snapshot: LegacySnapshot) -> Result<()> {
    controller()?;
    let mut c = cfg();
    if let Some(existing) = &c.progress.snapshot {
        return ensure(existing == &snapshot, Error::IdempotencyConflict);
    }
    ensure(
        !c.progress.sealed && snapshot.count <= 1_000_000,
        Error::QuotaExceeded,
    )?;
    authenticated(snapshot.source_canister)?;
    nonzero(snapshot.snapshot_id.as_slice())?;
    c.progress.snapshot = Some(snapshot);
    save_cfg(&c);
    certify_snapshot(&c.progress);
    Ok(())
}

#[ic_cdk::update]
fn import_legacy_handles(
    snapshot_id: Hash,
    entries: Vec<LegacyReservation>,
) -> Result<SnapshotProgress> {
    controller()?;
    let mut c = cfg();
    let snapshot = c.progress.snapshot.as_ref().ok_or(Error::NotFound)?;
    ensure(
        snapshot.snapshot_id == snapshot_id && !c.progress.sealed,
        Error::VersionConflict,
    )?;
    ensure(
        !entries.is_empty() && entries.len() <= 256,
        Error::QuotaExceeded,
    )?;
    let mut inserts = Vec::with_capacity(entries.len());
    for e in &entries {
        ensure_valid(
            normalize_handle(&e.handle)? == e.handle && e.frozen_admins.len() <= 16,
            "legacy record",
        )?;
        authenticated(e.legacy_owner)?;
        // Frozen administrators claim as callers, so each must be a real identity.
        e.frozen_admins
            .iter()
            .try_for_each(|admin| authenticated(*admin))?;
        if let Some(old) = LEGACY.with_borrow(|t| t.load(e.handle.as_bytes())) {
            ensure(old == *e, Error::IdempotencyConflict)?;
            continue;
        }
        ensure_valid(
            c.progress
                .last_handle
                .as_ref()
                .is_none_or(|last| last < &e.handle),
            "snapshot entries must be sorted and unique",
        )?;
        c.progress.rolling_digest = digest("dmsg/legacy-entry/v1", &(c.progress.rolling_digest, e));
        c.progress.imported += 1;
        c.progress.last_handle = Some(e.handle.clone());
        inserts.push(e);
    }
    ensure(
        c.progress.imported <= snapshot.count,
        Error::IntegrityFailed,
    )?;
    if !inserts.is_empty() {
        LEGACY.with_borrow_mut(|t| {
            for e in inserts {
                t.put(e.handle.as_bytes(), e);
            }
        });
        save_cfg(&c);
        certify_snapshot(&c.progress);
    }
    Ok(c.progress)
}

#[ic_cdk::update]
fn seal_legacy_snapshot() -> Result<SnapshotProgress> {
    controller()?;
    let mut c = cfg();
    if c.progress.sealed {
        return Ok(c.progress);
    }
    let s = c.progress.snapshot.as_ref().ok_or(Error::NotFound)?;
    ensure(
        c.progress.imported == s.count && c.progress.rolling_digest == s.entries_digest,
        Error::IntegrityFailed,
    )?;
    c.progress.sealed = true;
    save_cfg(&c);
    certify_snapshot(&c.progress);
    Ok(c.progress)
}

// The caller persists the updated config together with its other local changes.
// No await or fallible business validation may follow the first write.
fn commit_name(
    c: &mut Config,
    name: &str,
    from: Option<AccountId>,
    to: AccountId,
    version: u64,
    at: u64,
) -> HandleRecord {
    let event = HandleEvent {
        sequence: EVENTS.with(|t| t.len()),
        previous: c.event_tip,
        handle: name.into(),
        from,
        to,
        version,
        at,
        legacy_snapshot: c
            .progress
            .snapshot
            .as_ref()
            .expect("sealed snapshot")
            .snapshot_id,
    };
    let tip = digest("dmsg/handle-event/v1", &event);
    EVENTS.with(|t| {
        t.append(&CompactStored::new(&event))
            .expect("append handle event")
    });
    c.event_tip = tip;
    let r = HandleRecord {
        handle: name.into(),
        owner_account: to,
        version,
        event_tip: tip,
    };
    NAMES.with_borrow_mut(|t| t.put(name.as_bytes(), &r));
    CERT.with_borrow_mut(|c| c.put(name.as_bytes().to_vec(), &r));
    r
}

#[ic_cdk::update]
async fn claim_legacy_handle(intent: HandleIntent, snapshot_id: Hash) -> Result<HandleRecord> {
    let caller = ic_cdk::api::msg_caller();
    check_intent(
        &intent,
        HandleAction::ClaimLegacy,
        ic_cdk::api::canister_self(),
    )?;
    let c = cfg();
    ensure(
        c.progress.sealed
            && c.progress
                .snapshot
                .as_ref()
                .is_some_and(|s| s.snapshot_id == snapshot_id),
        Error::VersionConflict,
    )?;
    let legacy = LEGACY
        .with_borrow(|t| t.load(intent.handle.as_bytes()))
        .ok_or(Error::NotFound)?;
    ensure(!legacy.quarantined, Error::Locked)?;
    let owner_is_name = legacy.legacy_name_principal == Some(legacy.legacy_owner);
    ensure(
        if owner_is_name {
            legacy.frozen_admins.contains(&caller) && caller != legacy.legacy_owner
        } else {
            caller == legacy.legacy_owner
        },
        Error::Forbidden,
    )?;
    ensure(
        intent.target_account.is_none()
            && intent.expected_version == 0
            && intent.terms_digest
                == digest(
                    "dmsg/legacy-claim/v1",
                    &(snapshot_id, &legacy, &intent.account_id),
                ),
        Error::IntegrityFailed,
    )?;
    if let Some(r) = record(&intent.handle) {
        ensure(
            r.owner_account == intent.account_id && r.version == 1,
            Error::IdempotencyConflict,
        )?;
        return Ok(r);
    }
    consume(&intent).await?;
    if let Some(r) = record(&intent.handle) {
        ensure(
            r.owner_account == intent.account_id && r.version == 1,
            Error::IdempotencyConflict,
        )?;
        return Ok(r);
    }
    let mut c = cfg();
    let at = now();
    let result = commit_name(&mut c, &intent.handle, None, intent.account_id, 1, at);
    save_cfg(&c);
    Ok(result)
}

// Locks the name and account only while its own ledger charge runs, so an
// unpaid request can never hold a name between messages.
#[ic_cdk::update]
async fn register_handle(registration: Registration) -> Result<HandleOperation> {
    let canister_id = ic_cdk::api::canister_self();
    let i = &registration.intent;
    check_intent(i, HandleAction::Register, canister_id)?;
    ensure(
        ic_cdk::api::msg_caller() == registration.payer.owner,
        Error::AuthRequired,
    )?;
    let key = op_key(&i.account_id, i.op_id);
    let fp = digest("dmsg/handle-registration/v1", &registration);
    if let Some(o) = registration_replay(&key, &fp)? {
        return Ok(o);
    }
    let c = cfg();
    check_registration(&c, &registration)?;
    check_available(&i.handle)?;
    check_pending(&c, &i.account_id)?;
    consume(i).await?;
    // Configuration, locks and capacity can all change during authorization.
    if let Some(o) = registration_replay(&key, &fp)? {
        return Ok(o);
    }
    let mut c = cfg();
    let amount = check_registration(&c, &registration)?;
    check_pending(&c, &i.account_id)?;
    check_available(&i.handle)?;
    let o = HandleOperation {
        registration,
        digest: fp,
        phase: HandlePhase::Charging,
        amount,
        created_at: now(),
        memo: digest("dmsg/handle-memo/v1", &(canister_id, key)),
        ledger_block: None,
    };
    let args = transfer_args(&o, canister_id)?;
    let i = &o.registration.intent;
    LOCKS.with_borrow_mut(|t| t.insert(i.handle.as_bytes().to_vec(), key.into_array()));
    ACTIVE_ACCOUNTS.with_borrow_mut(|t| t.insert(i.account_id.0));
    save_op(&key, &o);
    c.pending += 1;
    save_cfg(&c);
    charge(&key, args, false).await
}

// Replays stay cheap and independent of current quote configuration.
fn registration_replay(key: &Hash, fingerprint: &Hash) -> Result<Option<HandleOperation>> {
    let Ok(o) = op(key) else { return Ok(None) };
    ensure(o.digest == *fingerprint, Error::IdempotencyConflict)?;
    Ok(Some(o))
}

fn check_registration(c: &Config, registration: &Registration) -> Result<u128> {
    ensure(c.progress.sealed, Error::LegacyWriteDisabled)?;
    let i = &registration.intent;
    let amount = price(&i.handle)
        .checked_sub(c.init.ledger_fee)
        .ok_or(Error::FeeBlocked)?;
    ensure(registration.fee == c.init.ledger_fee, Error::FeeBlocked)?;
    ensure(
        i.target_account.is_none()
            && i.expected_version == 0
            && i.terms_digest
                == charge_terms_digest(
                    c.init.ledger,
                    &registration.payer,
                    amount,
                    registration.fee,
                ),
        Error::IntegrityFailed,
    )?;
    Ok(amount)
}

fn check_available(name: &str) -> Result<()> {
    ensure(
        !NAMES.with_borrow(|t| t.contains(name.as_bytes()))
            && !LEGACY.with_borrow(|t| t.contains(name.as_bytes()))
            && !LOCKS.with_borrow(|t| t.contains_key(&name.as_bytes().to_vec())),
        Error::VersionConflict,
    )
}

fn check_pending(c: &Config, account_id: &AccountId) -> Result<()> {
    ensure(
        c.pending < c.init.max_pending
            && !ACTIVE_ACCOUNTS.with_borrow(|t| t.contains(&account_id.0)),
        Error::QuotaExceeded,
    )
}

fn release(c: &mut Config, o: &HandleOperation) {
    LOCKS.with_borrow_mut(|t| t.remove(&o.registration.intent.handle.as_bytes().to_vec()));
    ACTIVE_ACCOUNTS.with_borrow_mut(|t| t.remove(&o.registration.intent.account_id.0));
    c.pending = c.pending.checked_sub(1).expect("pending accounting");
}

fn transfer_args(o: &HandleOperation, canister_id: Principal) -> Result<TransferFromArgs> {
    Ok(TransferFromArgs {
        spender_subaccount: None,
        from: o.registration.payer,
        to: Account {
            owner: canister_id,
            subaccount: None,
        },
        amount: Nat::from(o.amount),
        fee: Some(Nat::from(o.registration.fee)),
        memo: Some(o.memo.to_vec().into()),
        created_at_time: Some(millis_to_nanos(o.created_at)?),
    })
}

// The caller persists `Charging` before this call. A retry of an unknown
// charge stays unknown unless the ledger proves payment.
async fn charge(key: &Hash, args: TransferFromArgs, was_unknown: bool) -> Result<HandleOperation> {
    let response: std::result::Result<
        std::result::Result<Nat, TransferFromError>,
        stable::CallFailure,
    > = stable::call_classified(cfg().init.ledger, "icrc2_transfer_from", (args,)).await;
    let at = now();
    // Reload once: reconciliation can commit while the ledger call is in flight.
    let o = op(key)?;
    if o.phase == HandlePhase::Committed {
        return Ok(o);
    }
    match response {
        Ok(Ok(block))
        | Ok(Err(TransferFromError::Duplicate {
            duplicate_of: block,
        })) => match dmsg_runtime::ledger::block_index(block) {
            Ok(block) => finish_paid(key, o, block, at),
            Err(_) => charge_unknown(key, o),
        },
        // This rejection cannot resolve the previous ambiguous attempt.
        Ok(Err(_)) if was_unknown => charge_unknown(key, o),
        Ok(Err(error)) => reject(key, o, error.to_string()),
        Err(failure) if failure.preserves_unknown(was_unknown) => charge_unknown(key, o),
        Err(_) => reject(key, o, "ledger call not executed".into()),
    }
}

fn charge_unknown(key: &Hash, mut o: HandleOperation) -> Result<HandleOperation> {
    o.phase = HandlePhase::ChargeUnknown;
    save_op(key, &o);
    Err(Error::ExecutionUnknown)
}

// A definitive nonexecution releases both locks; the operation ID stays spent.
fn reject(key: &Hash, mut o: HandleOperation, reason: String) -> Result<HandleOperation> {
    o.phase = HandlePhase::Rejected {
        reason: reason.clone(),
    };
    save_op(key, &o);
    let mut c = cfg();
    release(&mut c, &o);
    save_cfg(&c);
    Err(Error::Unavailable(reason))
}

fn finish_paid(key: &Hash, mut o: HandleOperation, block: u64, at: u64) -> Result<HandleOperation> {
    if o.phase == HandlePhase::Committed {
        return Ok(o);
    }
    ensure(
        matches!(o.phase, HandlePhase::Charging | HandlePhase::ChargeUnknown),
        Error::VersionConflict,
    )?;
    let i = &o.registration.intent;
    ensure(
        LOCKS.with_borrow(|t| t.get(&i.handle.as_bytes().to_vec())) == Some(key.into_array())
            && !NAMES.with_borrow(|t| t.contains(i.handle.as_bytes())),
        Error::VersionConflict,
    )?;
    o.ledger_block = Some(block);
    let mut c = cfg();
    // Ownership, receipt and lock release commit in the same callback.
    commit_name(&mut c, &i.handle, None, i.account_id, 1, at);
    o.phase = HandlePhase::Committed;
    save_op(key, &o);
    release(&mut c, &o);
    save_cfg(&c);
    Ok(o)
}

// Retry an unknown charge with the original ledger arguments.
#[ic_cdk::update]
async fn commit_handle(account_id: AccountId, op_id: Hash) -> Result<HandleOperation> {
    let key = op_key(&account_id, op_id);
    let mut o = op(&key)?;
    ensure(
        ic_cdk::api::msg_caller() == o.registration.payer.owner,
        Error::AuthRequired,
    )?;
    match o.phase {
        HandlePhase::ChargeUnknown => {}
        HandlePhase::Committed => return Ok(o),
        HandlePhase::Charging => return Err(Error::Pending),
        HandlePhase::Rejected { .. } => return Err(Error::VersionConflict),
    }
    let args = transfer_args(&o, ic_cdk::api::canister_self())?;
    o.phase = HandlePhase::Charging;
    save_op(&key, &o);
    charge(&key, args, true).await
}

#[ic_cdk::update]
async fn reconcile_handle_charge(
    account_id: AccountId,
    op_id: Hash,
    block: u64,
) -> Result<HandleOperation> {
    let key = op_key(&account_id, op_id);
    let o = op(&key)?;
    if o.phase == HandlePhase::Committed {
        return Ok(o);
    }
    ensure(
        matches!(o.phase, HandlePhase::Charging | HandlePhase::ChargeUnknown),
        Error::VersionConflict,
    )?;
    let tx = dmsg_runtime::ledger::read_transfer(cfg().init.ledger, block).await?;
    let canister_id = ic_cdk::api::canister_self();
    ensure(
        tx.from == o.registration.payer
            && tx.to
                == Account {
                    owner: canister_id,
                    subaccount: None,
                }
            && tx.amount == o.amount
            && tx.memo == Some(o.memo.to_vec())
            && tx.created_at_time == Some(millis_to_nanos(o.created_at)?)
            && tx.spender
                == Some(Account {
                    owner: canister_id,
                    subaccount: None,
                }),
        Error::IntegrityFailed,
    )?;
    let at = now();
    finish_paid(&key, op(&key)?, block, at)
}

#[ic_cdk::update]
async fn transfer_handle(from: HandleIntent, accept: HandleIntent) -> Result<HandleRecord> {
    let canister_id = ic_cdk::api::canister_self();
    check_intent(&from, HandleAction::Transfer, canister_id)?;
    check_intent(&accept, HandleAction::AcceptTransfer, canister_id)?;
    ensure(
        from.account_id != accept.account_id
            && from.target_account.as_ref() == Some(&accept.account_id)
            && accept.target_account.as_ref() == Some(&from.account_id)
            && from.handle == accept.handle
            && from.expected_version == accept.expected_version
            && from.op_id == accept.op_id,
        Error::IntegrityFailed,
    )?;
    let terms = digest(
        "dmsg/handle-transfer/v1",
        &(
            canister_id,
            &from.handle,
            &from.account_id,
            &accept.account_id,
            from.expected_version,
            from.op_id,
        ),
    );
    ensure(
        from.terms_digest == terms && accept.terms_digest == terms,
        Error::IntegrityFailed,
    )?;
    let key = op_key(&from.account_id, from.op_id);
    let fingerprint = digest("dmsg/transfer/v1", &(&from, &accept));
    if let Some(receipt) = TRANSFERS.with_borrow(|t| t.load(key.as_slice())) {
        ensure(receipt.digest == fingerprint, Error::IdempotencyConflict)?;
        return Ok(receipt.record);
    }
    let r = record(&from.handle).ok_or(Error::NotFound)?;
    ensure(
        r.owner_account == from.account_id
            && r.version == from.expected_version
            && r.version < u64::MAX,
        Error::VersionConflict,
    )?;
    let authorized: Result<()> = stable::call(
        cfg().init.home_user,
        "consume_handle_transfer_authorizations",
        (&from, &accept),
    )
    .await?;
    authorized?;
    let at = now();
    if let Some(receipt) = TRANSFERS.with_borrow(|t| t.load(key.as_slice())) {
        ensure(receipt.digest == fingerprint, Error::IdempotencyConflict)?;
        return Ok(receipt.record);
    }
    let current = record(&from.handle).ok_or(Error::NotFound)?;
    ensure(current == r, Error::VersionConflict)?;
    let mut c = cfg();
    let result = commit_name(
        &mut c,
        &from.handle,
        Some(from.account_id),
        accept.account_id,
        r.version + 1,
        at,
    );
    TRANSFERS.with_borrow_mut(|t| {
        t.put(
            key.as_slice(),
            &TransferReceipt {
                digest: fingerprint,
                record: result.clone(),
            },
        )
    });
    save_cfg(&c);
    Ok(result)
}

#[ic_cdk::query]
fn resolve_handle_certified(handles: Vec<String>) -> Result<CertifiedBatch> {
    ensure(
        !handles.is_empty() && handles.len() <= MAX_BATCH,
        Error::QuotaExceeded,
    )?;
    let keys: Result<Vec<_>> = handles
        .iter()
        .map(|h| normalize_handle(h).map(|h| h.into_bytes()))
        .collect();
    CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), keys?))
}

#[ic_cdk::query]
fn get_handle_operation(account_id: AccountId, op_id: Hash) -> Result<HandleOperation> {
    op(&op_key(&account_id, op_id))
}

// Uncertified by design: a claim rechecks the exact frozen record on chain, so
// a forged reply can only make the claim fail.
#[ic_cdk::query]
fn get_legacy_reservation(handle: String) -> Result<Option<LegacyReservation>> {
    let name = normalize_handle(&handle)?;
    Ok(LEGACY.with_borrow(|t| t.load(name.as_bytes())))
}

#[ic_cdk::query]
fn snapshot_progress() -> SnapshotProgress {
    cfg().progress
}

#[ic_cdk::query]
fn snapshot_certified() -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![SNAPSHOT_KEY.to_vec()]))
}

#[ic_cdk::query]
fn get_handle_event(sequence: u64) -> Option<HandleEvent> {
    EVENTS.with(|t| t.get(sequence).map(CompactStored::into_inner))
}
