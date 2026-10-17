//! Shared implementation included by the four legacy dMsg canisters.
//! Stable memory 200 is independent of every historical business state layout.
use candid::Principal;
use ic_message_types::migration::{
    FreezeState, FreezeStatus, LegacyMode, PendingWrite, SnapshotPage, SnapshotScope,
};
use ic_stable_structures::{
    DefaultMemoryImpl, StableCell, Storable, memory_manager::VirtualMemory, storable::Bound,
};
use serde_bytes::{ByteArray, ByteBuf};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    future::Future,
};

#[derive(Clone, Default)]
struct Stored(FreezeState);

impl Storable for Stored {
    const BOUND: Bound = Bound::Unbounded;

    fn into_bytes(self) -> Vec<u8> {
        cbor2::to_vec(&self.0).expect("encode legacy freeze")
    }

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(cbor2::to_vec(&self.0).expect("encode legacy freeze"))
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        Self(cbor2::from_slice(&bytes).expect("decode legacy freeze"))
    }
}

thread_local! {
    static CURRENT_TICKET: Cell<Option<u64>> = const { Cell::new(None) };
    static STATE: RefCell<StableCell<Stored, VirtualMemory<DefaultMemoryImpl>>> = RefCell::new(
        StableCell::init(crate::store::migration_memory(), Stored::default())
    );
}

fn change<T>(f: impl FnOnce(&mut FreezeState) -> Result<T, String>) -> Result<T, String> {
    STATE.with_borrow_mut(|cell| {
        let mut next = cell.get().clone();
        let result = f(&mut next.0)?;
        cell.set(next);
        Ok(result)
    })
}

pub fn upgraded() {
    change(|state| {
        state.upgraded();
        Ok(())
    })
    .expect("legacy freeze upgrade");
}

pub fn installed() {
    change(|state| {
        state.instrumented = true;
        Ok(())
    })
    .expect("legacy tracking initialization");
}

pub fn configuration_writable() -> Result<(), String> {
    STATE.with_borrow(|state| state.get().0.writable())
}

pub fn business<T>(
    method: &str,
    input: Vec<u8>,
    action: impl FnOnce(Principal, u64) -> Result<T, String>,
) -> Result<T, String> {
    let caller = ic_cdk::api::msg_caller();
    let at = ic_cdk::api::time() / 1_000_000;
    let ticket = change(|s| s.begin(method, caller, input, at))?;
    let result = action(caller, at);
    change(|s| s.finish(ticket))?;
    result
}

pub async fn business_async<T, F: Future<Output = Result<T, String>>>(
    method: &str,
    input: Vec<u8>,
    action: impl FnOnce(Principal, u64) -> F,
) -> Result<T, String> {
    let caller = ic_cdk::api::msg_caller();
    let at = ic_cdk::api::time() / 1_000_000;
    let ticket = change(|s| s.begin(method, caller, input, at))?;
    let mut future = Box::pin(action(caller, at));
    let result = std::future::poll_fn(|cx| {
        let previous = CURRENT_TICKET.with(|t| t.replace(Some(ticket)));
        let result = future.as_mut().poll(cx);
        CURRENT_TICKET.with(|t| t.set(previous));
        result
    })
    .await;
    // Reload stable phase and boot after the await. ReadOnly cannot be reached
    // while this live ticket exists; interrupted old boots require reconciliation.
    change(|s| s.finish(ticket))?;
    result
}

/// An inter-canister transport failure may hide a committed side effect. Keep
/// the original entry parameters for explicit reconciliation instead of
/// treating every returned error as a known cancellation.
pub fn outbound_failed() {
    if let Some(ticket) = CURRENT_TICKET.with(Cell::get) {
        change(|state| state.uncertain(ticket)).expect("record unknown legacy call");
    }
}

pub fn after_await() -> Result<(), String> {
    if let Some(ticket) = CURRENT_TICKET.with(Cell::get) {
        let now = ic_cdk::api::time() / 1_000_000;
        STATE.with_borrow(|state| state.get().0.after_callback(ticket, now))?;
    }
    Ok(())
}

#[ic_cdk::query]
fn legacy_status() -> FreezeStatus {
    STATE.with_borrow(|s| s.get().0.status())
}

#[ic_cdk::update(guard = "crate::is_controller")]
fn admin_legacy_drain(cutover_id: ByteArray<32>) -> Result<FreezeStatus, String> {
    let at = ic_cdk::api::time() / 1_000_000;
    change(|state| state.drain(cutover_id, at))
}

#[ic_cdk::update(guard = "crate::is_controller")]
fn admin_legacy_seal(cutover_id: ByteArray<32>) -> Result<FreezeStatus, String> {
    let at = ic_cdk::api::time() / 1_000_000;
    change(|state| {
        let already = state.mode == LegacyMode::ReadOnly;
        let status = state.seal(cutover_id, at)?;
        if !already {
            crate::store::migration_seal()?;
        }
        Ok(status)
    })
}

#[ic_cdk::query(guard = "crate::is_controller")]
fn admin_legacy_pending() -> Vec<PendingWrite> {
    STATE.with_borrow(|s| s.get().0.pending.values().cloned().collect())
}

#[ic_cdk::update(guard = "crate::is_controller")]
fn admin_legacy_resolve(ticket: u64, evidence_digest: ByteArray<32>) -> Result<(), String> {
    change(|state| state.resolve_interrupted(ticket, evidence_digest))
}

#[ic_cdk::update(guard = "crate::is_controller")]
fn admin_legacy_acknowledge_baseline(evidence_digest: ByteArray<32>) -> Result<(), String> {
    change(|state| state.acknowledge_baseline(evidence_digest))
}

// The SNS validators of the admin_legacy_* steps: the same transition on a
// copy of the current state, nothing written, and the text a proposal shows.

fn preview<T>(f: impl FnOnce(&mut FreezeState) -> Result<T, String>) -> Result<T, String> {
    STATE.with_borrow(|cell| f(&mut cell.get().0.clone()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The tickets that keep a canister from sealing, without callers or inputs.
fn blocking(state: &FreezeState) -> String {
    state
        .pending
        .values()
        .map(|p| {
            let kind = if p.uncertain {
                "uncertain"
            } else if !p.live || p.boot < state.boot {
                "interrupted"
            } else {
                "live"
            };
            format!(
                "{} {} ({kind}, started {} ms)",
                p.id, p.method, p.started_at
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[ic_cdk::query]
fn validate_admin_legacy_acknowledge_baseline(
    evidence_digest: ByteArray<32>,
) -> Result<String, String> {
    let canister = ic_cdk::api::canister_self();
    preview(|state| {
        let already = state.baseline_evidence.is_some();
        state.acknowledge_baseline(evidence_digest)?;
        Ok(format!(
            "Record baseline review {} on legacy canister {canister}: no write the old module started is still in flight.{}",
            hex(evidence_digest.as_ref()),
            if already { " Already recorded." } else { "" }
        ))
    })
}

#[ic_cdk::query]
fn validate_admin_legacy_drain(cutover_id: ByteArray<32>) -> Result<String, String> {
    let canister = ic_cdk::api::canister_self();
    let at = ic_cdk::api::time() / 1_000_000;
    preview(|state| {
        let already = match state.mode {
            LegacyMode::Active => "",
            LegacyMode::Draining => " Already draining.",
            LegacyMode::ReadOnly => " Already read-only.",
        };
        let status = state.drain(cutover_id, at)?;
        Ok(format!(
            "Stop new business writes on legacy canister {canister} for cutover {}; {} writes in flight finish first.{already}",
            hex(cutover_id.as_ref()),
            status.pending,
        ))
    })
}

#[ic_cdk::query]
fn validate_admin_legacy_seal(cutover_id: ByteArray<32>) -> Result<String, String> {
    let canister = ic_cdk::api::canister_self();
    let at = ic_cdk::api::time() / 1_000_000;
    preview(|state| {
        let already = state.mode == LegacyMode::ReadOnly;
        // List the writes in flight only when they are what blocks the seal.
        state.seal(cutover_id, at).map_err(|e| match e.as_str() {
            "LegacyWritesPending" => format!("{e}: {}", blocking(state)),
            _ => e,
        })?;
        Ok(format!(
            "Make legacy canister {canister} read-only for cutover {}; its business writes never reopen.{}",
            hex(cutover_id.as_ref()),
            if already { " Already read-only." } else { "" }
        ))
    })
}

#[ic_cdk::query]
fn validate_admin_legacy_resolve(
    ticket: u64,
    evidence_digest: ByteArray<32>,
) -> Result<String, String> {
    let canister = ic_cdk::api::canister_self();
    preview(|state| {
        let operation = state
            .pending
            .get(&ticket)
            .map(|p| format!("{} started {} ms", p.method, p.started_at));
        state.resolve_interrupted(ticket, evidence_digest)?;
        Ok(match operation {
            Some(operation) => format!(
                "Settle interrupted operation {ticket} ({operation}) on legacy canister {canister} with review record {}.",
                hex(evidence_digest.as_ref())
            ),
            None => format!("Operation {ticket} on legacy canister {canister} is already settled."),
        })
    })
}

/// This bounded update preserves the existing certified-data tree (including
/// legacy sign-in and name-log proofs). Its ingress reply can be independently
/// retained with a request_status certificate by the migration reader.
#[ic_cdk::update]
fn legacy_snapshot(scope: SnapshotScope, after: Option<String>) -> Result<SnapshotPage, String> {
    let caller = ic_cdk::api::msg_caller();
    let source = ic_cdk::api::canister_self();
    let status = legacy_status();
    if status.mode != LegacyMode::ReadOnly {
        return Err("LegacyNotFrozen".into());
    }
    // Ownership/roles are already public. Private scopes keep per-caller ACLs.
    if caller == Principal::anonymous() {
        return Err("anonymous user is not allowed".into());
    }
    let (rows, context) = if let SnapshotScope::Attestation { digest, expires_at } = &scope {
        let at = ic_cdk::api::time() / 1_000_000;
        if digest.as_ref().iter().all(|b| *b == 0)
            || *expires_at <= at
            || *expires_at > at.saturating_add(7 * 86_400_000)
            || after.is_some()
        {
            return Err("InvalidLegacyAttestation".into());
        }
        let value = candid::encode_args((caller, digest, expires_at)).map_err(|e| e.to_string())?;
        (vec![(caller.to_text(), value)], Vec::new())
    } else {
        (
            crate::store::migration_rows(&scope, caller)?,
            crate::store::migration_context(&scope)?,
        )
    };
    if rows.len() > 100_000 || rows.iter().map(|(_, v)| v.len()).sum::<usize>() > 256 * 1024 * 1024
    {
        return Err("LegacySnapshotTooLarge".into());
    }
    if !rows.windows(2).all(|r| r[0].0 < r[1].0) {
        return Err("LegacySnapshotOrder".into());
    }
    let encoded = candid::encode_args((
        "dmsg/legacy-snapshot/1",
        source,
        caller,
        status.epoch,
        &status.cutover_id,
        &scope,
        &ByteBuf::from(context.clone()),
    ))
    .map_err(|e| e.to_string())?;
    let initial: [u8; 32] = Sha256::digest(encoded).into();
    let mut digest = initial;
    for (key, value) in &rows {
        let mut hash = Sha256::new();
        hash.update(digest);
        hash.update((key.len() as u64).to_be_bytes());
        hash.update(key.as_bytes());
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
        digest = hash.finalize().into();
    }
    let mut entries = Vec::new();
    let mut total = 0;
    let mut complete = true;
    for (key, value) in rows
        .iter()
        .filter(|(key, _)| after.as_ref().is_none_or(|a| key > a))
    {
        if entries.len() >= 64 || total + value.len() > 128 * 1024 {
            complete = false;
            break;
        }
        total += value.len();
        entries.push((key.clone(), ByteBuf::from(value.clone())));
    }
    if entries.is_empty() && !complete {
        return Err("LegacySnapshotEntryTooLarge".into());
    }
    Ok(SnapshotPage {
        schema: 1,
        source,
        subject: caller,
        freeze: status,
        scope,
        source_context: ByteBuf::from(context),
        count: rows.len() as u64,
        initial_digest: ByteArray::new(initial),
        inventory_digest: ByteArray::new(digest),
        next: entries.last().map(|(key, _)| key.clone()).or(after),
        entries,
        complete,
    })
}
