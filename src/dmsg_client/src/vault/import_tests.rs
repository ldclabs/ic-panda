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
    assert_eq!(imported.items[0].fields.len(), 5);
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
             "email": "ann@example.com", "address1": "1 Road", "city": "Town", "ssn": "123"}},
            {"type": 5, "name": "Box", "sshKey": {"privateKey": "k", "publicKey": "p", "keyFingerprint": "f"}},
            {"type": 2, "name": "Note", "notes": "text"}
        ]
    });
    let imported = read(&write(&dir, "bw.json", json.to_string().as_bytes())).unwrap();
    assert_eq!((imported.format, imported.items.len()), ("bitwarden", 5));
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
    let csv = "name,url,username,password\n\
        One,https://one.example,a,pw1\n\
        Two,https://two.example,b,pw2\n";
    let path = write(&dir, "chrome.csv", csv.as_bytes());
    let preview = client.vault_import_preview(&path).unwrap();
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
    assert!(added.item.created_at > 0);

    let again = client.vault_import_preview(&path).unwrap();
    assert_eq!(again.digest, preview.digest);
    assert!(again.items[0].duplicate && !again.items[1].duplicate);
}
