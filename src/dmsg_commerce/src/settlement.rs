//! Durable, bounded settlement work. Only fixed order destinations are paid.
use crate::{
    api::now,
    checkout,
    checkout_model::{Order, Transfer},
    checkout_store, store,
};
use dmsg_runtime::storage::{MapExt, Stored};
use dmsg_types::{integration_billing::*, *};
use ic_stable_structures::{
    DefaultMemoryImpl, StableBTreeMap, StableCell, memory_manager::VirtualMemory,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    time::Duration,
};

type Memory = VirtualMemory<DefaultMemoryImpl>;
const BATCH: usize = 32;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Backfill {
    // Orders, transfers, then complete. Existing records keep schema 8.
    phase: u8,
    after: Option<Hash>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    Order(Hash),
    Transfer(Hash),
}

impl Job {
    fn key(self) -> Vec<u8> {
        let (kind, id) = match self {
            Self::Order(id) => (0, id),
            Self::Transfer(id) => (1, id),
        };
        [vec![kind], id.to_vec()].concat()
    }

    fn from_key(key: &[u8]) -> Self {
        let id = Hash::new(key[1..].try_into().expect("settlement job id"));
        match key[0] {
            0 => Self::Order(id),
            1 => Self::Transfer(id),
            _ => panic!("settlement job kind"),
        }
    }
}

thread_local! {
    static RUNNING: Cell<bool> = const { Cell::new(false) };
    // Deadline -> job, and job -> deadline, so rescheduling never duplicates work.
    static DUE: RefCell<StableBTreeMap<Vec<u8>, Stored<()>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(31)));
    static JOBS: RefCell<StableBTreeMap<Vec<u8>, Stored<u64>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(32)));
    static BACKFILL: RefCell<StableCell<Stored<Backfill>, Memory>> =
        RefCell::new(StableCell::init(store::memory(33), Stored(Backfill::default())));
}

fn due_key(key: &[u8], at: u64) -> Vec<u8> {
    [at.to_be_bytes().as_slice(), key].concat()
}

fn schedule(job: Job, at: Option<u64>) {
    let key = job.key();
    let old = JOBS.with_borrow(|t| t.load(&key));
    if old == at {
        return;
    }
    if let Some(old) = old {
        DUE.with_borrow_mut(|t| t.delete(&due_key(&key, old)));
        JOBS.with_borrow_mut(|t| t.delete(&key));
    }
    if let Some(at) = at {
        DUE.with_borrow_mut(|t| t.put(&due_key(&key, at), &()));
        JOBS.with_borrow_mut(|t| t.put(&key, &at));
    }
}

pub(crate) fn schedule_order(o: &Order, at: u64) {
    let unsettled = o.status == CheckoutStatus::Applied
        && !o.cancellation_pending
        && o.balances
            .get(&o.quote.cash.ledger)
            .is_some_and(|b| b.service > 0 || b.fees > 0);
    schedule(Job::Order(o.id), unsettled.then(|| o.service_end().max(at)));
}

pub(crate) fn schedule_transfer(t: &Transfer, at: u64) {
    let v = &t.view;
    let retryable = match v.status {
        CashTransferStatus::Pending => true,
        CashTransferStatus::InFlight | CashTransferStatus::Unknown => {
            at.saturating_sub(v.created_at_time_ns / 1_000_000) < DAY
                && v.error_code.as_deref() != Some("TooOld")
        }
        CashTransferStatus::Rejected => {
            at.saturating_sub(v.created_at_time_ns / 1_000_000) < DAY
                && matches!(
                    v.error_code.as_deref(),
                    Some("TemporarilyUnavailable" | "NotExecuted")
                )
        }
        CashTransferStatus::Succeeded | CashTransferStatus::Superseded => false,
    };
    let next = if v.status == CashTransferStatus::Pending {
        at
    } else {
        at.saturating_add(MINUTE)
    };
    schedule(Job::Transfer(v.transfer_id), retryable.then_some(next));
}

/// Only hot records can retain money or pending transfers. New writes are
/// indexed immediately, including writes behind a backfill cursor.
fn backfill(at: u64) {
    let mut cursor = BACKFILL.with_borrow(|t| t.get().0.clone());
    let (count, last) = match cursor.phase {
        0 => {
            let rows = checkout_store::settlement_orders(cursor.after);
            for o in &rows {
                schedule_order(o, at);
            }
            (rows.len(), rows.last().map(|o| o.id))
        }
        1 => {
            let rows = checkout_store::settlement_transfers(cursor.after);
            for t in &rows {
                schedule_transfer(t, at);
            }
            (rows.len(), rows.last().map(|t| t.view.transfer_id))
        }
        _ => return,
    };
    cursor.after = last;
    if count < BATCH {
        cursor.phase += 1;
        cursor.after = None;
    }
    BACKFILL.with_borrow_mut(|t| t.set(Stored(cursor)));
}

fn due(at: u64) -> Vec<(Job, u64)> {
    DUE.with_borrow(|t| {
        t.range(..=due_key(&[255; 33], at))
            .take(BATCH)
            .map(|row| {
                let key = row.key();
                (
                    Job::from_key(&key[8..]),
                    u64::from_be_bytes(key[..8].try_into().expect("settlement deadline")),
                )
            })
            .collect()
    })
}

/// Timers are recreated after upgrades; work and backfill cursors are durable.
/// One-shot timers schedule from the current time, without replaying every
/// missed minute after downtime. Per-transfer guards also exclude recovery calls.
pub(crate) fn start() {
    ic_cdk_timers::set_timer(Duration::from_secs(60), run());
}

struct Running;

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.with(|running| running.set(false));
    }
}

async fn run() {
    start();
    if RUNNING.with(|running| running.replace(true)) {
        return;
    }
    let _running = Running;
    let caller = ic_cdk::api::canister_self();
    let mut at = now();
    backfill(at);
    for (job, deadline) in due(at) {
        // A callback or a concurrent manual operation may already have changed it.
        if JOBS.with_borrow(|t| t.load(&job.key())) != Some(deadline) {
            continue;
        }
        // Commit a retry before any await. A failed job cannot starve later jobs.
        schedule(job, Some(at.saturating_add(MINUTE)));
        let result = match job {
            Job::Order(id) => checkout::settle_revenue(id, at),
            Job::Transfer(id) => {
                // Recheck an unknown transfer's retry window after downtime.
                let t = match checkout_store::transfer(id) {
                    Ok(t) => t,
                    Err(_) => {
                        schedule(job, None);
                        continue;
                    }
                };
                schedule_transfer(&t, at);
                if JOBS.with_borrow(|t| t.load(&job.key())).is_none() {
                    continue;
                }
                checkout::dispatch_transfer(id, caller, &mut at)
                    .await
                    .map(|_| ())
            }
        };
        if let Err(error) = result {
            if JOBS.with_borrow(|t| t.load(&job.key())).is_some() {
                schedule(job, Some(at.saturating_add(MINUTE)));
            }
            ic_cdk::println!("commerce settlement {job:?}: {error:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_move_behind_other_due_jobs_without_duplication() {
        let first = Job::Order(Hash::new([1; 32]));
        let second = Job::Order(Hash::new([2; 32]));
        schedule(first, Some(100));
        schedule(second, Some(101));
        assert_eq!(due(101), vec![(first, 100), (second, 101)]);
        schedule(first, Some(200));
        assert_eq!(due(101), vec![(second, 101)]);
        schedule(first, Some(200));
        assert_eq!(DUE.with_borrow(|t| t.len()), 2);
        schedule(second, None);
        assert_eq!(due(200), vec![(first, 200)]);
    }

    #[test]
    fn retries_preserve_unknowns_and_stop_at_definite_rejections_or_the_window() {
        use crate::checkout_model::{self, Balance, fixture};
        let at = fixture::base::NOW;
        let mut o = Order::new(fixture::base::principal(8), fixture::open());
        let ledger = o.quote.cash.ledger;
        o.balances.insert(ledger, Balance::default());
        let mut t = checkout_model::transfer(
            &mut o,
            ledger,
            fixture::base::principal(9).into(),
            100,
            10,
            20,
            at,
        )
        .unwrap();
        let job = Job::Transfer(t.view.transfer_id);
        t.view.status = CashTransferStatus::Unknown;
        t.view.error_code = Some("ExecutionUnknown".into());
        schedule_transfer(&t, at);
        assert_eq!(due(at + MINUTE), vec![(job, at + MINUTE)]);
        schedule_transfer(&t, at + DAY);
        assert!(due(at + 2 * DAY).is_empty());
        t.view.status = CashTransferStatus::Rejected;
        t.view.error_code = Some("BadFee".into());
        schedule_transfer(&t, at);
        assert!(due(at + MINUTE).is_empty());
        t.view.error_code = Some("TemporarilyUnavailable".into());
        schedule_transfer(&t, at);
        assert_eq!(due(at + MINUTE), vec![(job, at + MINUTE)]);
        t.view.status = CashTransferStatus::Succeeded;
        schedule_transfer(&t, at);
        assert!(due(at + MINUTE).is_empty());
    }
}
