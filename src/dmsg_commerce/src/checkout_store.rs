//! Checkout persistence, authenticated price history, reader indexes and cold history.
use crate::{
    checkout_model::{Order, Transfer},
    store,
};
use candid::Principal;
use dmsg_protocol::{authenticated, digest};
use dmsg_runtime::storage::{CompactStored, MapExt, Stored};
use dmsg_types::{integration_billing::*, *};
use ic_stable_structures::{
    memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    ops::Bound::{Excluded, Included},
};

type Memory = VirtualMemory<DefaultMemoryImpl>;
type Table<V> = StableBTreeMap<Vec<u8>, V, Memory>;
const HISTORY_MS: u64 = 30 * DAY;
const MAX_FULL_ORDERS: u64 = 100_000;
const MAX_ORDER_HISTORY: u64 = 1_000_000;
const MAX_PRICE_HISTORY: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Asset {
    pub policy: SettlementAsset,
    pub verified: bool,
    pub fee: u128,
}

thread_local! {
    static PRICE_AUTHORITY: RefCell<StableCell<Stored<Option<Principal>>, Memory>> =
        RefCell::new(StableCell::init(store::memory(15), Stored(None)));
    static ASSETS: RefCell<Table<CompactStored<Asset>>> = RefCell::new(StableBTreeMap::init(store::memory(10)));
    static ORDERS: RefCell<Table<CompactStored<Order>>> = RefCell::new(StableBTreeMap::init(store::memory(11)));
    static DEPOSITS: RefCell<Table<Stored<CheckoutDeposit>>> = RefCell::new(StableBTreeMap::init(store::memory(12)));
    static TRANSFERS: RefCell<Table<CompactStored<Transfer>>> = RefCell::new(StableBTreeMap::init(store::memory(13)));
    static BLOCKS: RefCell<Table<Stored<Hash>>> = RefCell::new(StableBTreeMap::init(store::memory(14)));
    static PRICES: RefCell<Table<Stored<SettlementAsset>>> = RefCell::new(StableBTreeMap::init(store::memory(21)));
    static ORDER_READERS: RefCell<Table<Stored<()>>> = RefCell::new(StableBTreeMap::init(store::memory(22)));
    static TRANSFER_READERS: RefCell<Table<Stored<()>>> = RefCell::new(StableBTreeMap::init(store::memory(23)));
    static ARCHIVED_ORDERS: RefCell<Table<CompactStored<Order>>> = RefCell::new(StableBTreeMap::init(store::memory(24)));
    static ARCHIVED_TRANSFERS: RefCell<Table<CompactStored<Transfer>>> = RefCell::new(StableBTreeMap::init(store::memory(25)));
    static ORDER_EXPIRY: RefCell<Table<Stored<()>>> = RefCell::new(StableBTreeMap::init(store::memory(26)));
    static TRANSFER_EXPIRY: RefCell<Table<Stored<()>>> = RefCell::new(StableBTreeMap::init(store::memory(27)));
}

pub(crate) fn asset(ledger: Principal) -> Result<Asset> {
    ASSETS
        .with_borrow(|t| t.load(ledger.as_slice()))
        .ok_or(Error::NotFound)
}

pub(crate) fn assets() -> Vec<Asset> {
    ASSETS.with_borrow(|t| t.page(vec![], 2).into_iter().map(|(_, a)| a).collect())
}

fn price_prefix(ledger: Principal) -> Vec<u8> {
    digest("dmsg/checkout/price-history/v2", &ledger).to_vec()
}

fn price_key(ledger: Principal, version: u64) -> Vec<u8> {
    [price_prefix(ledger), version.to_be_bytes().to_vec()].concat()
}

/// Only governance/price-authority publications enter history. Ledger fee observations
/// change the verified fee, never the already published pricing snapshot.
pub(crate) fn save_asset(a: &Asset, at: u64) -> Result<()> {
    let prefix = price_prefix(a.policy.ledger);
    let rows = PRICES.with_borrow(|t| {
        t.range(prefix.clone()..=[prefix, vec![255; 8]].concat())
            .map(|v| (v.key().clone(), v.value().0))
            .collect::<Vec<_>>()
    });
    let key = price_key(a.policy.ledger, a.policy.policy_version);
    let old = PRICES.with_borrow(|t| t.load(&key));
    if let Some(old) = &old {
        ensure(*old == a.policy, Error::IdempotencyConflict)?;
    } else {
        ensure(
            rows.iter()
                .filter(|(_, p)| p.price_valid_until_ms > at)
                .count()
                < MAX_PRICE_HISTORY,
            Error::QuotaExceeded,
        )?;
    }
    PRICES.with_borrow_mut(|t| {
        for (k, p) in rows {
            if p.price_valid_until_ms <= at {
                t.delete(&k);
            }
        }
        t.put(&key, &a.policy);
    });
    ASSETS.with_borrow_mut(|t| t.put(a.policy.ledger.as_slice(), a));
    publish_assets();
    Ok(())
}

pub(crate) fn observe_fee(ledger: Principal, fee: u128) -> Result<()> {
    let mut a = asset(ledger)?;
    a.fee = fee;
    ASSETS.with_borrow_mut(|t| t.put(ledger.as_slice(), &a));
    Ok(())
}

pub(crate) fn check_price(quoted: &SettlementAsset, at: u64) -> Result<Asset> {
    let published =
        PRICES.with_borrow(|t| t.load(&price_key(quoted.ledger, quoted.policy_version)));
    ensure(published.as_ref() == Some(quoted), Error::PolicyStale)?;
    let current = asset(quoted.ledger)?;
    dmsg_protocol::commerce_v2::check_quoted_asset(&current.policy, quoted, at)?;
    Ok(current)
}

pub(crate) fn price_authority() -> Option<Principal> {
    PRICE_AUTHORITY.with_borrow(|t| t.get().0)
}

pub(crate) fn set_price_authority(authority: Principal) {
    PRICE_AUTHORITY.with_borrow_mut(|t| t.set(Stored(Some(authority))));
}

pub(crate) fn order(id: Hash) -> Result<Order> {
    ORDERS
        .with_borrow(|t| t.load(id.as_slice()))
        .or_else(|| ARCHIVED_ORDERS.with_borrow(|t| t.load(id.as_slice())))
        .ok_or(Error::NotFound)
}

pub(crate) fn check_capacity() -> Result<()> {
    let full = ORDERS.with_borrow(|t| t.len());
    ensure(
        full < MAX_FULL_ORDERS
            && full + ARCHIVED_ORDERS.with_borrow(|t| t.len()) < MAX_ORDER_HISTORY,
        Error::QuotaExceeded,
    )
}

pub(crate) fn transfer(id: Hash) -> Result<Transfer> {
    TRANSFERS
        .with_borrow(|t| t.load(id.as_slice()))
        .or_else(|| ARCHIVED_TRANSFERS.with_borrow(|t| t.load(id.as_slice())))
        .ok_or(Error::NotFound)
}

fn block_key(block: &CashBlock) -> Hash {
    digest("dmsg/checkout/block/v2", block)
}

fn deposit_key(id: Hash, block: &CashBlock) -> Vec<u8> {
    [id.as_slice(), block_key(block).as_slice()].concat()
}

pub(crate) fn block_owner(block: &CashBlock) -> Option<Hash> {
    BLOCKS.with_borrow(|t| t.load(block_key(block).as_slice()))
}

pub(crate) fn save_deposit(d: &CheckoutDeposit) {
    BLOCKS.with_borrow_mut(|t| t.put(block_key(&d.block).as_slice(), &d.order_id));
    DEPOSITS.with_borrow_mut(|t| t.put(&deposit_key(d.order_id, &d.block), d));
}

pub(crate) fn deposit(id: Hash, block: &CashBlock) -> Result<CheckoutDeposit> {
    DEPOSITS
        .with_borrow(|t| t.load(&deposit_key(id, block)))
        .ok_or(Error::NotFound)
}

pub(crate) fn deposits(id: Hash, after: Option<Hash>, take: u16) -> Result<CheckoutDepositsPage> {
    ensure_valid((1..=128).contains(&take), "page size")?;
    let start = after.map_or_else(
        || Included(id.to_vec()),
        |cursor| Excluded([id.as_slice(), cursor.as_slice()].concat()),
    );
    let end = Included([id.as_slice(), &[255; 32]].concat());
    let rows = DEPOSITS.with_borrow(|t| {
        t.range((start, end))
            .take(usize::from(take) + 1)
            .map(|v| (v.key().clone(), v.value().0))
            .collect::<Vec<_>>()
    });
    let next = (rows.len() > usize::from(take)).then(|| {
        Hash::new(
            rows[usize::from(take) - 1].0[32..]
                .try_into()
                .expect("deposit key"),
        )
    });
    Ok(CheckoutDepositsPage {
        deposits: rows
            .into_iter()
            .take(usize::from(take))
            .map(|(_, d)| d)
            .collect(),
        next,
    })
}

pub(crate) fn reader(o: &Order, caller: Principal, governance: Principal) -> bool {
    caller == o.quote.cash.payer.owner
        || caller == o.quote.product.merchant.owner
        || caller == o.quote.product.adapter
        || caller == governance
}

pub(crate) fn read_access(o: &Order, caller: Principal) -> Result<()> {
    ensure(
        reader(o, caller, store::config(|c| c.governance)),
        Error::Forbidden,
    )
}

fn readers(o: &Order) -> BTreeSet<Principal> {
    [
        o.quote.cash.payer.owner,
        o.quote.product.merchant.owner,
        o.quote.product.adapter,
        store::config(|c| c.governance),
    ]
    .into_iter()
    .collect()
}

fn reader_prefix(caller: Principal) -> Vec<u8> {
    digest("dmsg/checkout/reader/v2", &caller).to_vec()
}

fn reader_key(caller: Principal, id: Hash) -> Vec<u8> {
    [reader_prefix(caller), id.to_vec()].concat()
}

fn page_ids(
    t: &Table<Stored<()>>,
    caller: Principal,
    after: Option<Hash>,
    take: u16,
) -> (Vec<Hash>, Option<Hash>) {
    let prefix = reader_prefix(caller);
    let start = after.map_or_else(
        || Included(prefix.clone()),
        |id| Excluded([prefix.clone(), id.to_vec()].concat()),
    );
    let end = Included([prefix, vec![255; 32]].concat());
    let mut ids: Vec<_> = t
        .range((start, end))
        .take(usize::from(take) + 1)
        .map(|row| Hash::new(row.key()[32..].try_into().expect("reader key")))
        .collect();
    let next = if ids.len() > usize::from(take) {
        ids.pop();
        ids.last().copied()
    } else {
        None
    };
    (ids, next)
}

pub(crate) fn operations(
    caller: Principal,
    after: Option<Hash>,
    take: u16,
) -> Result<CheckoutOperationsPage> {
    authenticated(caller)?;
    ensure_valid((1..=32).contains(&take), "page size")?;
    let (ids, next) = ORDER_READERS.with_borrow(|t| page_ids(t, caller, after, take));
    let orders = ids
        .into_iter()
        .map(|id| {
            let o = order(id)?;
            Ok(CheckoutOperationAudit {
                order: o.view(),
                balances: o
                    .balances
                    .iter()
                    .map(|(ledger, b)| CheckoutLedgerBalance {
                        ledger: *ledger,
                        incoming_atomic: b.incoming,
                        refundable_atomic: b.refundable,
                        service_reserve_atomic: b.service,
                        fee_reserve_atomic: b.fees,
                        outgoing_atomic: b.outgoing,
                    })
                    .collect(),
            })
        })
        .collect::<Result<_>>()?;
    Ok(CheckoutOperationsPage { orders, next })
}

pub(crate) fn transfers(
    caller: Principal,
    after: Option<Hash>,
    take: u16,
) -> Result<CashTransfersPage> {
    authenticated(caller)?;
    ensure_valid((1..=32).contains(&take), "page size")?;
    let (ids, next) = TRANSFER_READERS.with_borrow(|t| page_ids(t, caller, after, take));
    let transfers = ids
        .into_iter()
        .map(|id| transfer(id).map(|t| t.view))
        .collect::<Result<_>>()?;
    Ok(CashTransfersPage { transfers, next })
}

fn expiry_key(at: u64, id: Hash) -> Vec<u8> {
    [at.to_be_bytes().as_slice(), id.as_slice()].concat()
}

fn transfer_expiry(t: &Transfer) -> Option<u64> {
    matches!(
        t.view.status,
        CashTransferStatus::Succeeded | CashTransferStatus::Superseded
    )
    .then_some(t.updated_at_ms.saturating_add(HISTORY_MS))
}

/// Internal bookkeeping never revives a cold certificate or moves its retention deadline.
pub(crate) fn save_state(o: &Order) {
    assert!(o.conserved(), "per-ledger money conservation");
    if ARCHIVED_ORDERS.with_borrow(|t| t.contains(o.id.as_slice())) {
        ARCHIVED_ORDERS.with_borrow_mut(|t| t.put(o.id.as_slice(), o));
    } else {
        ORDERS.with_borrow_mut(|t| t.put(o.id.as_slice(), o));
    }
}

pub(crate) fn save(o: &mut Order, at: u64) {
    assert!(o.conserved(), "per-ledger money conservation");
    let old = order(o.id).ok();
    if let Some(deadline) = old.as_ref().and_then(Order::archive_after) {
        ORDER_EXPIRY.with_borrow_mut(|t| t.delete(&expiry_key(deadline, o.id)));
    }
    if old.is_none() {
        ORDER_READERS.with_borrow_mut(|t| {
            for p in readers(o) {
                t.put(&reader_key(p, o.id), &());
            }
        });
    }
    o.updated_at_ms = at;
    ARCHIVED_ORDERS.with_borrow_mut(|t| t.delete(o.id.as_slice()));
    ORDERS.with_borrow_mut(|t| t.put(o.id.as_slice(), o));
    if let Some(deadline) = o.archive_after() {
        ORDER_EXPIRY.with_borrow_mut(|t| t.put(&expiry_key(deadline, o.id), &()));
    }
    store::certify(order_key(o.id), &o.view());
}

pub(crate) fn save_transfer(t: &mut Transfer, at: u64) {
    let old = transfer(t.view.transfer_id).ok();
    if let Some(deadline) = old.as_ref().and_then(transfer_expiry) {
        TRANSFER_EXPIRY.with_borrow_mut(|m| m.delete(&expiry_key(deadline, t.view.transfer_id)));
    }
    if old.is_none() {
        let o = order(t.view.order_id).expect("order before transfer");
        let mut readers = readers(&o);
        readers.insert(t.view.to.owner);
        TRANSFER_READERS.with_borrow_mut(|m| {
            for p in readers {
                m.put(&reader_key(p, t.view.transfer_id), &());
            }
        });
    }
    t.updated_at_ms = at;
    ARCHIVED_TRANSFERS.with_borrow_mut(|m| m.delete(t.view.transfer_id.as_slice()));
    TRANSFERS.with_borrow_mut(|m| m.put(t.view.transfer_id.as_slice(), t));
    if let Some(deadline) = transfer_expiry(t) {
        TRANSFER_EXPIRY.with_borrow_mut(|m| m.put(&expiry_key(deadline, t.view.transfer_id), &()));
    }
}

fn due(t: &Table<Stored<()>>, at: u64) -> Vec<Vec<u8>> {
    t.range(..=[at.to_be_bytes().as_slice(), &[255; 32]].concat())
        .take(32)
        .map(|row| row.key().clone())
        .collect()
}

/// Archive at most 32 orders and 32 transfers. All ledger obligations, deduplication
/// keys, exact input hashes, reader indexes and final receipts remain in stable memory.
pub(crate) fn sweep(at: u64) -> CheckoutHistorySweep {
    let keys = ORDER_EXPIRY.with_borrow(|t| due(t, at));
    let mut orders = 0;
    for key in keys {
        let id = Hash::new(key[8..].try_into().expect("expiry key"));
        if let Some(mut o) = ORDERS.with_borrow(|t| t.load(id.as_slice())) {
            if o.archive_after().is_some_and(|end| end <= at) {
                o.authorization = None;
                o.decision = None;
                o.reservation_released = true;
                ARCHIVED_ORDERS.with_borrow_mut(|t| t.put(id.as_slice(), &o));
                ORDERS.with_borrow_mut(|t| t.delete(id.as_slice()));
                store::CERT.with_borrow_mut(|c| c.remove(&order_key(id)));
                orders += 1;
            }
        }
        ORDER_EXPIRY.with_borrow_mut(|t| t.delete(&key));
    }
    let keys = TRANSFER_EXPIRY.with_borrow(|t| due(t, at));
    let mut transfers = 0;
    for key in keys {
        let id = Hash::new(key[8..].try_into().expect("expiry key"));
        if let Some(t) = TRANSFERS.with_borrow(|t| t.load(id.as_slice())) {
            if transfer_expiry(&t).is_some_and(|end| end <= at) {
                ARCHIVED_TRANSFERS.with_borrow_mut(|m| m.put(id.as_slice(), &t));
                TRANSFERS.with_borrow_mut(|m| m.delete(id.as_slice()));
                transfers += 1;
            }
        }
        TRANSFER_EXPIRY.with_borrow_mut(|t| t.delete(&key));
    }
    CheckoutHistorySweep { orders, transfers }
}

pub(crate) fn order_certificate_available(id: Hash) -> Result<()> {
    ensure(
        ORDERS.with_borrow(|t| t.contains(id.as_slice())),
        Error::ResultExpired,
    )
}

pub(crate) fn order_key(id: Hash) -> Vec<u8> {
    digest("dmsg/checkout/certificate/v2", &id).to_vec()
}

pub(crate) fn assets_key() -> Vec<u8> {
    digest("dmsg/settlement-assets/v2", &"supported").to_vec()
}

pub(crate) fn publish_assets() {
    store::certify(assets_key(), &asset_views());
}

pub(crate) fn asset_views() -> Vec<SettlementAssetView> {
    assets()
        .into_iter()
        .map(|a| SettlementAssetView {
            policy: a.policy,
            ledger_verified: a.verified,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkout_model::fixture::{self, base::*};
    use dmsg_runtime::storage::compact_bytes;
    use dmsg_types::billing::*;

    fn setup() {
        store::set_config(store::Config::new(CommerceInit {
            environment: Environment::Local,
            governance: principal(90),
            membership_canister: principal(91),
            user_homes: vec![principal(92)],
            max_subjects: 1000,
            daily_orders: 1000,
            catalog: Catalog {
                schema: 1,
                version: 1,
                effective_at_ms: 0,
                plans: dmsg_protocol::billing::default_plans(1),
                storage_products: vec![],
                terms_digest: Hash::new([8; 32]),
            },
        }));
    }

    fn closed(i: u64) -> Order {
        let mut input = fixture::open();
        let op = digest("history fixture", &i);
        input.quote.offer.operation_id = op;
        input.authorization.offer = input.quote.offer.clone();
        let mut o = Order::new(principal(8), input);
        o.status = CheckoutStatus::Rejected;
        o.reservation_released = true;
        o
    }

    #[test]
    fn history_preserves_input_identity_and_reader_pages_and_drops_cold_leaves() {
        setup();
        let mut o = closed(1);
        let original = o.clone();
        save(&mut o, NOW);
        let due = o.archive_after().unwrap();
        assert_eq!(sweep(due - 1).orders, 0);
        assert_eq!(sweep(due).orders, 1);
        let archived = order(o.id).unwrap();
        assert!(archived.authorization.is_none());
        assert_eq!(archived.input_hash, original.input_hash);
        assert_eq!(archived.view(), original.view());
        assert!(compact_bytes(&archived).len() < compact_bytes(&original).len());
        assert_eq!(order_certificate_available(o.id), Err(Error::ResultExpired));
        assert!(store::CERT
            .with_borrow(|c| c.get(&order_key(o.id)))
            .is_none());
        let page = operations(o.quote.cash.payer.owner, None, 32).unwrap();
        assert_eq!(page.orders.len(), 1);
        assert!(page.next.is_none());
        // A new original-source obligation brings the same order back into the live tree.
        let mut restored = archived;
        restored.balances.insert(
            principal(6),
            crate::checkout_model::Balance {
                incoming: 100,
                refundable: 100,
                ..Default::default()
            },
        );
        save(&mut restored, due + 1);
        assert!(order_certificate_available(o.id).is_ok());
        assert_eq!(sweep(due + 100 * DAY).orders, 0);
        assert_eq!(order(o.id).unwrap().input_hash, original.input_hash);
    }

    #[test]
    fn indexes_skip_other_readers_and_only_return_real_continuations() {
        setup();
        let mut target = closed(0);
        save(&mut target, NOW);
        for i in 1..600 {
            let mut other = closed(i);
            other.quote.cash.payer.owner = principal(99);
            save(&mut other, NOW);
        }
        let page = operations(target.quote.cash.payer.owner, None, 1).unwrap();
        assert_eq!(page.orders.len(), 1);
        assert_eq!(page.orders[0].order.progress.order_id, target.id);
        assert!(page.next.is_none());
        let merchant = target.quote.product.merchant.owner;
        let mut after = None;
        let mut count = 0;
        loop {
            let page = operations(merchant, after, 32).unwrap();
            assert!(!page.orders.is_empty());
            count += page.orders.len();
            after = page.next;
            if after.is_none() {
                break;
            }
        }
        assert_eq!(count, 600);
    }

    #[test]
    fn incoming_pages_cover_every_deposit_without_losing_block_deduplication() {
        let id = Hash::new([4; 32]);
        for index in 0..131 {
            save_deposit(&CheckoutDeposit {
                order_id: id,
                block: CashBlock {
                    ledger: principal(6),
                    block_index: index,
                },
                from: principal(9).into(),
                amount_atomic: index + 1,
                refundable_atomic: index + 1,
            });
        }
        let first = deposits(id, None, 128).unwrap();
        assert_eq!(first.deposits.len(), 128);
        let second = deposits(id, first.next, 128).unwrap();
        assert_eq!(second.deposits.len(), 3);
        assert!(second.next.is_none());
        let ids: BTreeSet<_> = first
            .deposits
            .into_iter()
            .chain(second.deposits)
            .map(|d| d.block.block_index)
            .collect();
        assert_eq!(ids.len(), 131);
        assert_eq!(
            block_owner(&CashBlock {
                ledger: principal(6),
                block_index: 130
            }),
            Some(id)
        );
    }

    #[test]
    fn only_published_price_snapshots_can_be_used_and_expiry_is_bounded() {
        let a = Asset {
            policy: fixture::asset(),
            verified: true,
            fee: 10,
        };
        save_asset(&a, NOW).unwrap();
        let mut forged = a.policy.clone();
        forged.price_usd_micros = 1_010_000;
        assert_eq!(check_price(&forged, NOW), Err(Error::PolicyStale));
        let mut next = a.clone();
        next.policy.policy_version += 1;
        next.policy.price_usd_micros = 999_000;
        next.policy.price_observed_at_ms = NOW + MINUTE;
        next.policy.price_valid_until_ms = NOW + 31 * MINUTE;
        save_asset(&next, NOW + MINUTE).unwrap();
        check_price(&a.policy, NOW + MINUTE).unwrap();
        assert_eq!(
            check_price(&a.policy, a.policy.price_valid_until_ms),
            Err(Error::PolicyStale)
        );
        next.policy.policy_version += 1;
        next.policy.price_observed_at_ms = NOW + 32 * MINUTE;
        next.policy.price_valid_until_ms = NOW + 62 * MINUTE;
        save_asset(&next, NOW + 32 * MINUTE).unwrap();
        assert_eq!(PRICES.with_borrow(|t| t.len()), 1);
    }

    #[test]
    fn archival_waits_for_actual_transfer_completion_and_is_bounded() {
        setup();
        let mut o = closed(0);
        o.pending_transfers = 1;
        save(&mut o, NOW);
        for i in 1..70 {
            let mut next = closed(i);
            save(&mut next, NOW);
        }
        let at = NOW + 100 * DAY;
        assert_eq!(sweep(at).orders, 32);
        assert_eq!(sweep(at).orders, 32);
        assert_eq!(sweep(at).orders, 5);
        assert!(order_certificate_available(o.id).is_ok());
        o.pending_transfers = 0;
        save(&mut o, at);
        assert_eq!(sweep(at).orders, 0);
        assert_eq!(sweep(at + HISTORY_MS).orders, 1);
    }

    #[test]
    #[ignore = "native history/index/encoding sample, not production Wasm capacity"]
    fn checkout_history_profile() {
        setup();
        let mut seeded = 0;
        for count in [1_000u64, 10_000] {
            let started = std::time::Instant::now();
            for i in seeded..count {
                let mut o = closed(i);
                if i != 0 {
                    o.quote.cash.payer.owner = principal(99);
                }
                save(&mut o, NOW);
            }
            seeded = count;
            let write_ms = started.elapsed().as_millis();
            let caller = fixture::open().quote.cash.payer.owner;
            let started = std::time::Instant::now();
            for _ in 0..100 {
                std::hint::black_box(operations(caller, None, 32).unwrap());
            }
            let indexed_us = started.elapsed().as_micros();
            let started = std::time::Instant::now();
            for _ in 0..100 {
                ORDERS.with_borrow(|t| {
                    std::hint::black_box(
                        t.iter()
                            .take(512)
                            .map(|r| r.value().into_inner())
                            .filter(|o| reader(o, caller, principal(90)))
                            .count(),
                    );
                });
            }
            let scan_us = started.elapsed().as_micros();
            println!("orders={count} seed_ms={write_ms} indexed_100_us={indexed_us} legacy_scan_100_us={scan_us}");
        }
        let before = compact_bytes(&order(closed(0).id).unwrap()).len();
        let started = std::time::Instant::now();
        while sweep(NOW + 100 * DAY).orders > 0 {}
        let archived = order(closed(0).id).unwrap();
        let compacted = compact_bytes(&archived).len();
        let sweep_ms = started.elapsed().as_millis();
        println!("archive_10000_ms={sweep_ms} active_record_bytes={before} archived_record_bytes={compacted}");
    }
}
