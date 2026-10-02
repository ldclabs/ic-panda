use dmsg_types::{ensure, Error, Hash, Result};
use std::{cell::RefCell, collections::BTreeSet};

thread_local! {
    static CHARGES: RefCell<BTreeSet<Hash>> = const { RefCell::new(BTreeSet::new()) };
}

// A durable Charging phase may outlive its callback. This guard tracks only
// live calls, drops on CDK cancellation, and disappears on upgrade. The number
// of guards is bounded by the registry's durable pending-operation limit.
pub(crate) struct ChargeGuard(Hash);

impl ChargeGuard {
    pub(crate) fn acquire(key: Hash) -> Result<Self> {
        CHARGES.with_borrow_mut(|calls| {
            ensure(calls.insert(key), Error::Pending)?;
            Ok(Self(key))
        })
    }
}

impl Drop for ChargeGuard {
    fn drop(&mut self) {
        CHARGES.with_borrow_mut(|calls| calls.remove(&self.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_duplicate_does_not_release_the_live_call() {
        let key = Hash::new([1; 32]);
        let call = ChargeGuard::acquire(key).unwrap();
        for _ in 0..2 {
            assert!(matches!(ChargeGuard::acquire(key), Err(Error::Pending)));
        }
        drop(call);
        assert!(ChargeGuard::acquire(key).is_ok());
    }
}
