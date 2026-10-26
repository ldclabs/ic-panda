use super::*;
use crate::store::LOCAL;
use crate::testing::{TempDir, unlocked};

fn login(title: &str) -> Changes {
    Changes {
        title: Some(title.into()),
        tags: Some(vec!["work".into()]),
        notes: Some("notes".into()),
        set: Some(vec![
            FieldChange {
                id: Some("username".into()),
                value: Some("alice".into()),
                ..FieldChange::default()
            },
            FieldChange {
                id: Some("password".into()),
                value: Some("s3cret".into()),
                ..FieldChange::default()
            },
        ]),
        ..Changes::default()
    }
}

fn item(title: &str) -> Item {
    let mut item = Item::new("note", title, 1);
    item.tags = vec!["work".into()];
    item
}

#[test]
fn items_have_limits() {
    assert!(item("x").validate().is_ok());
    assert!(item(" ").validate().is_err());
    assert!(item(&"x".repeat(201)).validate().is_err());
    let mut kind = item("x");
    kind.kind = "file".into();
    assert!(kind.validate().is_err());

    let mut tags = item("x");
    tags.tags = vec![" a ".into(), "A".into(), "".into(), "b".into()];
    assert!(tags.validate().is_err());
    tags.normalize();
    assert_eq!(tags.tags, ["a", "b"]);
    assert!(tags.validate().is_ok());

    let mut big = item("x");
    big.notes = "x".repeat(20_000);
    big.fields = (0..5)
        .map(|i| Field::new(&format!("f{i}"), "text", &"y".repeat(10_000)))
        .collect();
    assert_eq!(big.validate(), Err(Error::QuotaExceeded));
    big.fields.truncate(4);
    assert!(big.validate().is_ok());

    for (id, kind, value) in [
        ("Bad", "text", ""),
        ("ok", "file", ""),
        ("ok", "otp", "not base32!"),
        ("ok", "date", "2026-02-30"),
        ("ok", "month", "2026-13"),
    ] {
        let mut wrong = item("x");
        wrong.fields.push(Field::new(id, kind, value));
        assert!(wrong.validate().is_err(), "{id} {kind} {value}");
    }
    let mut twice = item("x");
    twice.fields = vec![Field::new("a", "text", ""), Field::new("a", "text", "")];
    assert!(twice.validate().is_err());
    let mut files = item("x");
    files.files = vec!["ab".into()];
    assert!(files.validate().is_err());
    files.files = vec!["ab".repeat(32)];
    assert!(files.validate().is_ok());
}

#[test]
fn templates_and_the_previous_format() {
    let new = Item::new("login", "x", 1);
    let ids: Vec<&str> = new.fields.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["username", "password", "url"]);
    for kind in TYPES {
        let item = Item::new(kind, "x", 1);
        assert!(item.validate().is_ok(), "{kind}");
        if let Some(secret) = primary_secret(kind) {
            assert!(item.field(secret).is_some_and(Field::hidden), "{kind}");
        }
    }

    let previous = |kind: &str, secret: &str| Previous {
        kind: kind.into(),
        title: "old".into(),
        tags: vec!["t".into()],
        body: "body".into(),
        username: "bob".into(),
        secret: secret.into(),
        url: "https://example.com".into(),
        favorite: true,
        created_at: 5,
        updated_at: 6,
        resolved_conflicts: Some(vec!["a:b".into()]),
    };
    let login = open_item(&canonical(&previous("login", "pw"))).unwrap();
    assert_eq!(
        (
            login.value("username"),
            login.value("password"),
            login.value("url")
        ),
        ("bob", "pw", "https://example.com")
    );
    assert_eq!((login.notes.as_str(), login.favorite), ("body", true));
    assert_eq!((login.created_at, login.updated_at), (5, 6));
    assert_eq!(login.resolved_conflicts, Some(vec!["a:b".into()]));
    let key = open_item(&canonical(&previous("key", "seed"))).unwrap();
    assert_eq!(key.value("key"), "seed");
    assert_eq!(key.value("username"), "bob");
    let note = open_item(&canonical(&previous("note", "s"))).unwrap();
    assert!(note.field("secret").is_some_and(Field::hidden));
    for item in [login, key, note] {
        assert!(item.validate().is_ok());
    }
    assert_eq!(
        resolved_in(&canonical(&previous("note", ""))),
        ["a:b".to_string()]
    );
    assert!(resolved_in(b"\x01").is_empty());
}

#[test]
fn edits_deletes_and_restores_are_versions() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let first = client.vault_create("login", &login("alpha")).unwrap();
    assert!(!first.synced && !first.conflict && !first.deleted);
    assert_eq!(first.item.value("password"), "s3cret");
    let changes = Changes {
        title: Some("alpha 2".into()),
        set: Some(vec![FieldChange {
            id: Some("password".into()),
            value: Some("new".into()),
            ..FieldChange::default()
        }]),
        ..Changes::default()
    };
    let second = client.vault_edit(&first.id, None, &changes).unwrap();
    assert_eq!(second.id, first.id);
    assert_ne!(second.revision, first.revision);
    assert_eq!(client.vault_show(&first.id).unwrap().item.title, "alpha 2");
    // An edit that changes nothing writes nothing.
    let same = client.vault_edit(&first.id, None, &changes).unwrap();
    assert_eq!(same.revision, second.revision);

    let found = |client: &Client, query: &str| {
        client
            .vault_list(&Filter {
                query: Some(query.into()),
                ..Filter::default()
            })
            .unwrap()
            .len()
    };
    assert_eq!(found(&client, "WORK"), 1);
    assert_eq!(found(&client, "alice"), 1);
    // Hidden values are never searched.
    assert_eq!(found(&client, "new"), 0);

    let deleted = client.vault_delete(&first.id).unwrap();
    assert!(deleted.deleted);
    assert!(client.vault_list(&Filter::default()).unwrap().is_empty());
    let trash = client
        .vault_list(&Filter {
            trash: true,
            ..Filter::default()
        })
        .unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].at, deleted.at);
    assert_eq!(client.vault_delete(&first.id).err(), Some(Error::NotFound));
    assert_eq!(
        client
            .vault_edit(&first.id, None, &Changes::default())
            .err(),
        Some(Error::NotFound)
    );

    let restored = client.vault_restore(&first.id).unwrap();
    assert!(!restored.deleted);
    assert_eq!(restored.item.value("password"), "new");
    assert_eq!(client.vault_list(&Filter::default()).unwrap().len(), 1);
    // Every version waits for the relay, each with its job.
    assert_eq!(client.workspace.store.count_in(LOCAL).unwrap(), 4);
    let jobs: Vec<(String, crate::sync::ContentJob)> = client.workspace.store.jobs().unwrap();
    assert_eq!(jobs.len(), 4);

    let masked = restored.masked(false);
    assert_eq!(masked.item.value("password"), MASK);
    assert_eq!(masked.item.value("username"), "alice");
    assert_eq!(masked.item.value("url"), "");
}

#[test]
fn fields_change_by_id_or_label() {
    let mut item = Item::new("login", "x", 1);
    item.fields[1].value = "hidden".into();
    let changes = Changes {
        fields: Some(vec![
            FieldChange {
                id: Some("password".into()),
                label: Some("PIN".into()),
                ..FieldChange::default()
            },
            FieldChange {
                kind: Some("otp".into()),
                value: Some("JBSWY3DPEHPK3PXP".into()),
                section: Some("2FA".into()),
                ..FieldChange::default()
            },
            FieldChange {
                id: Some("url".into()),
                ..FieldChange::default()
            },
        ]),
        add_tags: Some(vec!["new".into()]),
        ..Changes::default()
    };
    changes.apply(&mut item);
    item.normalize();
    assert_eq!(item.fields.len(), 3);
    assert_eq!(
        (item.fields[0].id.as_str(), item.fields[0].value.as_str()),
        ("password", "hidden")
    );
    assert_eq!(item.fields[0].label, "PIN");
    assert_eq!(item.fields[1].kind, "otp");
    assert_eq!(item.fields[1].id.len(), 8);
    assert_eq!(item.fields[2].kind, "url");
    assert!(item.validate().is_ok());

    let more = Changes {
        set: Some(vec![
            FieldChange {
                label: Some("pin".into()),
                value: Some("1234".into()),
                ..FieldChange::default()
            },
            FieldChange {
                label: Some("Recovery".into()),
                kind: Some("concealed".into()),
                value: Some("codes".into()),
                ..FieldChange::default()
            },
        ]),
        remove: Some(vec!["url".into()]),
        remove_tags: Some(vec!["NEW".into()]),
        ..Changes::default()
    };
    more.apply(&mut item);
    assert_eq!(item.value("password"), "1234");
    assert_eq!(item.fields.len(), 3);
    assert_eq!(item.fields[2].label, "Recovery");
    assert!(item.tags.is_empty());

    // A type's own field, without a label, goes by its ID and keeps no label.
    let mut database = Item::new("database", "x", 1);
    let host = Changes {
        set: Some(vec![FieldChange {
            label: Some("Host".into()),
            value: Some("db.example".into()),
            ..FieldChange::default()
        }]),
        ..Changes::default()
    };
    host.apply(&mut database);
    assert_eq!(database.fields.len(), 6);
    let field = database.field("host").unwrap();
    assert_eq!(
        (field.value.as_str(), field.label.as_str()),
        ("db.example", "")
    );
}

#[test]
fn dmsg_keys_are_read_only() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let reserved = Changes {
        title: Some("x".into()),
        tags: Some(vec!["dmsg-agent".into()]),
        ..Changes::default()
    };
    assert_eq!(
        client.vault_create("key", &reserved).err(),
        Some(Error::Forbidden)
    );
    let mut key = Item::new("key", "Agent ID a", 1);
    key.tags = vec!["dmsg-agent".into()];
    key.fields[0].value = "seed".into();
    let id = "ab".repeat(32);
    client.vault_item_with_id(&id, &key).unwrap();
    let rename = Changes {
        title: Some("y".into()),
        ..Changes::default()
    };
    let untag = Changes {
        tags: Some(vec![]),
        ..Changes::default()
    };
    for changes in [&rename, &untag] {
        assert_eq!(
            client.vault_edit(&id, None, changes).err(),
            Some(Error::Forbidden)
        );
    }
    assert_eq!(client.vault_delete(&id).err(), Some(Error::Forbidden));
    assert_eq!(
        client.vault_duplicate(&id, "copy").err(),
        Some(Error::Forbidden)
    );
    let revision = client.vault_show(&id).unwrap().revision;
    assert_eq!(
        client.vault_revert(&id, &revision).err(),
        Some(Error::Forbidden)
    );
    // No other item takes the tag.
    let other = client.vault_create("note", &rename).unwrap();
    assert_eq!(
        client
            .vault_edit(
                &other.id,
                None,
                &Changes {
                    add_tags: Some(vec!["dmsg-controller".into()]),
                    ..Changes::default()
                }
            )
            .err(),
        Some(Error::Forbidden)
    );
    assert_eq!(client.vault_show(&id).unwrap().item.value("key"), "seed");
}

#[test]
fn history_shows_and_reverts_versions() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let first = client.vault_create("login", &login("one")).unwrap();
    let rename = |title: &str| Changes {
        title: Some(title.into()),
        ..Changes::default()
    };
    let second = client.vault_edit(&first.id, None, &rename("two")).unwrap();
    let history = client.vault_history(&first.id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].revision, second.revision);
    assert!(history[0].current && !history[1].current);
    assert_eq!(history[0].parent.as_deref(), Some(first.revision.as_str()));
    assert_eq!(history[0].device.len(), 64);
    let old = client.vault_version(&first.id, &first.revision).unwrap();
    assert_eq!(old.item.title, "one");
    assert_eq!(
        client.vault_version(&first.id, "nope").err(),
        Some(Error::NotFound)
    );

    let reverted = client.vault_revert(&first.id, &first.revision).unwrap();
    assert_eq!(reverted.item.title, "one");
    assert_eq!(reverted.item.value("password"), "s3cret");
    assert_ne!(reverted.revision, first.revision);
    assert_eq!(client.vault_history(&first.id).unwrap().len(), 3);
    // Reverting to what is current writes nothing.
    let again = client.vault_revert(&first.id, &first.revision).unwrap();
    assert_eq!(again.revision, reverted.revision);

    let copy = client.vault_duplicate(&first.id, "one (copy)").unwrap();
    assert_ne!(copy.id, first.id);
    assert_eq!(copy.item.value("password"), "s3cret");
    assert_eq!(copy.item.title, "one (copy)");

    client.vault_delete(&first.id).unwrap();
    assert_eq!(
        client.vault_revert(&first.id, &first.revision).err(),
        Some(Error::NotFound)
    );
    let history = client.vault_history(&first.id).unwrap();
    assert!(history[0].deleted && history[0].current);
    client.lock();
    assert_eq!(client.vault_history(&first.id).err(), Some(Error::Locked));
}

#[test]
fn a_stale_edit_is_a_conflict_copy() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let first = client.vault_create("login", &login("alpha")).unwrap();
    let rename = |title: &str| Changes {
        title: Some(title.into()),
        ..Changes::default()
    };
    let head = client.vault_edit(&first.id, None, &rename("ours")).unwrap();
    let theirs = Changes {
        title: Some("theirs".into()),
        notes: Some("their notes".into()),
        set: Some(vec![FieldChange {
            id: Some("password".into()),
            value: Some("other".into()),
            ..FieldChange::default()
        }]),
        ..Changes::default()
    };
    let stale = client
        .vault_edit(&first.id, Some(&first.revision), &theirs)
        .unwrap();
    assert!(stale.conflict);
    // The copy is the edit of the version it named.
    assert_eq!(stale.item.value("username"), "alice");
    let key = format!("{}:{}", stale.id, stale.revision);
    // A conflict copy is still sent, so the relay keeps it for every device.
    let row = client.workspace.store.object(&key).unwrap().unwrap();
    assert_eq!((row.state.as_str(), row.conflict), (LOCAL, true));
    assert!(
        client
            .workspace
            .store
            .job::<crate::sync::ContentJob>(&key)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        client.vault_show(&first.id).unwrap().revision,
        head.revision
    );
    let conflicts = client.vault_conflicts().unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].head.revision, head.revision);
    assert_eq!(conflicts[0].hidden, ["password"]);
    let masked = conflicts[0].clone().masked(false);
    assert_eq!(masked.copy.item.value("password"), MASK);

    // A merge takes the parts it picks from the copy.
    let picks = BTreeMap::from([
        ("field:password".to_string(), Side::Theirs),
        ("title".to_string(), Side::Current),
    ]);
    let merged = client
        .vault_resolve(
            &key,
            &Keep::Merge {
                picks,
                notes: Some("both".into()),
            },
        )
        .unwrap();
    assert_eq!(merged.item.title, "ours");
    assert_eq!(merged.item.value("password"), "other");
    assert_eq!(merged.item.notes, "both");
    assert_eq!(merged.item.resolved_conflicts, Some(vec![key.clone()]));
    assert!(client.vault_conflicts().unwrap().is_empty());
    assert_eq!(
        client.vault_resolve(&key, &Keep::Current).err(),
        Some(Error::NotFound)
    );
    // Later edits keep the record of what was resolved.
    let edited = client
        .vault_edit(&first.id, None, &rename("later"))
        .unwrap();
    assert_eq!(edited.item.resolved_conflicts, Some(vec![key]));
}

#[test]
fn a_copy_can_be_adopted() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let first = client.vault_create("note", &login("alpha")).unwrap();
    let rename = |title: &str| Changes {
        title: Some(title.into()),
        ..Changes::default()
    };
    client.vault_edit(&first.id, None, &rename("ours")).unwrap();
    let stale = client
        .vault_edit(&first.id, Some(&first.revision), &rename("theirs"))
        .unwrap();
    let key = format!("{}:{}", stale.id, stale.revision);
    assert!(client.vault_conflicts().unwrap()[0].hidden.is_empty());
    let resolved = client.vault_resolve(&key, &Keep::Theirs).unwrap();
    assert_eq!(resolved.item.title, "theirs");
}

#[test]
fn ssh_items_fill_in_their_public_key() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let key = ssh::generate("me@host");
    let set = |id: &str, value: &str| Changes {
        title: Some("server".into()),
        set: Some(vec![FieldChange {
            id: Some(id.into()),
            value: Some(value.into()),
            ..FieldChange::default()
        }]),
        ..Changes::default()
    };
    let entry = client
        .vault_create("ssh", &set("private_key", &key.private_key))
        .unwrap();
    assert_eq!(entry.item.value("public_key"), key.public_key);
    assert_eq!(entry.item.value("fingerprint"), key.fingerprint);
    // Multi-line values keep their lines.
    assert_eq!(entry.item.value("private_key"), key.private_key);

    // A new private key brings its own public key and fingerprint; a
    // comment of one's own on the same key stays.
    let other = ssh::generate("other");
    let edited = client
        .vault_edit(&entry.id, None, &set("private_key", &other.private_key))
        .unwrap();
    assert_eq!(edited.item.value("public_key"), other.public_key);
    assert_eq!(edited.item.value("fingerprint"), other.fingerprint);
    let renamed = format!("{} laptop", other.public_key.rsplit_once(' ').unwrap().0);
    let edited = client
        .vault_edit(&entry.id, None, &set("public_key", &renamed))
        .unwrap();
    assert_eq!(edited.item.value("public_key"), renamed);
}

/// The cost of a large vault: `cargo test -p dmsg_client --release
/// large_vault -- --ignored --nocapture`.
#[test]
#[ignore]
fn large_vault() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let started = std::time::Instant::now();
    for i in 0..1000 {
        let id = client
            .vault_create("login", &login(&format!("item {i}")))
            .unwrap()
            .id;
        for v in 0..9 {
            let changes = Changes {
                notes: Some(format!("version {v}")),
                ..Changes::default()
            };
            client.vault_edit(&id, None, &changes).unwrap();
        }
    }
    println!("10,000 versions written in {:?}", started.elapsed());
    for _ in 0..3 {
        let started = std::time::Instant::now();
        let all = client.vault_list(&Filter::default()).unwrap();
        println!("listed {} items in {:?}", all.len(), started.elapsed());
    }
    let started = std::time::Instant::now();
    client.vault_health().unwrap();
    println!("checked in {:?}", started.elapsed());
}

#[test]
fn staged_import_receipt_is_atomic_and_preserves_original_selection() {
    let directory = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&directory.0, false, None);
    let item = Item::new("login", "Imported", 1);
    let first = client
        .vault_add_all_once("staged-source", vec![item.clone()])
        .unwrap();
    assert_eq!(
        client
            .vault_add_all_once("staged-source", vec![item.clone()])
            .unwrap(),
        first
    );
    let mut changed = item.clone();
    changed.title = "Changed selection".into();
    assert!(matches!(
        client.vault_add_all_once("staged-source", vec![changed]),
        Err(Error::VersionConflict)
    ));
    assert_eq!(client.vault_show(&first[0]).unwrap().item.title, "Imported");
    let mut invalid = item.clone();
    invalid.kind = "unsupported".into();
    assert!(
        client
            .vault_add_all_once("other-source", vec![item.clone(), invalid])
            .is_err()
    );
    assert_eq!(
        client
            .vault_add_all_once("other-source", vec![item])
            .unwrap()
            .len(),
        1
    );
}
