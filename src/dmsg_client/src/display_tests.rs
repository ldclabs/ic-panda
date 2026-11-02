use crate::{display::*, testing};
use dmsg_types::{AccountId, Error};

#[tokio::test]
async fn participant_images_cache_success_absence_and_failures_until_explicit_clear() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let status = Arc::new(AtomicUsize::new(200));
    let server = tokio::spawn({
        let calls = calls.clone();
        let status = status.clone();
        async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                socket.read(&mut request).await.unwrap();
                calls.fetch_add(1, Ordering::SeqCst);
                let body = b"RIFF0000WEBPfixture";
                let head = format!(
                    "HTTP/1.1 {} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    status.load(Ordering::SeqCst),
                    body.len()
                );
                socket.write_all(head.as_bytes()).await.unwrap();
                socket.write_all(body).await.unwrap();
            }
        }
    });
    let dir = testing::TempDir::new();
    let mut c = testing::unlocked(&dir.0, false, None);
    testing::relay_to(&mut c, &origin);
    let url = format!("{origin}/peer.webp");
    let image = participant_avatar(&c.hold(), &url).await.unwrap().unwrap();
    assert_eq!(
        participant_avatar(&c.hold(), &url).await.unwrap(),
        Some(image.clone())
    );
    assert_eq!(
        c.participant_avatar_cached(&url).unwrap(),
        Some(image.clone())
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    status.store(503, Ordering::SeqCst);
    c.cache_invalidate(&[crate::cache::Kind::Avatar]).unwrap();
    assert_eq!(
        participant_avatar(&c.hold(), &url).await.unwrap(),
        Some(image.clone())
    );
    assert_eq!(
        participant_avatar(&c.hold(), &url).await.unwrap(),
        Some(image)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    status.store(404, Ordering::SeqCst);
    let missing = format!("{origin}/missing.webp");
    assert!(
        participant_avatar(&c.hold(), &missing)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        participant_avatar(&c.hold(), &missing)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    c.cache_clear().unwrap();
    assert!(c.participant_avatar_cached(&url).unwrap().is_none());
    status.store(200, Ordering::SeqCst);
    assert!(participant_avatar(&c.hold(), &url).await.unwrap().is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    c.lock();
    assert!(c.participant_avatar_cached(&url).is_err());
    server.abort();
}

fn profile(version: u64, now: u64) -> Profile {
    Profile {
        name: Some("private-display-marker".into()),
        avatar_url: Some("https://example.test/private-image-marker".into()),
        updated_at: Some(version),
        observed_at_ms: now,
    }
}

#[test]
fn participant_avatars_survive_cache_updates_and_old_records() {
    let old: Name = serde_json::from_value(serde_json::json!({
        "name":"Peer", "updated_at":1, "observed_at_ms":1
    }))
    .unwrap();
    assert!(old.avatar_url.is_none());
    let dir = testing::TempDir::new();
    let c = testing::unlocked(&dir.0, false, None);
    let id = AccountId([1; 12]);
    c.keep_public_names(vec![(id, old.clone())]).unwrap();
    let mut current = old.clone();
    current.avatar_url = Some("https://assets.example.test/peer.png".into());
    current.updated_at = Some(2);
    assert!(c.keep_public_names(vec![(id, current.clone())]).unwrap());
    assert!(!c.keep_public_names(vec![(id, old)]).unwrap());
    assert_eq!(c.public_names().unwrap()[&id], current);
    current.avatar_url = None;
    current.updated_at = Some(3);
    assert!(c.keep_public_names(vec![(id, current)]).unwrap());
    assert!(c.public_names().unwrap()[&id].avatar_url.is_none());
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
                    avatar_url: None,
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
            avatar_url: None,
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
            avatar_url: None,
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
            avatar_url: None,
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
