//! A live callback owns its operation. An upgrade leaves only the durable state,
//! so an interrupted transfer can still be retried with its original parameters.
use dmsg_types::*;
use std::{cell::RefCell, collections::BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Funding(Hash),
    Transfer(Hash),
    Product(Hash),
    Entitlement(Hash),
}

thread_local! {
    static CALLS: RefCell<BTreeSet<Key>> = const { RefCell::new(BTreeSet::new()) };
}

pub(crate) struct CallGuard(Key);

impl CallGuard {
    fn take(key: Key) -> Result<Self> {
        CALLS.with_borrow_mut(|s| -> Result<()> {
            ensure(!s.contains(&key), Error::Pending)?;
            ensure(s.len() < 128, Error::QuotaExceeded)?;
            s.insert(key);
            Ok(())
        })?;
        Ok(Self(key))
    }

    pub fn funding(id: Hash) -> Result<Self> {
        Self::take(Key::Funding(id))
    }

    pub fn transfer(id: Hash) -> Result<Self> {
        Self::take(Key::Transfer(id))
    }

    pub fn product(id: Hash) -> Result<Self> {
        Self::take(Key::Product(id))
    }

    pub fn entitlement(id: Hash) -> Result<Self> {
        Self::take(Key::Entitlement(id))
    }
}

impl Drop for CallGuard {
    fn drop(&mut self) {
        CALLS.with_borrow_mut(|s| s.remove(&self.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_are_exclusive_until_the_owner_returns() {
        let id = Hash::new([1; 32]);
        let guard = CallGuard::transfer(id).unwrap();
        assert!(matches!(CallGuard::transfer(id), Err(Error::Pending)));
        assert!(CallGuard::funding(id).is_ok());
        drop(guard);
        assert!(CallGuard::transfer(id).is_ok());
    }
}
