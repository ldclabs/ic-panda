use super::*;
use crate::vault::Field;

#[test]
fn reads_dates_and_months() {
    assert_eq!(days("1970-01-01"), Some(0));
    assert_eq!(days("2000-03-01"), Some(11_017));
    assert_eq!(days("2026-10-18"), Some(20_744));
    assert_eq!(days("2024-02-29"), Some(19_782));
    assert_eq!(month_end("2024-02"), days("2024-02-29"));
    for wrong in [
        "2023-02-29",
        "2026-1-01",
        "2026-13-01",
        "+026-01-01",
        "2026-01",
        "",
    ] {
        assert_eq!(days(wrong), None, "{wrong}");
    }
    for wrong in ["2026-00", "2026-1", "2026-01-01", "x"] {
        assert_eq!(month_end(wrong), None, "{wrong}");
    }
}

#[test]
fn rates_passwords() {
    for weak_one in [
        "password",
        "short",
        "aaaaaaaaaaaa",
        "abcdefghij",
        "Password1",
    ] {
        assert!(weak(weak_one), "{weak_one}");
    }
    for strong in [
        "correct horse battery staple",
        "Tr0ub4dor&3x!",
        "q7-Lm2.vX9_kP4!z",
    ] {
        assert!(!weak(strong), "{strong}");
    }
}

#[test]
fn finds_weak_reused_insecure_and_expiring() {
    let now = 20_744 * 86_400_000;
    let item = |kind: &str, fields: Vec<Field>| {
        let mut item = Item::new(kind, "x", 1);
        item.fields = fields;
        item
    };
    let items = vec![
        (
            "a".to_string(),
            item(
                "login",
                vec![
                    Field::new("password", "concealed", "hunter2"),
                    Field::new("url", "url", "http://example.com"),
                ],
            ),
        ),
        (
            "b".to_string(),
            item(
                "server",
                vec![
                    Field::new("password", "concealed", "hunter2"),
                    Field::new("url", "url", "http://localhost:8080"),
                ],
            ),
        ),
        (
            "c".to_string(),
            item(
                "card",
                vec![
                    Field::new("expiry", "month", "2026-09"),
                    Field::new("number", "concealed", "hunter2"),
                ],
            ),
        ),
        (
            "d".to_string(),
            item("license", vec![Field::new("expires", "date", "2026-11-01")]),
        ),
        (
            "e".to_string(),
            item("license", vec![Field::new("expires", "date", "2027-01-01")]),
        ),
    ];
    let mut found: Vec<(String, String, &str)> = findings(&items, now)
        .into_iter()
        .map(|f| (f.id, f.field, f.issue))
        .collect();
    found.sort();
    let expected = [
        ("a", "password", "reused"),
        ("a", "password", "weak"),
        ("a", "url", "insecure"),
        ("b", "password", "reused"),
        ("b", "password", "weak"),
        ("c", "expiry", "expired"),
        ("d", "expires", "expiring"),
    ];
    assert_eq!(
        found,
        expected
            .iter()
            .map(|(a, b, c)| (a.to_string(), b.to_string(), *c))
            .collect::<Vec<_>>()
    );
}
