//! Bound and coalesce read-only cross-canister work. No money state lives here.
use candid::Principal;
use dmsg_types::{ensure, Error, Hash, Result};
use std::{cell::RefCell, collections::BTreeSet};

const MAX_IN_FLIGHT: usize = 128;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Request {
    Payer(Principal),
    Quote(Hash),
    Funding(u64),
    Reconcile(Hash, u64),
}

thread_local! {
    static IN_FLIGHT: RefCell<BTreeSet<Request>> = const { RefCell::new(BTreeSet::new()) };
}

// CDK cancellation drops the guard on callback traps as well as normal errors.
// Upgrades discard these read-only tasks and guards; durable outbox state is
// independent and must never be unlocked or retried merely by dropping a guard.
pub(crate) struct CallGuard(Vec<Request>);

impl CallGuard {
    fn acquire(keys: Vec<Request>) -> Result<Self> {
        IN_FLIGHT.with_borrow_mut(|pending| {
            ensure(!keys.iter().any(|k| pending.contains(k)), Error::Pending)?;
            ensure(
                pending.len() + keys.len() <= MAX_IN_FLIGHT,
                Error::QuotaExceeded,
            )?;
            pending.extend(keys.iter().cloned());
            Ok(Self(keys))
        })
    }

    pub(crate) fn open(payer: Principal, quote: Hash) -> Result<Self> {
        Self::acquire(vec![Request::Payer(payer), Request::Quote(quote)])
    }

    pub(crate) fn funding(block: u64) -> Result<Self> {
        Self::acquire(vec![Request::Funding(block)])
    }

    pub(crate) fn reconcile(id: Hash, leg: u64) -> Result<Self> {
        Self::acquire(vec![Request::Reconcile(id, leg)])
    }
}

impl Drop for CallGuard {
    fn drop(&mut self) {
        IN_FLIGHT.with_borrow_mut(|pending| {
            for key in &self.0 {
                pending.remove(key);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_and_capacity_failures_do_not_leak_reservations() {
        let calls: Vec<_> = (0..MAX_IN_FLIGHT as u64)
            .map(|block| CallGuard::funding(block).unwrap())
            .collect();
        assert!(matches!(CallGuard::funding(0), Err(Error::Pending)));
        assert!(matches!(CallGuard::funding(128), Err(Error::QuotaExceeded)));
        drop(calls);
        assert!(CallGuard::funding(0).is_ok());
        assert!(CallGuard::funding(128).is_ok());
    }

    #[test]
    fn open_reserves_payer_and_quote_atomically() {
        let p = Principal::self_authenticating([1]);
        let other = Principal::self_authenticating([2]);
        let q = Hash::new([1; 32]);
        let other_q = Hash::new([2; 32]);
        let first = CallGuard::open(p, q).unwrap();
        assert!(matches!(CallGuard::open(p, other_q), Err(Error::Pending)));
        assert!(matches!(CallGuard::open(other, q), Err(Error::Pending)));
        let second = CallGuard::open(other, other_q).unwrap();
        drop((first, second));
        assert!(CallGuard::open(p, q).is_ok());
    }
}
