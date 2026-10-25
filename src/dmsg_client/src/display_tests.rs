use crate::{display::*, testing};
use dmsg_types::{AccountId, Error};

fn profile(version: u64, now: u64) -> Profile {
    Profile {
        name: Some("private-display-marker".into()),
        avatar_url: Some("https://example.test/private-image-marker".into()),
        updated_at: Some(version),
        observed_at_ms: now,
    }
}

#[test]
fn publication_and_avatar_never_roll_back_or_cross_urls() {
    let dir = testing::TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    assert!(c.keep_profile_display(profile(20, 100)).unwrap());
    let avatar = Avatar {
        url: profile(20, 100).avatar_url.unwrap(),
        mime: "image/webp".into(),
        bytes: b"RIFF0000WEBPimage".to_vec(),
        fetched_at_ms: 100,
    };
    assert!(c.keep_profile_avatar(avatar.clone()).unwrap());
    assert!(!c.keep_profile_display(profile(10, 200)).unwrap());
    assert!(
        !c.keep_profile_display(Profile::from_document(None, 300))
            .unwrap()
    );
    assert_eq!(c.profile_display().unwrap().unwrap().observed_at_ms, 100);
    assert!(!c.keep_profile_display(profile(20, 400)).unwrap());
    assert_eq!(c.profile_display().unwrap().unwrap().observed_at_ms, 400);
    let mut next = profile(30, 500);
    next.avatar_url = Some("https://example.test/new".into());
    c.keep_profile_display(next).unwrap();
    assert!(c.profile_avatar(&avatar.url).unwrap().is_none());
    assert!(!c.keep_profile_avatar(avatar).unwrap());
}

#[test]
fn names_evict_oldest_and_reject_stale_absence() {
    let dir = testing::TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    let found = (0u32..2100)
        .map(|n| {
            let mut id = [0u8; 12];
            id[8..].copy_from_slice(&n.to_be_bytes());
            (
                AccountId(id),
                Name {
                    name: Some("name".into()),
                    updated_at: Some(2),
                    observed_at_ms: u64::from(n),
                },
            )
        })
        .collect();
    c.keep_public_names(found).unwrap();
    let names = c.public_names().unwrap();
    assert_eq!(names.len(), 2048);
    assert!(names.values().all(|n| n.observed_at_ms >= 52));
    let id = *names.keys().next().unwrap();
    c.keep_public_names(vec![(
        id,
        Name {
            name: None,
            updated_at: None,
            observed_at_ms: 5000,
        },
    )])
    .unwrap();
    assert_eq!(c.public_names().unwrap()[&id], names[&id]);
}

#[test]
fn freshness_handles_clock_rollback_without_extending_observations() {
    assert!(!stale(1000, 600, 1599));
    assert!(stale(1000, 600, 1600));
    assert!(!stale(1000, 600, 900));
    assert!(stale(500_000, 600, 1000));
    assert!(
        Name {
            name: None,
            updated_at: Some(1),
            observed_at_ms: 1
        }
        .stale(60_001)
    );
}

#[test]
fn corrupt_records_are_discarded_independently_and_locked_records_do_not_open() {
    let dir = testing::TempDir::new();
    let mut c = testing::unlocked(&dir.0, false, None);
    c.keep_profile_display(profile(1, 1)).unwrap();
    c.workspace.store.set_sealed(NAMES, "corrupt").unwrap();
    assert!(c.public_names().unwrap().is_empty());
    assert!(c.workspace.store.sealed(NAMES).unwrap().is_none());
    assert!(c.profile_display().unwrap().is_some());
    let session = c.display_session().unwrap();
    c.lock();
    assert_eq!(c.profile_display().unwrap_err(), Error::Locked);
    assert_eq!(c.public_names().unwrap_err(), Error::Locked);
    assert!(c.check_display_session(session).is_err());
}

#[test]
fn database_and_wal_contain_no_display_plaintext() {
    let dir = testing::TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    c.keep_profile_display(profile(1, 1)).unwrap();
    let peer = AccountId([13; 12]);
    c.keep_public_names(vec![(
        peer,
        Name {
            name: Some("private-peer-marker".into()),
            updated_at: Some(1),
            observed_at_ms: 1,
        },
    )])
    .unwrap();
    let peer_text = peer.to_string();
    let markers = [
        b"private-display-marker".as_slice(),
        b"private-image-marker".as_slice(),
        b"private-peer-marker".as_slice(),
        peer_text.as_bytes(),
        peer.as_slice(),
    ];
    fn scan(path: &std::path::Path, markers: &[&[u8]]) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                scan(&path, markers);
            } else {
                let bytes = std::fs::read(path).unwrap();
                for marker in markers {
                    assert!(!bytes.windows(marker.len()).any(|s| s == *marker));
                }
            }
        }
    }
    scan(&dir.0, &markers);
}

#[test]
fn failed_profile_replacement_rolls_back_profile_and_avatar_together() {
    let dir = testing::TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    let old = profile(1, 1);
    c.keep_profile_display(old.clone()).unwrap();
    let avatar = Avatar {
        url: old.avatar_url.clone().unwrap(),
        mime: "image/webp".into(),
        bytes: b"RIFF0000WEBPold".to_vec(),
        fetched_at_ms: 1,
    };
    c.keep_profile_avatar(avatar.clone()).unwrap();
    let db = rusqlite::Connection::open(
        dir.0
            .join("accounts")
            .join(testing::account().id.to_string())
            .join("workspace.sqlite"),
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER fail_avatar_delete BEFORE DELETE ON sealed WHEN old.key='profile-avatar' BEGIN SELECT RAISE(FAIL, 'forced failure'); END;").unwrap();
    let mut next = profile(2, 2);
    next.avatar_url = Some("https://example.test/new".into());
    assert!(c.keep_profile_display(next).is_err());
    assert_eq!(c.profile_display().unwrap(), Some(old));
    assert_eq!(c.profile_avatar(&avatar.url).unwrap(), Some(avatar));
}
