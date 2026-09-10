use super::*;

#[test]
fn uri_validation_preserves_canonical_identifiers() {
    for value in [
        "https://example.com/",
        "https://example.com:8443/a?b=c#d",
        "https://[::1]/",
        "https://xn--bcher-kva.example/%E4%B8%AD",
        "urn:example:person",
        "did:example:alice",
        "mailto:alice@example.com",
        "https://example.com/%2f%2F",
    ] {
        assert_eq!(validate_uri(value), Ok(()), "{value}");
    }
    for value in [
        "",
        "relative",
        "//example.com/",
        "https://example.com",
        "HTTPS://example.com/",
        "https://EXAMPLE.com/",
        "https://example.com:443/",
        "https://example.com/a/../b",
        "https://example.com/a\\b",
        "https://user@example.com/",
        "https://user:pass@example.com/",
        "https://example.com/中",
        "https://example.com/ ",
        "urn:test:\n",
        "urn:test:\0",
        "urn:test:\u{7f}",
        "https://example.com/%",
        "https://example.com/%0",
        "https://example.com/%xz",
    ] {
        assert!(validate_uri(value).is_err(), "{value:?}");
    }
}

#[test]
fn namespaces_and_issuer_roundtrips_cover_uri_forms() {
    for namespace in [
        "https://dmsg.test/u/",
        "https://[::1]:8443/u/",
        "urn:dmsg:",
        "did:dmsg:",
        "custom:",
        "custom:/",
        "custom://",
        "custom:///",
        "custom://host/",
        "custom://host/path:",
        "file:///accounts/",
    ] {
        validate_namespace(namespace).unwrap();
        for account in [AccountId([0; 12]), AccountId([1; 12]), AccountId([255; 12])] {
            let issuer = account_issuer(namespace, &account).unwrap();
            assert_eq!(issuer, format!("{namespace}{account}"));
            validate_uri(&issuer).unwrap();
            assert_eq!(parse_account_issuer(namespace, &issuer), Ok(account));
        }
        for principal in [
            Principal::management_canister(),
            Principal::anonymous(),
            Principal::from_slice(&[255; 29]),
        ] {
            let issuer = principal_issuer(namespace, principal).unwrap();
            assert_eq!(issuer, format!("{namespace}{}", principal.to_text()));
            validate_uri(&issuer).unwrap();
        }
    }
}

#[test]
fn namespaces_reject_ambiguous_suffixes() {
    for namespace in [
        "https://dmsg.test/u",
        "urn:dmsg",
        "https://dmsg.test/u/?q=/",
        "https://dmsg.test/u/#/",
        "https://dmsg.test/u/?",
        "https://user@dmsg.test/u/",
    ] {
        assert!(validate_namespace(namespace).is_err(), "{namespace}");
        assert!(account_issuer(namespace, &AccountId([1; 12])).is_err());
        assert!(principal_issuer(namespace, Principal::anonymous()).is_err());
        assert!(parse_account_issuer(namespace, "urn:dmsg:00000000000000000000").is_err());
    }
}

#[test]
fn account_parsing_requires_exact_namespace_and_canonical_xid() {
    let namespace = "https://dmsg.test/u/";
    let account = AccountId([255; 12]);
    let issuer = account_issuer(namespace, &account).unwrap();
    for value in [
        issuer.replace("dmsg.test", "other.test"),
        format!("{issuer}/"),
        format!("{issuer}?"),
        format!("{issuer}#"),
        format!("{namespace}{}", account.to_string().to_ascii_uppercase()),
        format!("{namespace}{}v", &account.to_string()[..19]),
        format!("{namespace}0000000000000000000"),
        format!("{namespace}000000000000000000000"),
        format!("{namespace}%300000000000000000000"),
        format!("{namespace}aaaaa-aa"),
        namespace.into(),
    ] {
        assert!(parse_account_issuer(namespace, &value).is_err(), "{value}");
    }
}

#[test]
fn uri_and_namespace_byte_limits_allow_the_boundary() {
    let uri = format!("urn:{}", "a".repeat(MAX_URI_BYTES - 4));
    assert_eq!(validate_uri(&uri), Ok(()));
    assert!(validate_uri(&format!("{uri}a")).is_err());
    let namespace = format!("urn:{}:", "a".repeat(MAX_URI_BYTES - 64 - 5));
    assert_eq!(validate_namespace(&namespace), Ok(()));
    let account = AccountId([255; 12]);
    let issuer = account_issuer(&namespace, &account).unwrap();
    assert_eq!(parse_account_issuer(&namespace, &issuer), Ok(account));
    validate_uri(&principal_issuer(&namespace, Principal::from_slice(&[255; 29])).unwrap())
        .unwrap();
    assert!(validate_namespace(&format!("{namespace}:")).is_err());
}

#[test]
fn browser_origins_require_exact_https_or_extension_origins() {
    for origin in [
        "https://example.com",
        "https://example.com:8443",
        "https://[::1]",
        "https://xn--bcher-kva.example",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop",
    ] {
        assert_eq!(validate_origin(origin), Ok(()), "{origin}");
    }
    for origin in [
        "",
        "null",
        "http://example.com",
        "https://example.com/",
        "https://EXAMPLE.com",
        "https://example.com:443",
        "https://user@example.com",
        "https://example.com?",
        "https://example.com#",
        "https://example.com/path",
        "https://example.com\n",
        " https://example.com",
        "https://bücher.example",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop/",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnoq",
        "chrome-extension://abcdefghijklmnopabcdefghijklmno",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnopa",
        "chrome-extension://ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP",
    ] {
        assert!(validate_origin(origin).is_err(), "{origin:?}");
    }
    let origin = format!("https://{}", "a".repeat(256 - 8));
    assert_eq!(validate_origin(&origin), Ok(()));
    assert!(validate_origin(&format!("{origin}a")).is_err());
}
