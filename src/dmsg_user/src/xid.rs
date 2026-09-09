//! Persistent account allocation, based on TokenList's canister allocator.
//! The caller atomically commits the returned state alongside the new account.
use candid::Principal;
use dmsg_protocol::digest;
use dmsg_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Generator {
    pub profile_version: u8,
    pub namespace_digest: Hash,
    pub fingerprint: [u8; 5],
    pub last_second: Option<u32>,
    pub next_counter: u32,
}
impl Generator {
    pub fn new(environment: &Environment, namespace: &str, canister: Principal) -> Self {
        let namespace_digest = digest(
            "dmsg/account-id-generator/v1",
            &("dmsg", environment, namespace, canister),
        );
        Self {
            profile_version: 1,
            fingerprint: namespace_digest[..5].try_into().expect("five bytes"),
            namespace_digest,
            last_second: None,
            next_counter: 0,
        }
    }
    pub fn validate(
        &self,
        environment: &Environment,
        namespace: &str,
        canister: Principal,
    ) -> Result<()> {
        let expected = Self::new(environment, namespace, canister);
        ensure(
            self.profile_version == 1
                && self.namespace_digest == expected.namespace_digest
                && self.fingerprint == expected.fingerprint
                && self.next_counter <= 1 << 24
                && (self.last_second.is_some() || self.next_counter == 0),
            Error::IdGeneratorStateConflict,
        )
    }
    pub fn allocate(&self, seconds: u64) -> Result<(AccountId, Self)> {
        ensure(self.profile_version == 1, Error::IdGeneratorStateConflict)?;
        let second = u32::try_from(seconds).map_err(|_| Error::IdTimestampOutOfRange)?;
        let mut next = self.clone();
        if next.last_second.is_none_or(|last| second > last) {
            next.last_second = Some(second);
            next.next_counter = 0;
        }
        ensure(next.next_counter < 1 << 24, Error::IdCapacityExceeded)?;
        let mut bytes = [0; 12];
        bytes[..4].copy_from_slice(&next.last_second.expect("allocated second").to_be_bytes());
        bytes[4..9].copy_from_slice(&next.fingerprint);
        bytes[9..].copy_from_slice(&next.next_counter.to_be_bytes()[1..]);
        next.next_counter += 1;
        Ok((AccountId::new(bytes), next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn generator() -> Generator {
        Generator::new(
            &Environment::Local,
            "https://dmsg.test/u/",
            Principal::from_slice(&[1]),
        )
    }
    #[test]
    fn rollback_and_upgrade_do_not_reuse_ids() {
        let (a, g) = generator().allocate(100).unwrap();
        let g: Generator = cbor2::from_slice(&cbor2::to_vec(&g).unwrap()).unwrap();
        let (b, g) = g.allocate(90).unwrap();
        let (c, g) = g.allocate(100).unwrap();
        let (d, g) = g.allocate(101).unwrap();
        assert!(a < b && b < c && c < d);
        assert_eq!(g.last_second, Some(101));
        assert_eq!(g.next_counter, 1);
    }
    #[test]
    fn exhausted_counters_and_changed_namespaces_fail_without_mutation() {
        let mut g = generator();
        g.last_second = Some(100);
        g.next_counter = (1 << 24) - 1;
        let (last, full) = g.allocate(100).unwrap();
        assert_eq!(&last[9..], &[255; 3]);
        assert_eq!(full.allocate(100), Err(Error::IdCapacityExceeded));
        assert_eq!(full.allocate(99), Err(Error::IdCapacityExceeded));
        assert!(full.allocate(101).is_ok());
        assert_eq!(
            full.allocate(u32::MAX as u64 + 1),
            Err(Error::IdTimestampOutOfRange)
        );
        assert_eq!(g.next_counter, (1 << 24) - 1);
        assert!(g
            .validate(
                &Environment::Production,
                "https://dmsg.test/u/",
                Principal::from_slice(&[1])
            )
            .is_err());
        assert!(g
            .validate(
                &Environment::Local,
                "https://other.test/u/",
                Principal::from_slice(&[1])
            )
            .is_err());
        assert!(g
            .validate(
                &Environment::Local,
                "https://dmsg.test/u/",
                Principal::from_slice(&[2])
            )
            .is_err());
        g.profile_version = 9;
        assert_eq!(g.allocate(100), Err(Error::IdGeneratorStateConflict));
    }
}
