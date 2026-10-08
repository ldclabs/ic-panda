use crate::state::Escrow;
use candid::Principal;
use dmsg_protocol::{canonical, digest};
use dmsg_runtime::cert_map::CertMap;
use dmsg_runtime::storage::{CompactStored, MapExt, Stored};
use dmsg_types::{payment::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use std::{cell::RefCell, collections::BTreeMap};

/// Calls admitted in the current minute, in total and per caller.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Budget {
    pub(crate) calls: u32,
    pub(crate) callers: BTreeMap<Principal, u32>,
}

pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) init: PaymentInit,
    pub(crate) day: u64,
    pub(crate) orders_today: u32,
    pub(crate) minute: u64,
    pub(crate) ledger_reads: Budget,
    pub(crate) ledger_writes: Budget,
    pub(crate) authorizations: Budget,
}

impl Config {
    pub(crate) fn new(init: PaymentInit) -> Self {
        Self {
            schema: STABLE_SCHEMA,
            init,
            day: 0,
            orders_today: 0,
            minute: 0,
            ledger_reads: Budget::default(),
            ledger_writes: Budget::default(),
            authorizations: Budget::default(),
        }
    }
}

type Memory = VirtualMemory<DefaultMemoryImpl>;
type Table<V> = StableBTreeMap<Vec<u8>, V, Memory>;

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    // Stable memory itself; on the host a shared vector the capacity image exports.
    pub(crate) static RAW: DefaultMemoryImpl = DefaultMemoryImpl::default();
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        // 8 MiB buckets; 32,768 buckets address up to 256 GiB of stable data.
        RefCell::new(MemoryManager::init(RAW.with(Clone::clone)));
    static STABLE_CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    // Budgets commit at ordinary message boundaries and are persisted at
    // upgrade; administrative changes are persisted when they are made.
    static CONFIG: RefCell<Option<Config>> =
        RefCell::new(STABLE_CONFIG.with_borrow(|t| t.get().value()));
    pub(crate) static ESCROWS: RefCell<Table<CompactStored<Escrow>>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    pub(crate) static QUOTES: RefCell<Table<Stored<()>>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    pub(crate) static FUNDING: RefCell<Table<Stored<Hash>>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    pub(crate) static DEPOSITS: RefCell<Table<CompactStored<Deposit>>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    pub(crate) static LEGS: RefCell<Table<CompactStored<TransferLeg>>> =
        RefCell::new(StableBTreeMap::init(memory(5)));
    pub(crate) static SIGNERS: RefCell<Table<CompactStored<ReceiptSigner>>> =
        RefCell::new(StableBTreeMap::init(memory(6)));
    // Payer prefix || escrow ID of each escrow still waiting for a decision.
    pub(crate) static OPEN: RefCell<Table<Stored<()>>> =
        RefCell::new(StableBTreeMap::init(memory(7)));
    // Outgoing ledger block -> (escrow, leg) that produced it.
    pub(crate) static OUTGOING: RefCell<Table<Stored<(Hash, u64)>>> =
        RefCell::new(StableBTreeMap::init(memory(8)));
    // Key is payer prefix || escrow ID; the escrow ID is read back from the key.
    pub(crate) static PAYER_INDEX: RefCell<Table<Stored<()>>> =
        RefCell::new(StableBTreeMap::init(memory(9)));
    pub(crate) static FEE_POLICIES: RefCell<Table<CompactStored<DeliveryFeePolicy>>> =
        RefCell::new(StableBTreeMap::init(memory(10)));
    // Hashes only: each certified value is rebuilt from its record for a witness.
    pub(crate) static CERT: RefCell<CertMap<Memory>> =
        RefCell::new(CertMap::new(memory(11), memory(12)));
    // created_at_time || escrow ID || leg ID of every leg that has not
    // succeeded or been superseded, oldest first for payout dispatch.
    pub(crate) static PENDING: RefCell<Table<Stored<()>>> =
        RefCell::new(StableBTreeMap::init(memory(13)));
}

/// Borrow the configuration for reads; do not touch CONFIG inside `f`.
pub(crate) fn with_cfg<R>(f: impl FnOnce(&Config) -> R) -> R {
    CONFIG.with_borrow(|c| f(c.as_ref().expect("initialized")))
}

fn persist(c: &Config) {
    STABLE_CONFIG.with_borrow_mut(|t| t.set(CompactStored::some(c)));
}

/// Install a configuration and persist it.
pub(crate) fn save_cfg(c: Config) {
    persist(&c);
    CONFIG.with_borrow_mut(|value| *value = Some(c));
}

/// Persist the budgets and daily count before an upgrade.
pub(crate) fn persist_config() {
    with_cfg(persist);
}

/// Mutate the administrative configuration, recertify it and persist it, so
/// governance changes never depend on `pre_upgrade`.
pub(crate) fn configure(f: impl FnOnce(&mut PaymentInit)) {
    CONFIG.with_borrow_mut(|value| {
        let c = value.as_mut().expect("initialized");
        f(&mut c.init);
        certify_config(c);
        persist(c);
    });
}

/// Upper bounds a governance limit may take. Escrows are never deleted, so
/// `MAX_PAYMENT_ESCROWS` bounds stable memory; see the capacity profile.
pub(crate) const MAX_DAILY_ORDERS: u32 = 1_000_000;
pub(crate) const MAX_OPEN_PER_PAYER: u32 = 16;
pub(crate) const MAX_CALLS_PER_MINUTE: u32 = 100_000;
/// Offer verifications one caller may start per minute.
const AUTHORIZATIONS_PER_CALLER: u32 = 10;

pub(crate) fn check_limits(l: &PaymentLimits) -> Result<()> {
    ensure_valid(
        (1..=MAX_PAYMENT_ESCROWS).contains(&l.max_escrows)
            && (1..=MAX_DAILY_ORDERS).contains(&l.daily_orders)
            && (1..=MAX_OPEN_PER_PAYER).contains(&l.max_open_per_payer)
            && [
                l.authorizations_per_minute,
                l.ledger_reads_per_minute,
                l.ledger_writes_per_minute,
            ]
            .iter()
            .all(|n| (1..=MAX_CALLS_PER_MINUTE).contains(n))
            && (1..=MAX_CALLS_PER_MINUTE).contains(&l.ledger_calls_per_caller),
        "payment limits",
    )
}

fn public_config(c: &Config) -> PaymentConfiguration {
    PaymentConfiguration {
        schema: 2,
        user_homes: c.init.user_homes.clone(),
        ledger: c.init.ledger,
        platform: c.init.platform,
        ledger_fee: c.init.ledger_fee,
        max_fee: c.init.max_fee,
        signer_epoch: c.init.signer.epoch,
        enabled: c.init.enabled,
        max_escrows: c.init.limits.max_escrows,
    }
}

const CONFIGURATION_KEY: &[u8] = b"configuration";

pub(crate) fn signer_key(epoch: u64) -> Vec<u8> {
    [b"signer/".as_slice(), epoch.to_be_bytes().as_slice()].concat()
}

pub(crate) fn fee_key(version: u64) -> Vec<u8> {
    [b"fee/".as_slice(), version.to_be_bytes().as_slice()].concat()
}

pub(crate) fn certify_config(c: &Config) {
    let bytes = canonical(&public_config(c));
    CERT.with_borrow_mut(|tree| tree.insert(CONFIGURATION_KEY.to_vec(), &bytes));
}

pub(crate) fn certify_signer(s: &ReceiptSigner) {
    CERT.with_borrow_mut(|c| c.insert(signer_key(s.epoch), &canonical(s)));
}

pub(crate) fn certify_fee_policy(p: &DeliveryFeePolicy) {
    CERT.with_borrow_mut(|c| c.insert(fee_key(p.version), &canonical(p)));
}

/// Current value of a certified configuration, signer or fee-policy key.
pub(crate) fn configuration_leaf(key: &[u8]) -> Option<Vec<u8>> {
    if key == CONFIGURATION_KEY {
        return Some(with_cfg(|c| canonical(&public_config(c))));
    }
    let number = |prefix: &[u8]| -> Option<[u8; 8]> { key.strip_prefix(prefix)?.try_into().ok() };
    if let Some(epoch) = number(b"signer/") {
        return SIGNERS
            .with_borrow(|t| t.load(&epoch))
            .map(|s| canonical(&s));
    }
    let version = number(b"fee/")?;
    FEE_POLICIES
        .with_borrow(|t| t.load(&version))
        .map(|p| canonical(&p))
}

pub(crate) enum CallBudget {
    LedgerRead(Principal),
    LedgerWrite(Principal),
    Authorization(Principal),
}

pub(crate) fn reserve_call(at: u64, kind: CallBudget) -> Result<()> {
    CONFIG.with_borrow_mut(|value| {
        let c = value.as_mut().expect("initialized");
        if c.minute != at / MINUTE {
            c.minute = at / MINUTE;
            c.ledger_reads = Budget::default();
            c.ledger_writes = Budget::default();
            c.authorizations = Budget::default();
        }
        let l = c.init.limits.clone();
        let (budget, caller, global, per_caller) = match kind {
            CallBudget::LedgerRead(caller) => (
                &mut c.ledger_reads,
                caller,
                l.ledger_reads_per_minute,
                l.ledger_calls_per_caller,
            ),
            CallBudget::LedgerWrite(caller) => (
                &mut c.ledger_writes,
                caller,
                l.ledger_writes_per_minute,
                l.ledger_calls_per_caller,
            ),
            CallBudget::Authorization(caller) => (
                &mut c.authorizations,
                caller,
                l.authorizations_per_minute,
                AUTHORIZATIONS_PER_CALLER,
            ),
        };
        ensure(budget.calls < global, Error::QuotaExceeded)?;
        let used = budget.callers.entry(caller).or_default();
        ensure(*used < per_caller.min(global), Error::QuotaExceeded)?;
        *used += 1;
        budget.calls += 1;
        Ok(())
    })
}

pub(crate) fn check_order_capacity(at: u64) -> Result<()> {
    with_cfg(|c| {
        ensure(
            ESCROWS.with_borrow(|t| t.len()) < c.init.limits.max_escrows,
            Error::QuotaExceeded,
        )?;
        ensure(
            c.day != at / DAY || c.orders_today < c.init.limits.daily_orders,
            Error::QuotaExceeded,
        )
    })
}

pub(crate) fn reserve_order(at: u64) -> Result<()> {
    check_order_capacity(at)?;
    CONFIG.with_borrow_mut(|value| {
        let c = value.as_mut().expect("initialized");
        if c.day != at / DAY {
            c.day = at / DAY;
            c.orders_today = 0;
        }
        c.orders_today += 1;
    });
    Ok(())
}

pub(crate) fn orders_today(at: u64) -> u32 {
    with_cfg(|c| if c.day == at / DAY { c.orders_today } else { 0 })
}

pub(crate) fn load(id: &Hash) -> Result<Escrow> {
    ESCROWS.with_borrow(|t| t.load(id.as_slice()).ok_or(Error::NotFound))
}

/// Persist bookkeeping that does not change EscrowInfo or its certified leaf.
pub(crate) fn save_escrow(e: &Escrow) {
    assert!(e.conserved(), "funds conservation");
    ESCROWS.with_borrow_mut(|t| t.put(e.escrow_id.as_slice(), e));
}

pub(crate) fn save(e: &Escrow) {
    save_escrow(e);
    CERT.with_borrow_mut(|c| c.insert(e.escrow_id.to_vec(), &canonical(&e.info())));
}

pub(crate) fn key(id: Hash, n: u64) -> Vec<u8> {
    [id.as_slice(), n.to_be_bytes().as_slice()].concat()
}

pub(crate) fn signer(epoch: u64) -> Result<ReceiptSigner> {
    SIGNERS.with_borrow(|t| t.load(&epoch.to_be_bytes()).ok_or(Error::NotFound))
}

pub(crate) fn payer_prefix(payer: Principal) -> Hash {
    digest("dmsg/payer-index/v1", &payer)
}

pub(crate) fn payer_index_key(payer: Principal, id: &Hash) -> Vec<u8> {
    [payer_prefix(payer).as_slice(), id.as_slice()].concat()
}

/// Escrows of `payer` still waiting for a funds decision; at most
/// `max_open_per_payer` of them.
pub(crate) fn open_escrows(payer: Principal) -> Vec<Hash> {
    let prefix = payer_prefix(payer);
    OPEN.with_borrow(|t| {
        t.range(prefix.to_vec()..)
            .take_while(|e| e.key().starts_with(prefix.as_slice()))
            .map(|e| Hash::new(e.key()[32..].try_into().expect("open index key")))
            .collect()
    })
}

/// Record a new escrow under its payer.
pub(crate) fn index_payer(e: &Escrow) {
    let k = payer_index_key(e.payer_principal, &e.escrow_id);
    OPEN.with_borrow_mut(|t| t.put(&k, &()));
    PAYER_INDEX.with_borrow_mut(|t| t.put(&k, &()));
}

/// The escrow has a funds decision; its payer slot is free.
pub(crate) fn release_payer(e: &Escrow) {
    let removed =
        OPEN.with_borrow_mut(|t| t.remove(&payer_index_key(e.payer_principal, &e.escrow_id)));
    assert!(removed.is_some(), "open accounting");
}

fn pending_key(l: &TransferLeg) -> Vec<u8> {
    [
        l.created_at_time.to_be_bytes().as_slice(),
        &key(l.escrow_id, l.leg_id),
    ]
    .concat()
}

/// Store a leg and keep it in the dispatch index until it succeeds or is
/// superseded. `created_at_time` never changes, so neither does its key.
pub(crate) fn put_leg(l: &TransferLeg) {
    LEGS.with_borrow_mut(|t| t.put(&key(l.escrow_id, l.leg_id), l));
    let k = pending_key(l);
    PENDING.with_borrow_mut(|t| {
        if matches!(l.status, LegStatus::Succeeded | LegStatus::Superseded) {
            t.delete(&k);
        } else if !t.contains(&k) {
            t.put(&k, &());
        }
    });
}

pub(crate) fn get_leg(id: Hash, n: u64) -> Result<TransferLeg> {
    LEGS.with_borrow(|t| t.load(&key(id, n)).ok_or(Error::NotFound))
}

/// Up to 32 legs awaiting a ledger result, oldest `created_at_time` first,
/// after the cursor `(created_at_time, escrow_id, leg_id)`.
pub(crate) fn pending_legs(after: Option<(u64, Hash, u64)>) -> Vec<TransferLeg> {
    let cursor = after.map_or_else(Vec::new, |(at, id, n)| {
        [at.to_be_bytes().as_slice(), &key(id, n)].concat()
    });
    PENDING
        .with_borrow(|t| t.page(cursor, 32))
        .into_iter()
        .map(|(k, ())| {
            let id = Hash::new(k[8..40].try_into().expect("pending key"));
            let n = u64::from_be_bytes(k[40..].try_into().expect("pending key"));
            get_leg(id, n).expect("pending leg")
        })
        .collect()
}

pub(crate) const STABLE_SCHEMA: u16 = 12;
