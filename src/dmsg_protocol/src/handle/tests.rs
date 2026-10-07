use super::*;

#[test]
fn handles_normalize_only_ascii_and_enforce_limits() {
    for (input, expected) in [("A", "a"), ("ALIce_01", "alice_01"), ("9_", "9_")] {
        assert_eq!(normalize_handle(input).unwrap(), expected);
        assert_eq!(normalize_handle(expected).unwrap(), expected);
    }
    assert!(normalize_handle(&"A".repeat(19)).is_ok());
    for input in [
        "",
        "_alice",
        "alice-bob",
        "a b",
        "a/b",
        "a.b",
        "å",
        "中",
        "a\0",
    ] {
        assert!(normalize_handle(input).is_err(), "{input:?}");
    }
    assert!(normalize_handle(&"a".repeat(20)).is_err());
    // A handle must never read as an AccountId's 20-character Xid text.
    let account = AccountId([7; 12]).to_string();
    assert_eq!(account.len(), 20);
    assert!(normalize_handle(&account).is_err());
}

#[test]
fn valid_handle_price_boundaries_use_smallest_token_units() {
    for (len, tokens) in [
        (1, 1_000_000),
        (2, 200_000),
        (3, 50_000),
        (4, 50_000),
        (5, 20_000),
        (6, 20_000),
        (7, 100),
        (19, 100),
    ] {
        assert_eq!(price(&"a".repeat(len)), tokens * 100_000_000);
    }
    assert_eq!(price(&"a".repeat(19)), MIN_HANDLE_PRICE);
}

#[test]
fn handle_buckets_take_the_leading_digest_bits() {
    // dmsg_app's handle tests pin the same values.
    assert_eq!(handle_bucket("alice"), 768_019);
    assert_eq!(handle_bucket("panda"), 566_360);
    for name in ["alice", "panda", "a", &"z".repeat(19)] {
        let d = digest("dmsg/handle-bucket/v1", &name);
        let bits = u32::from_be_bytes(d.as_slice()[..4].try_into().unwrap()) >> 12;
        assert_eq!(handle_bucket(name), bits);
        assert!(handle_bucket(name) < 1 << HANDLE_BUCKET_BITS);
    }
}

#[test]
fn charge_digest_binds_ledger_payer_amount_and_fee() {
    let ledger = Principal::from_slice(&[1]);
    let payer = Account {
        owner: Principal::from_slice(&[2]),
        subaccount: None,
    };
    let expected = charge_terms_digest(ledger, &payer, 1, 2);
    assert_ne!(
        charge_terms_digest(Principal::from_slice(&[3]), &payer, 1, 2),
        expected
    );
    assert_ne!(
        charge_terms_digest(
            ledger,
            &Account {
                owner: ledger,
                ..payer
            },
            1,
            2
        ),
        expected
    );
    assert_ne!(
        charge_terms_digest(
            ledger,
            &Account {
                subaccount: Some([1; 32]),
                ..payer
            },
            1,
            2
        ),
        expected
    );
    assert_ne!(
        charge_terms_digest(ledger, &payer, 1 + (1u128 << 100), 2),
        expected
    );
    assert_ne!(charge_terms_digest(ledger, &payer, 1, 3), expected);
}
