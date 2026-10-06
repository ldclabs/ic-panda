use crate::state::Escrow;
use candid::Principal;
use dmsg_protocol::canonical;
use dmsg_runtime::cert_map::CertMap;
use dmsg_runtime::storage::{CompactStored, MapExt, Stored};
use dmsg_types::{payment::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use std::{cell::RefCell, collections::BTreeMap};

pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) init: PaymentInit,
    pub(crate) day: u64,
    pub(crate) orders_today: u32,
    pub(crate) minute: u64,
    pub(crate) ledger_reads: BTreeMap<Principal, u32>,
    pub(crate) ledger_writes: BTreeMap<Principal, u32>,
    pub(crate) authorizations: BTreeMap<Principal, u32>,
}

type Memory = VirtualMemory<DefaultMemoryImpl>;
type Table<V> = StableBTreeMap<Vec<u8>, V, Memory>;

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    pub(crate) static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        // 8 MiB buckets; 32,768 buckets address up to 256 GiB of stable data.
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STABLE_CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    // Heap survives ordinary messages and await commit points. Persist this
    // bounded record at upgrade, not on every budget reservation.
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
    pub(crate) static PAYER_OPEN: RefCell<Table<Stored<u32>>> =
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
}

/// Borrow the configuration for reads; do not touch CONFIG inside `f`.
pub(crate) fn with_cfg<R>(f: impl FnOnce(&Config) -> R) -> R {
    CONFIG.with_borrow(|c| f(c.as_ref().expect("initialized")))
}

pub(crate) fn save_cfg(c: Config) {
    CONFIG.with_borrow_mut(|value| *value = Some(c));
}

/// Mutate admission configuration without cloning any minute-budget maps.
pub(crate) fn configure(f: impl FnOnce(&mut PaymentInit)) {
    CONFIG.with_borrow_mut(|value| {
        let c = value.as_mut().expect("initialized");
        f(&mut c.init);
        certify_config(c);
    });
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
        max_escrows: c.init.max_escrows,
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
            c.ledger_reads.clear();
            c.ledger_writes.clear();
            c.authorizations.clear();
        }
        let (counts, caller, per_caller) = match kind {
            CallBudget::LedgerRead(caller) => (&mut c.ledger_reads, caller, 20),
            CallBudget::LedgerWrite(caller) => (&mut c.ledger_writes, caller, 40),
            CallBudget::Authorization(caller) => (&mut c.authorizations, caller, 10),
        };
        ensure(counts.values().sum::<u32>() < 200, Error::QuotaExceeded)?;
        let count = counts.entry(caller).or_default();
        ensure(*count < per_caller, Error::QuotaExceeded)?;
        *count += 1;
        Ok(())
    })
}

pub(crate) fn check_order_capacity(at: u64) -> Result<()> {
    with_cfg(|c| {
        ensure(
            ESCROWS.with_borrow(|t| t.len()) < c.init.max_escrows,
            Error::QuotaExceeded,
        )?;
        ensure(
            c.day != at / DAY || c.orders_today < c.init.daily_orders,
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

pub(crate) fn persist_config() {
    with_cfg(|c| STABLE_CONFIG.with_borrow_mut(|t| t.set(CompactStored::some(c))));
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

pub(crate) fn open_count(p: Principal) -> u32 {
    PAYER_OPEN.with_borrow(|t| t.load(p.as_slice()).unwrap_or(0))
}

pub(crate) fn release_payer(e: &Escrow) {
    let n = open_count(e.payer_principal)
        .checked_sub(1)
        .expect("open accounting");
    PAYER_OPEN.with_borrow_mut(|t| {
        if n == 0 {
            t.delete(e.payer_principal.as_slice());
        } else {
            t.put(e.payer_principal.as_slice(), &n);
        }
    });
}

pub(crate) fn put_leg(l: &TransferLeg) {
    LEGS.with_borrow_mut(|t| t.put(&key(l.escrow_id, l.leg_id), l));
}

pub(crate) fn get_leg(id: Hash, n: u64) -> Result<TransferLeg> {
    LEGS.with_borrow(|t| t.load(&key(id, n)).ok_or(Error::NotFound))
}

pub(crate) const STABLE_SCHEMA: u16 = 10;
