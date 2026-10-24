use super::*;
use crate::testing::{TempDir, unlocked};
use std::io::Write;

fn write(dir: &TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.0.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn reads_csv_exports() {
    let dir = TempDir::new();
    let onepassword = "\u{feff}Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\r\n\
        \"Mail, work\",https://mail.example.com,alice,\"p\"\"w\",otpauth://totp/x?secret=JBSWY3DPEHPK3PXP,true,false,\"work,mail\",\"line 1\nline 2\"\r\n\
        Old,https://old.example.com,bob,pw,,false,true,,\r\n\
        Bad otp,,carol,pw,not-a-secret,false,false,,\r\n";
    let imported = read(&write(&dir, "1p.csv", onepassword.as_bytes())).unwrap();
    assert_eq!((imported.format, imported.skipped), ("csv", 1));
    let mail = &imported.items[0];
    assert_eq!(mail.kind, "login");
    assert_eq!(mail.title, "Mail, work");
    assert_eq!(mail.value("password"), "p\"w");
    assert_eq!(mail.notes, "line 1\nline 2");
    assert_eq!(mail.tags, ["work", "mail"]);
    assert!(mail.favorite);
    assert!(mail.fields.iter().any(|f| f.kind == "otp"));
    let bad = &imported.items[1];
    assert!(
        bad.fields
            .iter()
            .any(|f| f.kind == "concealed" && f.value == "not-a-secret")
    );

    let firefox = "\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\"\n\
        \"https://shop.example.org\",\"dave\",\"pw\",,\"\",\"{x}\"\n";
    let imported = read(&write(&dir, "ff.csv", firefox.as_bytes())).unwrap();
    assert_eq!(imported.items[0].title, "shop.example.org");
    assert_eq!(imported.items[0].value("url"), "https://shop.example.org");

    let bitwarden = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
        Social,1,login,Site,,\"PIN: 1234\nQ: A\",0,https://site.example,erin,pw,\n\
        ,,note,Memo,remember this,,,,,,\n";
    let imported = read(&write(&dir, "bw.csv", bitwarden.as_bytes())).unwrap();
    assert_eq!(imported.items[0].tags, ["Social"]);
    assert_eq!(imported.items[0].fields.len(), 4);
    let extra = imported.items[0].fields.last().unwrap();
    assert!(extra.hidden());
    assert_eq!(extra.value, "PIN: 1234\nQ: A");
    assert_eq!(imported.items[1].kind, "note");
    assert_eq!(imported.items[1].notes, "remember this");

    assert_eq!(
        read(&write(&dir, "x.csv", b"a,b\n1,2\n")).err(),
        Some(Error::InvalidInput("format".into()))
    );
}

#[test]
fn reads_bitwarden_json() {
    let dir = TempDir::new();
    let json = serde_json::json!({
        "encrypted": false,
        "folders": [{"id": "f1", "name": "Work"}],
        "items": [
            {"type": 1, "name": "Git", "folderId": "f1", "favorite": true, "notes": "n",
             "login": {"username": "u", "password": "p", "totp": "JBSWY3DPEHPK3PXP",
                       "uris": [{"uri": "https://git.example"}, {"uri": "https://git2.example"}]},
             "fields": [{"name": "Recovery", "value": "r", "type": 1}, {"name": "Link", "value": "x", "type": 3}]},
            {"type": 3, "name": "Visa", "card": {"cardholderName": "A B", "brand": "Visa",
             "number": "4111111111111111", "expMonth": "7", "expYear": "29", "code": "123"}},
            {"type": 4, "name": "Me", "identity": {"firstName": "Ann", "lastName": "Lee",
             "email": "ann@example.com", "address1": "1 Road", "city": "Town", "ssn": "123",
             "passportNumber": "P123", "licenseNumber": "L456"}},
            {"type": 5, "name": "Box", "sshKey": {"privateKey": "k", "publicKey": "p", "keyFingerprint": "f"}},
            {"type": 2, "name": "Note", "notes": "text"}
        ]
    });
    let imported = read(&write(&dir, "bw.json", json.to_string().as_bytes())).unwrap();
    assert_eq!((imported.format, imported.items.len()), ("bitwarden", 5));
    assert_eq!(imported.incomplete, 1);
    let git = &imported.items[0];
    assert_eq!((git.value("username"), git.value("password")), ("u", "p"));
    assert_eq!(git.value("url"), "https://git.example");
    assert_eq!(git.tags, ["Work"]);
    assert!(git.fields.iter().any(|f| f.kind == "otp"));
    assert!(
        git.fields
            .iter()
            .any(|f| f.kind == "url" && f.value == "https://git2.example")
    );
    assert!(
        git.fields
            .iter()
            .any(|f| f.label == "Recovery" && f.hidden())
    );
    assert!(!git.fields.iter().any(|f| f.label == "Link"));
    let card = &imported.items[1];
    assert_eq!(card.value("expiry"), "2029-07");
    assert_eq!(card.value("cvv"), "123");
    let me = &imported.items[2];
    assert_eq!(me.value("name"), "Ann Lee");
    assert_eq!(me.value("address"), "1 Road\nTown");
    for (label, value) in [("Passport", "P123"), ("License", "L456")] {
        assert!(
            me.fields
                .iter()
                .any(|f| f.label == label && f.hidden() && f.value == value)
        );
    }
    assert_eq!(imported.items[3].value("private_key"), "k");
    assert_eq!(imported.items[4].notes, "text");

    let encrypted = serde_json::json!({"encrypted": true, "items": []});
    assert_eq!(
        read(&write(&dir, "e.json", encrypted.to_string().as_bytes())).err(),
        Some(Error::InvalidInput("encrypted".into()))
    );
}

#[test]
fn reads_1password_1pux() {
    let dir = TempDir::new();
    let data = serde_json::json!({"accounts": [{"attrs": {}, "vaults": [{"attrs": {"name": "Private"}, "items": [
        {"uuid": "1", "favIndex": 1, "createdAt": 1700000000, "updatedAt": 1700000100, "state": "active",
         "categoryUuid": "001",
         "details": {
            "loginFields": [
                {"value": "alice", "name": "username", "fieldType": "T", "designation": "username"},
                {"value": "pw", "name": "password", "fieldType": "P", "designation": "password"}],
            "notesPlain": "notes",
            "sections": [{"title": "Extra", "name": "s", "fields": [
                {"title": "one-time password", "id": "TOTP_1", "value": {"totp": "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP"}},
                {"title": "PIN", "id": "pin", "value": {"concealed": "1234"}},
                {"title": "Since", "id": "since", "value": {"date": 1700000000}},
                {"title": "Attachment", "id": "file", "value": {"file": {"fileName": "a.txt"}}}]}]},
         "overview": {"title": "Mail", "url": "https://mail.example", "tags": ["work"],
                      "urls": [{"label": "", "url": "https://mail.example"}, {"label": "admin", "url": "https://admin.example"}]}},
        {"uuid": "2", "state": "active", "categoryUuid": "002", "createdAt": 1700000000, "updatedAt": 1700000000,
         "details": {"sections": [{"title": "", "fields": [
            {"title": "cardholder name", "id": "cardholder", "value": {"string": "A B"}},
            {"title": "number", "id": "ccnum", "value": {"creditCardNumber": "4111111111111111"}},
            {"title": "expiry date", "id": "expiry", "value": {"monthYear": 202907}}]}]},
         "overview": {"title": "Visa"}},
        {"uuid": "3", "state": "archived", "categoryUuid": "003", "details": {}, "overview": {"title": "Old"}}
    ]}]}]});
    let path = dir.0.join("export.1pux");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("export.attributes", options).unwrap();
    zip.write_all(b"{}").unwrap();
    zip.start_file("export.data", options).unwrap();
    zip.write_all(data.to_string().as_bytes()).unwrap();
    zip.finish().unwrap();

    let imported = read(&path).unwrap();
    assert_eq!(
        (imported.format, imported.skipped, imported.items.len()),
        ("1pux", 1, 2)
    );
    assert_eq!(imported.incomplete, 1);
    let mail = &imported.items[0];
    assert_eq!(
        (mail.value("username"), mail.value("password")),
        ("alice", "pw")
    );
    assert_eq!(mail.value("url"), "https://mail.example");
    assert_eq!(
        (mail.created_at, mail.updated_at),
        (1_700_000_000_000, 1_700_000_100_000)
    );
    assert!(mail.favorite);
    assert!(
        mail.fields
            .iter()
            .any(|f| f.kind == "otp" && f.section == "Extra")
    );
    assert!(mail.fields.iter().any(|f| f.label == "PIN" && f.hidden()));
    assert!(
        mail.fields
            .iter()
            .any(|f| f.kind == "date" && f.value == "2023-11-14")
    );
    assert!(
        mail.fields
            .iter()
            .any(|f| f.label == "admin" && f.value == "https://admin.example")
    );
    assert!(!mail.fields.iter().any(|f| f.label == "Attachment"));
    let card = &imported.items[1];
    assert_eq!(card.kind, "card");
    assert_eq!(card.value("number"), "4111111111111111");
    assert_eq!(card.value("expiry"), "2029-07");
    assert_eq!(card.value("cardholder"), "A B");
}

#[test]
fn previews_then_imports_the_picked_items() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let csv = "Title,Url,Username,Password,OTPAuth\n\
        One,https://one.example,a,pw1,otpauth://totp/x?secret=JBSWY3DPEHPK3PXP\n\
        Two,https://two.example,b,pw2,\n";
    let path = write(&dir, "1p.csv", csv.as_bytes());
    let preview = client.vault_import_preview(&path).unwrap();
    // A field of the export's own, such as a one-time password, reads the
    // same each time, and so does the digest.
    assert_eq!(client.vault_import_preview(&path).unwrap(), preview);
    assert_eq!(preview.items.len(), 2);
    assert!(preview.items.iter().all(|i| !i.duplicate));
    assert_eq!(preview.items[0].username, "a");
    // Hidden values stay out of the preview.
    assert!(!serde_json::to_string(&preview).unwrap().contains("pw1"));

    assert_eq!(
        client.vault_import(&path, "00", &[0]).err(),
        Some(Error::VersionConflict)
    );
    assert_eq!(
        client.vault_import(&path, &preview.digest, &[2]).err(),
        Some(Error::InvalidInput("picks".into()))
    );
    let ids = client.vault_import(&path, &preview.digest, &[0]).unwrap();
    assert_eq!(ids.len(), 1);
    let added = client.vault_show(&ids[0]).unwrap();
    assert_eq!(added.item.value("password"), "pw1");
    assert!(added.item.fields.iter().any(|f| f.kind == "otp"));
    assert!(added.item.created_at > 0);

    let again = client.vault_import_preview(&path).unwrap();
    assert_eq!(again.digest, preview.digest);
    assert!(again.items[0].duplicate && !again.items[1].duplicate);
}

#[test]
fn preserves_password_whitespace_and_multiline_custom_fields() {
    let dir = TempDir::new();
    let csv = "name,username,password\r\nMail,alice,\"secret\r\n\"\r\nSpaces,bob,   \r\n";
    let parsed = read(&write(&dir, "passwords.csv", csv.as_bytes())).unwrap();
    assert_eq!(parsed.items[0].value("password"), "secret\r\n");
    assert_eq!(parsed.items[1].value("password"), "   ");
    let parsed = bitwarden(&serde_json::json!({"items": [{
        "type": 1, "name": "Mail", "login": {"password": " \t\r\n"},
        "fields": [{"name": "Backup", "type": 1, "value": "first\nsecond\n"}]
    }]}))
    .unwrap();
    assert_eq!(parsed.items[0].value("password"), " \t\r\n");
    assert_eq!(
        parsed.items[0].fields.last().unwrap().value,
        "first\nsecond\n"
    );
}

#[test]
fn rejects_broken_csv_and_skips_misaligned_rows() {
    for malformed in [
        "name,username,password\nSite,alice,\"unfinished",
        "name,username,password\nSite,alice,\"closed\"garbage\n",
        "name,username,password\nSite,alice,unquoted\"quote\n",
    ] {
        assert_eq!(csv(malformed).err(), Some(invalid("format")));
    }
    let parsed =
        csv("name,username,password\rMissing,alice\rExtra,bob,pw,shifted\rGood,carol,\"p,w\"\r")
            .unwrap();
    assert_eq!(parsed.skipped, 2);
    assert_eq!(parsed.items.len(), 1);
    assert_eq!(parsed.items[0].value("password"), "p,w");
}

#[test]
fn duplicates_require_all_content_to_match() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let path = write(
        &dir,
        "export.csv",
        b"name,username,password,url,notes\nSite,Alice,old,https://site.example/A,first\n",
    );
    let preview = client.vault_import_preview(&path).unwrap();
    client.vault_import(&path, &preview.digest, &[0]).unwrap();
    let cases = [
        ("Site,Alice,old,https://site.example/A,first", true),
        ("Site,Alice,new,https://site.example/A,first", false),
        ("Site,alice,old,https://site.example/A,first", false),
        ("Site,Alice,old,https://site.example/a,first", false),
        ("Site,Alice,old,https://site.example/A,second", false),
    ];
    for (row, duplicate) in cases {
        std::fs::write(
            &path,
            format!("name,username,password,url,notes\n{row}\n{row}\n"),
        )
        .unwrap();
        let preview = client.vault_import_preview(&path).unwrap();
        assert_eq!(preview.items[0].duplicate, duplicate, "{row}");
        assert!(preview.items[1].duplicate);
    }
}

#[test]
fn reads_1password_form_types_and_keeps_hidden_template_fields_hidden() {
    let entry = serde_json::json!({
        "categoryUuid": "001", "overview": {"title": "Form"},
        "details": {"loginFields": [
            {"name": "count", "fieldType": "N", "value": "42"},
            {"name": "comment", "fieldType": "A", "value": "line 1\nline 2\n"},
            {"name": "phone", "fieldType": "TEL", "value": "+1 234"},
            {"name": "password", "type": "P", "value": "hidden"},
            {"designation": "username", "fieldType": "P", "value": "hidden-user"}
        ]}
    });
    let item = onepux_item(&entry, &mut false);
    for value in ["42", "line 1\nline 2\n", "+1 234"] {
        assert!(item.fields.iter().any(|f| f.value == value));
    }
    assert!(
        item.fields
            .iter()
            .find(|f| f.value == "hidden")
            .unwrap()
            .hidden()
    );
    assert!(item.field("username").unwrap().hidden());
    assert_eq!(shown(&item, "username"), "");
    let mut blank = item;
    blank.title.clear();
    let mut kept = Vec::new();
    keep(blank, &mut kept, &mut 0);
    assert_eq!(kept[0].title, "Untitled");
}

#[test]
fn changed_file_is_rejected_and_locked_clients_do_not_read_exports() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let path = write(&dir, "export.csv", b"name,password\nSite,old\n");
    let preview = client.vault_import_preview(&path).unwrap();
    // Even a change that parses to the same items invalidates confirmation.
    std::fs::write(&path, b"name,password\r\nSite,old\r\n").unwrap();
    assert_eq!(
        client.vault_import(&path, &preview.digest, &[0]).err(),
        Some(Error::VersionConflict)
    );
    std::fs::write(&path, b"name,password\nSite,new\n").unwrap();
    assert_eq!(
        client.vault_import(&path, &preview.digest, &[0]).err(),
        Some(Error::VersionConflict)
    );
    assert!(client.vault_list(&Filter::default()).unwrap().is_empty());
    client.lock();
    let missing = dir.0.join("missing.csv");
    assert_eq!(
        client.vault_import_preview(&missing).err(),
        Some(Error::Locked)
    );
    assert_eq!(
        client.vault_import(&missing, &preview.digest, &[0]).err(),
        Some(Error::Locked)
    );
}

#[test]
fn export_size_limit_applies_to_archives_too() {
    let dir = TempDir::new();
    for (name, head) in [
        ("large.csv", &b"name"[..]),
        ("large.1pux", &b"PK\x03\x04"[..]),
    ] {
        let path = write(&dir, name, head);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_EXPORT + 1)
            .unwrap();
        assert_eq!(read(&path).err(), Some(Error::QuotaExceeded));
    }
}

#[test]
fn a_failed_batch_leaves_no_items_or_sync_jobs() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let before = client.workspace.store.jobs::<cbor2::Value>().unwrap();
    let good = Item::new("note", "Good", 1);
    let bad = Item::new("note", "", 1);
    assert!(client.vault_add_all(vec![good.clone(), bad]).is_err());
    assert!(client.vault_list(&Filter::default()).unwrap().is_empty());
    assert_eq!(
        client.workspace.store.jobs::<cbor2::Value>().unwrap(),
        before
    );
    assert_eq!(client.vault_add_all(vec![good]).unwrap().len(), 1);
}

#[test]
fn a_storage_failure_rolls_back_the_whole_import_and_allows_retry() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let path = write(
        &dir,
        "export.csv",
        b"name,password\nOne,secret1\nTwo,secret2\n",
    );
    let preview = client.vault_import_preview(&path).unwrap();
    let database = dir
        .0
        .join("accounts")
        .join(crate::testing::account().id.to_string())
        .join("workspace.sqlite");
    let db = rusqlite::Connection::open(database).unwrap();
    let counts = || {
        ["objects", "heads", "chunks", "jobs"].map(|table| {
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
        })
    };
    let before = counts();
    db.execute_batch(
        "CREATE TRIGGER fail_second_import BEFORE INSERT ON objects
        WHEN NEW.kind = 'vault' AND (SELECT count(*) FROM objects WHERE kind = 'vault') = 1
        BEGIN SELECT RAISE(FAIL, 'injected import storage failure'); END;",
    )
    .unwrap();
    assert!(matches!(
        client.vault_import(&path, &preview.digest, &[0, 1]),
        Err(Error::Unavailable(_))
    ));
    assert_eq!(counts(), before);
    assert!(client.vault_list(&Filter::default()).unwrap().is_empty());
    db.execute_batch("DROP TRIGGER fail_second_import").unwrap();
    let ids = client
        .vault_import(&path, &preview.digest, &[0, 1])
        .unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(client.vault_list(&Filter::default()).unwrap().len(), 2);
    assert_eq!(
        client.vault_show(&ids[1]).unwrap().item.value("password"),
        "secret2"
    );
}

#[test]
fn omitted_data_is_reported_and_invalid_json_entries_are_not_empty_notes() {
    let imported = bitwarden(&serde_json::json!({"items": [
        null,
        {"type": 99, "name": "Future type"},
        {"type": 1, "name": "Trashed", "deletedDate": "2026-01-01"},
        {"type": 1, "name": "Passkey", "login": {"fido2Credentials": [{"keyValue": "secret"}]}},
        {"type": 2, "name": "With history", "passwordHistory": [{"password": "old"}]},
        {"type": 2, "name": "With file", "attachments": [{"fileName": "private.txt"}]}
    ]}))
    .unwrap();
    assert_eq!(imported.skipped, 3);
    assert_eq!(imported.incomplete, 3);
    assert_eq!(imported.items.len(), 3);
}

#[test]
fn selected_1pux_attachments_are_streamed_and_omissions_are_explicit() {
    let dir = TempDir::new();
    let path = dir.0.join("with-files.1pux");
    let data = serde_json::json!({"accounts":[{"vaults":[{"items":[{"overview":{"title":"Selected"},"details":{"passwordHistory":[{"value":"old-fixture"}],"passkeys":[{"key":"not-imported"}],"documentAttributes":{"documentId":"abc123","fileName":"a.txt","decryptedSize":4}},"categoryUuid":"003"}]}]}]});
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("export.data", options).unwrap();
    zip.write_all(data.to_string().as_bytes()).unwrap();
    zip.start_file("files/abc123___a.txt", options).unwrap();
    zip.write_all(b"data").unwrap();
    zip.finish().unwrap();
    let imported = read(&path).unwrap();
    let preview = imported.preview(&[]);
    assert_eq!(preview.attachments.len(), 1);
    assert!(preview.attachments[0].supported);
    assert!(preview.omissions.iter().any(|o| o.category == "history"));
    assert!(preview.omissions.iter().any(|o| o.category == "passkey"));
    let shown = serde_json::to_string(&preview).unwrap();
    assert!(!shown.contains("old-fixture"));
    assert!(!shown.contains("not-imported"));
    let mut received = Vec::new();
    let items = imported
        .stream_selected(
            &preview.digest,
            &[0],
            &[preview.attachments[0].entry_id.clone()],
            |e| {
                if let AttachmentEvent::Chunk(b) = e {
                    received.extend_from_slice(&b);
                }
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(received, b"data");
    assert_eq!(items.len(), 1);
    assert!(
        read(&path)
            .unwrap()
            .stream_selected("wrong", &[0], &[], |_| Ok(()))
            .is_err()
    );
    assert!(
        read(&path)
            .unwrap()
            .stream_selected(
                &preview.digest,
                &[],
                &[preview.attachments[0].entry_id.clone()],
                |_| Ok(())
            )
            .is_err()
    );
    read(&path)
        .unwrap()
        .stream_selected(&preview.digest, &[0], &[], |_| {
            panic!("unselected attachment must not be emitted")
        })
        .unwrap();
}

#[test]
fn onepux_rejects_archive_paths_and_symbolic_members() {
    for name in ["../escape", "/absolute", "files/../escape", "files\\escape"] {
        let dir = TempDir::new();
        let path = dir.0.join("bad.1pux");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("export.data", options).unwrap();
        zip.write_all(br#"{"accounts":[]}"#).unwrap();
        zip.start_file(name, options).unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
        assert!(read(&path).is_err(), "{name}");
    }
}
