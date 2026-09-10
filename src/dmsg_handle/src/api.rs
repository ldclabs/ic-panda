use crate::store::*;
use candid::{Nat, Principal};
use dmsg_protocol::*;
use dmsg_runtime::storage::MapExt;
use dmsg_runtime::{self as stable};
use dmsg_types::{handle::*, *};
use icrc_ledger_types::{
    icrc1::account::Account,
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}
fn me() -> Principal {
    ic_cdk::api::canister_self()
}
fn caller() -> Principal {
    ic_cdk::api::msg_caller()
}
fn controller() -> Result<()> {
    ensure(ic_cdk::api::is_controller(&caller()), Error::Forbidden)
}

fn check_intent(i: &HandleIntent, action: HandleAction) -> Result<()> {
    ensure(
        i.handle_canister == me() && i.action == action && normalize_handle(&i.handle)? == i.handle,
        Error::IntegrityFailed,
    )?;
    nonzero(i.op_id.as_slice())?;
    nonzero(i.account_id.as_slice())
}
async fn consume(i: &HandleIntent) -> Result<()> {
    let r: Result<()> = stable::call(
        cfg().init.home_user,
        "consume_handle_authorization",
        (i.clone(),),
    )
    .await?;
    r
}
#[ic_cdk::init]
fn init(args: HandleInit) {
    authenticated(args.home_user).expect("home user");
    authenticated(args.ledger).expect("ledger");
    assert!(args.ledger_fee < price("x") && args.max_pending > 0 && args.max_pending <= 10_000);
    save_cfg(&Config {
        schema: STABLE_SCHEMA,
        init: args,
        progress: SnapshotProgress {
            snapshot: None,
            imported: 0,
            rolling_digest: Hash::new([0; 32]),
            last_handle: None,
            sealed: false,
        },
        event_count: 0,
        event_tip: Hash::new([0; 32]),
        pending: 0,
    });
    certify_snapshot();
}
#[ic_cdk::post_upgrade]
fn post_upgrade() {
    assert_eq!(
        cfg().schema,
        STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    NAMES.with_borrow(|t| t.for_each(|k, r| CERT.with_borrow_mut(|c| c.put(k, &r))));
    certify_snapshot();
}
fn certify_snapshot() {
    CERT.with_borrow_mut(|c| c.put(b"_legacy_snapshot".to_vec(), &cfg().progress));
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
    certify_snapshot();
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
    let mut inserts = vec![];
    for e in &entries {
        ensure(
            normalize_handle(&e.handle)? == e.handle && e.frozen_admins.len() <= 16,
            invalid("legacy record"),
        )?;
        authenticated(e.legacy_owner)?;
        if let Some(old) = LEGACY.with_borrow(|t| t.load(e.handle.as_bytes())) {
            ensure(old == *e, Error::IdempotencyConflict)?;
            continue;
        }
        ensure(
            c.progress
                .last_handle
                .as_ref()
                .is_none_or(|last| last < &e.handle),
            invalid("snapshot entries must be sorted and unique"),
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
    for e in inserts {
        LEGACY.with_borrow_mut(|t| t.put(e.handle.as_bytes(), e));
    }
    save_cfg(&c);
    certify_snapshot();
    Ok(c.progress)
}
#[ic_cdk::update]
fn seal_legacy_snapshot() -> Result<SnapshotProgress> {
    controller()?;
    let mut c = cfg();
    let s = c.progress.snapshot.as_ref().ok_or(Error::NotFound)?;
    ensure(
        c.progress.imported == s.count && c.progress.rolling_digest == s.entries_digest,
        Error::IntegrityFailed,
    )?;
    c.progress.sealed = true;
    save_cfg(&c);
    certify_snapshot();
    Ok(c.progress)
}

fn commit_name(name: &str, from: Option<AccountId>, to: AccountId, version: u64) -> HandleRecord {
    let mut c = cfg();
    let event = HandleEvent {
        sequence: c.event_count,
        previous: c.event_tip,
        handle: name.into(),
        from,
        to: to.clone(),
        version,
        at: now(),
        legacy_snapshot: c
            .progress
            .snapshot
            .as_ref()
            .expect("sealed snapshot")
            .snapshot_id,
    };
    let tip = digest("dmsg/handle-event/v1", &event);
    EVENTS.with_borrow_mut(|t| t.put(&event.sequence.to_be_bytes(), &event));
    c.event_count += 1;
    c.event_tip = tip;
    save_cfg(&c);
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
    check_intent(&intent, HandleAction::ClaimLegacy)?;
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
            legacy.frozen_admins.contains(&caller()) && caller() != legacy.legacy_owner
        } else {
            caller() == legacy.legacy_owner
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
    Ok(commit_name(&intent.handle, None, intent.account_id, 1))
}

#[ic_cdk::update]
async fn reserve_handle(registration: Registration) -> Result<HandleOperation> {
    let i = &registration.intent;
    check_intent(i, HandleAction::Register)?;
    ensure(caller() == registration.payer.owner, Error::AuthRequired)?;
    let mut c = cfg();
    ensure(c.progress.sealed, Error::LegacyWriteDisabled)?;
    let amount = price(&i.handle)
        .checked_sub(c.init.ledger_fee)
        .ok_or(Error::FeeBlocked)?;
    ensure(
        registration.fee == c.init.ledger_fee
            && i.target_account.is_none()
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
    let key = op_key(&i.account_id, i.op_id);
    let fp = digest("dmsg/handle-registration/v1", &registration);
    if let Ok(o) = op(&key) {
        ensure(o.digest == fp, Error::IdempotencyConflict)?;
        return Ok(o);
    }
    ensure(
        record(&i.handle).is_none()
            && !LEGACY.with_borrow(|t| t.contains(i.handle.as_bytes()))
            && !LOCKS.with_borrow(|t| t.contains(i.handle.as_bytes())),
        Error::VersionConflict,
    )?;
    consume(i).await?;
    if let Ok(o) = op(&key) {
        ensure(o.digest == fp, Error::IdempotencyConflict)?;
        return Ok(o);
    }
    c = cfg();
    ensure(
        c.pending < c.init.max_pending
            && !SUBJECT_OPS.with_borrow(|t| t.contains(i.account_id.as_slice())),
        Error::QuotaExceeded,
    )?;
    ensure(
        record(&i.handle).is_none() && !LOCKS.with_borrow(|t| t.contains(i.handle.as_bytes())),
        Error::VersionConflict,
    )?;
    let o = HandleOperation {
        registration: registration.clone(),
        digest: fp,
        phase: HandlePhase::Reserved,
        amount,
        created_at: now(),
        expires_at: now() + 15 * MINUTE,
        memo: digest("dmsg/handle-memo/v1", &(me(), key)),
        ledger_block: None,
    };
    LOCKS.with_borrow_mut(|t| t.put(i.handle.as_bytes(), &key));
    SUBJECT_OPS.with_borrow_mut(|t| t.put(i.account_id.as_slice(), &key));
    save_op(&key, &o);
    c.pending += 1;
    save_cfg(&c);
    Ok(o)
}
fn release(o: &HandleOperation) {
    LOCKS.with_borrow_mut(|t| t.delete(o.registration.intent.handle.as_bytes()));
    SUBJECT_OPS.with_borrow_mut(|t| t.delete(o.registration.intent.account_id.as_slice()));
    let mut c = cfg();
    c.pending = c.pending.checked_sub(1).expect("pending accounting");
    save_cfg(&c);
}
fn finish_paid(key: &Hash, block: u64) -> Result<HandleOperation> {
    let mut o = op(key)?;
    if o.phase == HandlePhase::Committed {
        return Ok(o);
    }
    ensure(
        matches!(
            o.phase,
            HandlePhase::Charging | HandlePhase::ChargeUnknown | HandlePhase::Paid
        ),
        Error::VersionConflict,
    )?;
    let i = &o.registration.intent;
    ensure(
        LOCKS.with_borrow(|t| t.load(i.handle.as_bytes())) == Some(*key)
            && record(&i.handle).is_none(),
        Error::VersionConflict,
    )?;
    o.ledger_block = Some(block);
    o.phase = HandlePhase::Paid;
    save_op(key, &o);
    commit_name(&i.handle, None, i.account_id.clone(), 1);
    o.phase = HandlePhase::Committed;
    save_op(key, &o);
    release(&o);
    Ok(o)
}
#[ic_cdk::update]
async fn commit_handle(account_id: AccountId, op_id: Hash) -> Result<HandleOperation> {
    let key = op_key(&account_id, op_id);
    let mut o = op(&key)?;
    ensure(caller() == o.registration.payer.owner, Error::AuthRequired)?;
    if o.phase == HandlePhase::Committed {
        return Ok(o);
    }
    ensure(
        matches!(o.phase, HandlePhase::Reserved | HandlePhase::ChargeUnknown),
        if o.phase == HandlePhase::Charging {
            Error::Pending
        } else {
            Error::VersionConflict
        },
    )?;
    let was_unknown = o.phase == HandlePhase::ChargeUnknown;
    if !was_unknown {
        ensure(now() < o.expires_at, Error::Expired)?;
    }
    o.phase = HandlePhase::Charging;
    save_op(&key, &o);
    let args = TransferFromArgs {
        spender_subaccount: None,
        from: o.registration.payer,
        to: Account {
            owner: me(),
            subaccount: None,
        },
        amount: Nat::from(o.amount),
        fee: Some(Nat::from(o.registration.fee)),
        memo: Some(o.memo.to_vec().into()),
        created_at_time: Some(millis_to_nanos(o.created_at)?),
    };
    let response: Result<std::result::Result<Nat, TransferFromError>> =
        stable::call(cfg().init.ledger, "icrc2_transfer_from", (args,)).await;
    if let Ok(current) = op(&key) {
        if current.phase == HandlePhase::Committed {
            return Ok(current);
        }
    }
    match response {
        Ok(Ok(block)) => finish_paid(&key, dmsg_runtime::ledger::block_index(block)?),
        Ok(Err(TransferFromError::Duplicate { duplicate_of })) => {
            finish_paid(&key, dmsg_runtime::ledger::block_index(duplicate_of)?)
        }
        Ok(Err(_)) => {
            o = op(&key)?;
            if was_unknown {
                o.phase = HandlePhase::ChargeUnknown;
                save_op(&key, &o);
                Err(Error::ExecutionUnknown)
            } else {
                o.phase = HandlePhase::Expired;
                save_op(&key, &o);
                release(&o);
                Err(Error::Unavailable(
                    "ledger rejected charge; a new approved operation is required".into(),
                ))
            }
        }
        Err(_) => {
            o = op(&key)?;
            o.phase = HandlePhase::ChargeUnknown;
            save_op(&key, &o);
            Err(Error::ExecutionUnknown)
        }
    }
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
    ensure(
        tx.from == o.registration.payer
            && tx.to
                == Account {
                    owner: me(),
                    subaccount: None,
                }
            && tx.amount == o.amount
            && tx.memo == Some(o.memo.to_vec())
            && tx.created_at_time == Some(millis_to_nanos(o.created_at)?)
            && tx.spender
                == Some(Account {
                    owner: me(),
                    subaccount: None,
                }),
        Error::IntegrityFailed,
    )?;
    finish_paid(&key, block)
}
#[ic_cdk::update]
fn expire_handle_reservation(account_id: AccountId, op_id: Hash) -> Result<()> {
    let key = op_key(&account_id, op_id);
    let mut o = op(&key)?;
    ensure(
        o.phase == HandlePhase::Reserved && now() >= o.expires_at,
        Error::VersionConflict,
    )?;
    o.phase = HandlePhase::Expired;
    save_op(&key, &o);
    release(&o);
    Ok(())
}
#[ic_cdk::update]
async fn transfer_handle(from: HandleIntent, accept: HandleIntent) -> Result<HandleRecord> {
    check_intent(&from, HandleAction::Transfer)?;
    check_intent(&accept, HandleAction::AcceptTransfer)?;
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
            me(),
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
    if let Some((fp, r)) = TRANSFERS.with_borrow(|t| t.load(key.as_slice())) {
        ensure(
            fp == digest("dmsg/transfer/v1", &(&from, &accept)),
            Error::IdempotencyConflict,
        )?;
        return Ok(r);
    }
    let r = record(&from.handle).ok_or(Error::NotFound)?;
    ensure(
        r.owner_account == from.account_id
            && r.version == from.expected_version
            && r.version < u64::MAX,
        Error::VersionConflict,
    )?;
    consume(&from).await?;
    consume(&accept).await?;
    if let Some((_, r)) = TRANSFERS.with_borrow(|t| t.load(key.as_slice())) {
        return Ok(r);
    }
    let current = record(&from.handle).ok_or(Error::NotFound)?;
    ensure(current == r, Error::VersionConflict)?;
    let result = commit_name(
        &from.handle,
        Some(from.account_id.clone()),
        accept.account_id.clone(),
        r.version + 1,
    );
    TRANSFERS.with_borrow_mut(|t| {
        t.put(
            key.as_slice(),
            &(
                digest("dmsg/transfer/v1", &(&from, &accept)),
                result.clone(),
            ),
        )
    });
    Ok(result)
}
#[ic_cdk::query]
fn resolve_handle_certified(handles: Vec<String>) -> Result<CertifiedBatch> {
    let keys: Result<Vec<_>> = handles
        .iter()
        .map(|h| normalize_handle(h).map(|h| h.into_bytes()))
        .collect();
    CERT.with_borrow(|c| c.batch(me(), keys?))
}
#[ic_cdk::query]
fn get_handle_operation(account_id: AccountId, op_id: Hash) -> Result<HandleOperation> {
    op(&op_key(&account_id, op_id))
}
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
    CERT.with_borrow(|c| c.batch(me(), vec![b"_legacy_snapshot".to_vec()]))
}
#[ic_cdk::query]
fn list_legacy_reservations(after: Option<String>) -> Result<Vec<LegacyReservation>> {
    let key = match after {
        Some(name) => normalize_handle(&name)?.into_bytes(),
        None => vec![],
    };
    Ok(LEGACY
        .with_borrow(|t| t.page(key, 64))
        .into_iter()
        .map(|(_, v)| v)
        .collect())
}
#[ic_cdk::query]
fn get_handle_event(sequence: u64) -> Option<HandleEvent> {
    EVENTS.with_borrow(|t| t.load(&sequence.to_be_bytes()))
}
