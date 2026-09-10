//! dMsg namespace validation and error mapping for the shared Xid generator.
//! The caller atomically commits the returned state alongside the new account.
use candid::Principal;
use dmsg_protocol::digest;
use dmsg_types::*;
use ic_auth_types::{XidGenerator, XidGeneratorError};

pub(crate) fn namespace_digest(
    environment: &Environment,
    namespace: &str,
    canister: Principal,
) -> Hash {
    digest(
        "dmsg/account-id-generator/v1",
        &("dmsg", environment, namespace, canister),
    )
}

pub(crate) fn validate(
    generator: &XidGenerator,
    stored_namespace_digest: Hash,
    environment: &Environment,
    namespace: &str,
    canister: Principal,
) -> Result<()> {
    let expected = namespace_digest(environment, namespace, canister);
    ensure(
        generator.profile_version == 1
            && stored_namespace_digest == expected
            && generator.fingerprint == expected[..5]
            && generator.next_counter <= 1 << 24
            && (generator.last_second.is_some() || generator.next_counter == 0),
        Error::IdGeneratorStateConflict,
    )
}

pub(crate) fn allocation_error(error: XidGeneratorError) -> Error {
    match error {
        XidGeneratorError::StateConflict => Error::IdGeneratorStateConflict,
        XidGeneratorError::TimestampOutOfRange => Error::IdTimestampOutOfRange,
        XidGeneratorError::CapacityExceeded => Error::IdCapacityExceeded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMESPACE: &str = "https://dmsg.test/u/";

    fn generator() -> (Hash, XidGenerator) {
        let digest = namespace_digest(&Environment::Local, NAMESPACE, Principal::from_slice(&[1]));
        (digest, XidGenerator::new(digest[..5].try_into().unwrap()))
    }

    #[test]
    fn rollback_and_upgrade_do_not_reuse_ids() {
        let (digest, g) = generator();
        let (a, g) = g.allocate(100).unwrap();
        let (digest, g): (Hash, XidGenerator) =
            cbor2::from_slice(&cbor2::to_vec(&(digest, g)).unwrap()).unwrap();
        validate(
            &g,
            digest,
            &Environment::Local,
            NAMESPACE,
            Principal::from_slice(&[1]),
        )
        .unwrap();
        let (b, g) = g.allocate(90).unwrap();
        let (c, g) = g.allocate(100).unwrap();
        let (d, g) = g.allocate(101).unwrap();
        assert!(a < b && b < c && c < d);
        assert_eq!(g.last_second, Some(101));
        assert_eq!(g.next_counter, 1);
    }

    #[test]
    fn allocation_failures_map_to_dmsg_errors_without_mutation() {
        let (_, mut g) = generator();
        g.last_second = Some(100);
        g.next_counter = (1 << 24) - 1;
        let (last, full) = g.allocate(100).unwrap();
        assert_eq!(&last[9..], &[255; 3]);
        assert_eq!(
            full.allocate(100).map_err(allocation_error),
            Err(Error::IdCapacityExceeded)
        );
        assert_eq!(
            full.allocate(99).map_err(allocation_error),
            Err(Error::IdCapacityExceeded)
        );
        assert!(full.allocate(101).is_ok());
        assert_eq!(
            full.allocate(u32::MAX as u64 + 1).map_err(allocation_error),
            Err(Error::IdTimestampOutOfRange)
        );
        assert_eq!(g.next_counter, (1 << 24) - 1);
        assert_eq!(full.next_counter, 1 << 24);
        g.profile_version = 9;
        assert_eq!(
            g.allocate(100).map_err(allocation_error),
            Err(Error::IdGeneratorStateConflict)
        );
    }

    #[test]
    fn changed_namespaces_and_invalid_state_are_rejected() {
        let (digest, g) = generator();
        let canister = Principal::from_slice(&[1]);
        for (environment, namespace, home) in [
            (Environment::Production, NAMESPACE, canister),
            (Environment::Local, "https://other.test/u/", canister),
            (Environment::Local, NAMESPACE, Principal::from_slice(&[2])),
        ] {
            assert_eq!(
                validate(&g, digest, &environment, namespace, home),
                Err(Error::IdGeneratorStateConflict)
            );
        }
        // The full digest must match even when the five-byte fingerprint matches.
        let mut changed_digest = *digest;
        changed_digest[31] ^= 1;
        assert_eq!(
            validate(
                &g,
                Hash::new(changed_digest),
                &Environment::Local,
                NAMESPACE,
                canister
            ),
            Err(Error::IdGeneratorStateConflict)
        );
        for invalid in [
            XidGenerator {
                profile_version: 9,
                ..g.clone()
            },
            XidGenerator {
                fingerprint: [0; 5],
                ..g.clone()
            },
            XidGenerator {
                next_counter: 1 << 24 | 1,
                last_second: Some(100),
                ..g.clone()
            },
            XidGenerator {
                next_counter: 1,
                ..g.clone()
            },
        ] {
            assert_eq!(
                validate(&invalid, digest, &Environment::Local, NAMESPACE, canister),
                Err(Error::IdGeneratorStateConflict)
            );
        }
        let exhausted = XidGenerator {
            next_counter: 1 << 24,
            last_second: Some(100),
            ..g
        };
        assert!(validate(&exhausted, digest, &Environment::Local, NAMESPACE, canister).is_ok());
    }
}
