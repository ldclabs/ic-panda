//! Importing other password managers' exports, parsed on this device:
//! 1Password (`.1pux`, CSV), Bitwarden (unencrypted JSON, CSV) and the CSV
//! exports of Chrome, Firefox and Safari. People see a preview without
//! hidden values, choose items, and the same file is read again to add
//! them; the preview's digest makes sure it is still the same content.

use super::{Field, Filter, Item, RESERVED_TAG};
use crate::client::Client;
use crate::codec::{canonical, sha256};
use dmsg_types::{Error, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{Cursor, Read, Seek};
use std::path::Path;
use zeroize::Zeroizing;

/// Largest export read, and largest `export.data` inside a `.1pux`.
const MAX_EXPORT: u64 = 64 * 1024 * 1024;

/// An export, read.
pub struct Imported {
    /// Digest of the entire source file, including data left out of the import.
    digest: String,
    /// `1pux`, `bitwarden` or `csv`.
    pub format: &'static str,
    /// The items it holds, valid as they are. Times the export lacks are 0.
    pub items: Vec<Item>,
    /// Entries left out: archived or invalid.
    pub skipped: usize,
    /// Imported entries that also contain unsupported data.
    pub incomplete: usize,
    /// Per-source item data omitted or requiring explicit attachment selection.
    pub omissions: Vec<Omission>,
    /// Indexed, explicitly selectable archive members.
    pub attachments: Vec<Attachment>,
    archive: Option<Zeroizing<Vec<u8>>>,
}

/// What an import would add, without hidden values.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// `1pux`, `bitwarden` or `csv`.
    pub format: &'static str,
    /// Names this content; the import must repeat it.
    pub digest: String,
    /// Entries left out.
    pub skipped: usize,
    /// Entries whose attachments, history, passkeys or other fields were omitted.
    pub incomplete: usize,
    /// Per-source item data omitted or requiring explicit attachment selection.
    pub omissions: Vec<Omission>,
    /// Indexed, explicitly selectable archive members.
    pub attachments: Vec<Attachment>,
    /// The items, in the export's order.
    pub items: Vec<PreviewItem>,
}

/// One source item's unsupported or unselected data, never values.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Omission {
    /// Original source position, including invalid/archived entries.
    pub source_item_index: usize,
    /// attachment, passkey, history, unknown_field or invalid_item.
    pub category: String,
    /// Stable explanation, never copied secret text.
    pub reason: String,
}

/// A ZIP member identified by the export's own document metadata.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    /// Original source position.
    pub source_item_index: usize,
    /// Eligible destination item index, absent for a skipped item.
    pub item_index: Option<usize>,
    /// Exact safe archive member name; never used as a destination path.
    pub entry_id: String,
    #[serde(skip)]
    member: String,
    /// Suggested display name.
    pub name: String,
    /// Exact declared size.
    pub size: u64,
    /// Can be selected for import.
    pub supported: bool,
    /// Stable explanation when unavailable.
    pub reason: Option<String>,
}

/// Worker-to-core streaming events. Only Chunk contains plaintext, and is zeroized.
pub enum AttachmentEvent {
    /// Start a member for the selected destination item index.
    Start {
        /// Index in the selected destination item list.
        item_index: usize,
        /// Display name, never a source filesystem path.
        name: String,
        /// Exact declared member size.
        size: u64,
    },
    /// At most one file chunk, not logged or serialized.
    Chunk(Zeroizing<Vec<u8>>),
    /// Exact length and CRC/EOF were checked.
    End,
}

/// One item of a [`Preview`].
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct PreviewItem {
    /// Its place in the export.
    pub index: usize,
    /// Its type.
    #[serde(rename = "type")]
    pub kind: String,
    /// Its title.
    pub title: String,
    /// Its user name, if any.
    pub username: String,
    /// Its address, if any.
    pub url: String,
    /// Its tags.
    pub tags: Vec<String>,
    /// How many fields have a value.
    pub fields: usize,
    /// An item with the same content is already here or earlier in this export.
    pub duplicate: bool,
}

fn invalid(what: &str) -> Error {
    Error::InvalidInput(what.into())
}

/// Read an export file.
///
/// # Errors
/// Returns `Error::InvalidInput("format")` for a file in none of the
/// formats, `Error::InvalidInput("encrypted")` for an encrypted export and
/// `Error::QuotaExceeded` for one over 64 MiB.
pub fn read(path: &Path) -> Result<Imported> {
    let file = std::fs::File::open(path).map_err(crate::storage)?;
    let metadata = file.metadata().map_err(crate::storage)?;
    ensure(metadata.is_file(), invalid("format"))?;
    ensure(metadata.len() <= MAX_EXPORT, Error::QuotaExceeded)?;
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(MAX_EXPORT + 1)
        .read_to_end(&mut bytes)
        .map_err(crate::storage)?;
    ensure(bytes.len() as u64 <= MAX_EXPORT, Error::QuotaExceeded)?;
    let mut imported = if bytes.starts_with(b"PK\x03\x04") {
        onepux(Cursor::new(&bytes[..]))?
    } else {
        let text = std::str::from_utf8(&bytes).map_err(|_| invalid("format"))?;
        let text = text.trim_start_matches('\u{feff}');
        if text.trim_start().starts_with('{') {
            let json: Value = serde_json::from_str(text).map_err(|_| invalid("format"))?;
            bitwarden(&json)?
        } else {
            csv(text)?
        }
    };
    imported.digest = hex::encode(*sha256(&bytes));
    if imported.format == "1pux" {
        imported.archive = Some(bytes);
    }
    Ok(imported)
}

/// Keep an item that is valid once normalized; count it skipped otherwise.
fn keep(mut item: Item, items: &mut Vec<Item>, skipped: &mut usize) {
    item.tags.retain(|t| !t.trim().starts_with(RESERVED_TAG));
    if item.title.trim().is_empty() {
        let title: String = [shown(&item, "url"), shown(&item, "username")]
            .into_iter()
            .find(|v| !v.is_empty())
            .map(|v| {
                url::Url::parse(v)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_string))
                    .unwrap_or_else(|| v.to_string())
            })
            .unwrap_or_else(|| "Untitled".into());
        item.title = title.chars().take(200).collect();
    }
    // Fields of the export's own are named by their place, not at random,
    // so that reading the file again gives the same items and digest.
    for (n, field) in item.fields.iter_mut().enumerate() {
        if field.id.is_empty() {
            field.id = format!("f{n}");
        }
    }
    item.normalize();
    // A value its type cannot hold is kept as concealed (a one-time
    // password) or plain text.
    for field in &mut item.fields {
        if field.validate().is_err() {
            field.kind = match field.kind.as_str() {
                "otp" => "concealed",
                "date" | "month" => "text",
                kind => kind,
            }
            .into();
        }
    }
    match item.validate() {
        Ok(()) => items.push(item),
        Err(_) => *skipped += 1,
    }
}

/// Put `value` in the template field `id` when the item has it and it is
/// empty, else add a field.
fn put(item: &mut Item, id: &str, kind: &str, label: &str, section: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    if let Some(field) = item
        .fields
        .iter_mut()
        .find(|f| !id.is_empty() && f.id == id && f.value.is_empty())
    {
        // A source's hidden field must stay hidden even when mapped to
        // a template field normally displayed as text (such as a user name).
        if matches!(kind, "concealed" | "otp") {
            field.kind = kind.into();
        }
        field.value = value.into();
        return;
    }
    item.fields.push(Field {
        id: String::new(),
        kind: kind.into(),
        label: label.into(),
        section: section.into(),
        value: value.into(),
    });
}

/// A `YYYY-MM-DD` date of Unix seconds.
fn date(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

// 1Password

fn onepux(file: impl Read + Seek) -> Result<Imported> {
    let mut archive = zip::ZipArchive::new(file).map_err(|_| invalid("format"))?;
    ensure(archive.len() <= 10_000, Error::QuotaExceeded)?;
    let mut members = std::collections::BTreeMap::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|_| invalid("archive member"))?;
        let member = entry.name().map_err(|_| invalid("archive name"))?;
        let name = member.trim_end_matches('/');
        ensure(
            !name.is_empty()
                && !name.contains('\\')
                && name
                    .split('/')
                    .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains(':'))
                && entry.enclosed_name().is_some()
                && entry.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
            invalid("unsafe archive member"),
        )?;
        ensure(
            members.insert(name.to_owned(), entry.size()).is_none(),
            invalid("duplicate archive member"),
        )?;
    }
    let entry = archive
        .by_name("export.data")
        .map_err(|_| invalid("format"))?;
    let declared = entry.size();
    ensure(declared <= MAX_EXPORT, Error::QuotaExceeded)?;
    let mut data = Zeroizing::new(Vec::new());
    entry
        .take(declared + 1)
        .read_to_end(&mut data)
        .map_err(|_| invalid("format"))?;
    ensure(data.len() as u64 == declared, Error::IntegrityFailed)?;
    let json: Value = serde_json::from_slice(&data).map_err(|_| invalid("format"))?;
    let (mut items, mut skipped, mut incomplete) = (Vec::new(), 0, 0);
    let mut omissions = Vec::new();
    let mut attachments = Vec::new();
    let mut source = 0;
    for account in json["accounts"]
        .as_array()
        .ok_or_else(|| invalid("format"))?
    {
        for vault in account["vaults"]
            .as_array()
            .ok_or_else(|| invalid("format"))?
        {
            for entry in vault["items"].as_array().ok_or_else(|| invalid("format"))? {
                let source_item_index = source;
                source += 1;
                let mut omit = |category: &str, reason: &str| {
                    omissions.push(Omission {
                        source_item_index,
                        category: category.into(),
                        reason: reason.into(),
                    })
                };
                if entry["state"] == "archived"
                    || !entry["overview"].is_object()
                    || !entry["details"].is_object()
                {
                    skipped += 1;
                    omit("invalid_item", "archived_or_invalid");
                    continue;
                }
                let details = &entry["details"];
                for (key, category) in [("passwordHistory", "history"), ("passkeys", "passkey")] {
                    if populated(&details[key]) {
                        omit(category, "not_imported");
                    }
                }
                let before = items.len();
                let mut omitted = false;
                let item = onepux_item(entry, &mut omitted);
                // Document-only items remain eligible when their attachment can be selected.
                keep(item, &mut items, &mut skipped);
                if items.len() == before {
                    omit("invalid_item", "invalid_item");
                    continue;
                }
                let mut documents = Vec::new();
                if details["documentAttributes"].is_object() {
                    documents.push(&details["documentAttributes"]);
                }
                for section in details["sections"].as_array().into_iter().flatten() {
                    for field in section["fields"].as_array().into_iter().flatten() {
                        if field["value"]["file"].is_object() {
                            documents.push(&field["value"]["file"]);
                        }
                    }
                }
                for doc in documents {
                    let name = doc["fileName"].as_str().unwrap_or("");
                    let id = doc["documentId"].as_str().unwrap_or("");
                    let path = format!("files/{id}___{name}");
                    let size = doc["decryptedSize"].as_u64().unwrap_or(u64::MAX);
                    let supported = !id.is_empty()
                        && id.len() <= 128
                        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                        && super::super::library::catalog::validate_name(name).is_ok()
                        && members.get(&path) == Some(&size)
                        && size <= crate::files::MAX_FILE
                        && attachments
                            .iter()
                            .filter(|a: &&Attachment| a.item_index == Some(before))
                            .count()
                            < 20;
                    attachments.push(Attachment {
                        source_item_index,
                        item_index: Some(before),
                        entry_id: format!("a{}", attachments.len()),
                        member: path,
                        name: name.into(),
                        size,
                        supported,
                        reason: (!supported)
                            .then(|| "missing_invalid_or_oversized_attachment".into()),
                    });
                    omit(
                        "attachment",
                        if supported {
                            "requires_explicit_selection"
                        } else {
                            "missing_invalid_or_oversized_attachment"
                        },
                    );
                }
                if populated(&entry["file"]) && !details["documentAttributes"].is_object() {
                    omit("attachment", "unrecognized_attachment_metadata");
                }
                if omitted {
                    omit("unknown_field", "some_source_features_are_not_imported");
                }
                incomplete += usize::from(
                    omissions
                        .iter()
                        .any(|o| o.source_item_index == source_item_index),
                );
            }
        }
    }
    Ok(Imported {
        digest: String::new(),
        format: "1pux",
        items,
        skipped,
        incomplete,
        omissions,
        attachments,
        archive: None,
    })
}

fn onepux_kind(category: &str) -> &'static str {
    match category {
        "001" | "005" => "login",
        "002" => "card",
        "004" => "identity",
        "100" => "license",
        "102" => "database",
        "110" => "server",
        "112" => "api",
        "114" => "ssh",
        "115" => "wallet",
        _ => "note",
    }
}

/// The template field a 1Password section field fills, by item type.
fn onepux_field(kind: &str, id: &str) -> &'static str {
    match (kind, id) {
        ("card", "cardholder") => "cardholder",
        ("card", "ccnum") => "number",
        ("card", "cvv") => "cvv",
        ("card", "expiry") => "expiry",
        ("card", "pin") => "pin",
        ("identity", "email") => "email",
        ("identity", "defphone" | "cellphone" | "homephone" | "busphone") => "phone",
        ("identity", "address") => "address",
        ("identity", "birthdate") => "birthday",
        ("license", "reg_code") => "license_key",
        ("license", "reg_name") => "licensed_to",
        ("license", "reg_email") => "email",
        ("license", "order_date") => "purchased",
        ("license", "expiry_date") => "expires",
        ("database", "database_type") => "engine",
        ("database", "hostname") => "host",
        ("database", "port") => "port",
        ("database", "database") => "database",
        ("database" | "server" | "api", "username") => "username",
        ("database" | "server", "password") => "password",
        ("server", "url") => "url",
        ("api", "credential") => "credential",
        ("api", "expires") => "expires",
        ("wallet", "recoveryPhrase") => "phrase",
        ("wallet", "password") => "password",
        ("wallet", "walletAddress" | "address") => "address",
        _ => "",
    }
}

fn onepux_item(entry: &Value, omitted: &mut bool) -> Item {
    let overview = &entry["overview"];
    let details = &entry["details"];
    let kind = onepux_kind(entry["categoryUuid"].as_str().unwrap_or_default());
    let seconds = |v: &Value| v.as_u64().and_then(|s| s.checked_mul(1000)).unwrap_or(0);
    let mut item = Item::new(
        kind,
        &text(&overview["title"]),
        seconds(&entry["createdAt"]),
    );
    item.updated_at = seconds(&entry["updatedAt"]);
    item.favorite = entry["favIndex"].as_u64().unwrap_or(0) > 0;
    item.tags = overview["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .map(text)
        .collect();
    item.notes = text(&details["notesPlain"]);
    for field in details["loginFields"].as_array().into_iter().flatten() {
        let value = text(&field["value"]);
        let field_type = field["fieldType"]
            .as_str()
            .or_else(|| field["type"].as_str());
        match (field["designation"].as_str(), field_type) {
            (Some("username"), field_type) => put(
                &mut item,
                "username",
                if field_type == Some("P") {
                    "concealed"
                } else {
                    "text"
                },
                "",
                "",
                &value,
            ),
            (Some("password"), _) => put(&mut item, "password", "concealed", "", "", &value),
            (_, Some("P")) => put(
                &mut item,
                "",
                "concealed",
                &text(&field["name"]),
                "",
                &value,
            ),
            (_, Some("E")) => put(&mut item, "", "email", &text(&field["name"]), "", &value),
            (_, Some("U")) => put(&mut item, "", "url", &text(&field["name"]), "", &value),
            (_, Some("TEL")) => put(&mut item, "", "phone", &text(&field["name"]), "", &value),
            (_, Some("T" | "N" | "A" | "")) | (_, None) => {
                put(&mut item, "", "text", &text(&field["name"]), "", &value)
            }
            _ => *omitted |= !value.is_empty(),
        }
    }
    put(
        &mut item,
        "password",
        "concealed",
        "",
        "",
        &text(&details["password"]),
    );
    let mut urls = overview["urls"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| (text(&u["label"]), text(&u["url"])))
        .collect::<Vec<_>>();
    let main = text(&overview["url"]);
    if !main.is_empty() && !urls.iter().any(|(_, u)| *u == main) {
        urls.insert(0, (String::new(), main));
    }
    for (label, url) in urls {
        put(&mut item, "url", "url", &label, "", &url);
    }
    let mut names = (String::new(), String::new());
    for section in details["sections"].as_array().into_iter().flatten() {
        let heading = text(&section["title"]);
        for field in section["fields"].as_array().into_iter().flatten() {
            let id = field["id"].as_str().unwrap_or_default();
            let label = text(&field["title"]);
            let target = onepux_field(kind, id);
            let section = if target.is_empty() {
                heading.as_str()
            } else {
                ""
            };
            let Some((variant, value)) = field["value"].as_object().and_then(|v| v.iter().next())
            else {
                continue;
            };
            match (kind, id) {
                ("identity", "firstname") if variant != "concealed" => {
                    names.0 = text(value);
                    continue;
                }
                ("identity", "lastname") if variant != "concealed" => {
                    names.1 = text(value);
                    continue;
                }
                _ => {}
            }
            let (field_kind, value) = match variant.as_str() {
                "concealed" | "creditCardNumber" => ("concealed", text(value)),
                "totp" => ("otp", text(value)),
                "url" => ("url", text(value)),
                "phone" => ("phone", text(value)),
                "email" => (
                    "email",
                    value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| text(&value["email_address"])),
                ),
                "date" => ("date", value.as_i64().map(date).unwrap_or_default()),
                "monthYear" => (
                    "month",
                    value
                        .as_u64()
                        .map(|m| format!("{:04}-{:02}", m / 100, m % 100))
                        .unwrap_or_default(),
                ),
                "address" => (
                    "text",
                    [
                        text(&value["street"]),
                        [
                            text(&value["city"]),
                            text(&value["state"]),
                            text(&value["zip"]),
                        ]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                        text(&value["country"]),
                    ]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
                ),
                "sshKey" => {
                    put(
                        &mut item,
                        "public_key",
                        "text",
                        "",
                        "",
                        &text(&value["metadata"]["publicKey"]),
                    );
                    put(
                        &mut item,
                        "fingerprint",
                        "text",
                        "",
                        "",
                        &text(&value["metadata"]["fingerprint"]),
                    );
                    ("concealed", text(&value["privateKey"]))
                }
                "file" => continue,
                "string" | "menu" | "gender" | "creditCardType" => ("text", text(value)),
                _ => {
                    *omitted |= populated(value);
                    continue;
                }
            };
            let target = if variant == "sshKey" {
                "private_key"
            } else {
                target
            };
            put(&mut item, target, field_kind, &label, section, &value);
        }
    }
    let name = [names.0, names.1]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    put(&mut item, "name", "text", "", "", &name);
    item
}

// Bitwarden

fn populated(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        _ => true,
    }
}

fn bitwarden(json: &Value) -> Result<Imported> {
    ensure(json["encrypted"] != true, invalid("encrypted"))?;
    let items_json = json["items"].as_array().ok_or_else(|| invalid("format"))?;
    let folders: Vec<(String, String)> = json["folders"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| (text(&f["id"]), text(&f["name"])))
        .collect();
    let (mut items, mut skipped, mut incomplete) = (Vec::new(), 0, 0);
    let mut omissions = Vec::new();
    for (source_item_index, entry) in items_json.iter().enumerate() {
        let mut omit = |category: &str, reason: &str| {
            omissions.push(Omission {
                source_item_index,
                category: category.into(),
                reason: reason.into(),
            })
        };
        if populated(&entry["deletedDate"]) {
            skipped += 1;
            omit("invalid_item", "deleted_source_item");
            continue;
        }
        let kind = match entry["type"].as_u64() {
            Some(1) => "login",
            Some(2) => "note",
            Some(3) => "card",
            Some(4) => "identity",
            Some(5) => "ssh",
            _ => {
                skipped += 1;
                omit("invalid_item", "unsupported_item_type");
                continue;
            }
        };
        for (value, category) in [
            (&entry["attachments"], "attachment"),
            (&entry["passwordHistory"], "history"),
            (&entry["login"]["fido2Credentials"], "passkey"),
        ] {
            if populated(value) {
                omit(category, "not_in_imported_copy");
            }
        }
        let mut item = Item::new(kind, &text(&entry["name"]), 0);
        item.notes = text(&entry["notes"]);
        item.favorite = entry["favorite"] == true;
        if let Some((_, folder)) = folders
            .iter()
            .find(|(id, _)| *id == text(&entry["folderId"]))
        {
            item.tags.push(folder.clone());
        }
        let login = &entry["login"];
        put(
            &mut item,
            "username",
            "text",
            "",
            "",
            &text(&login["username"]),
        );
        put(
            &mut item,
            "password",
            "concealed",
            "",
            "",
            &text(&login["password"]),
        );
        put(&mut item, "", "otp", "", "", &text(&login["totp"]));
        for uri in login["uris"].as_array().into_iter().flatten() {
            put(&mut item, "url", "url", "", "", &text(&uri["uri"]));
        }
        let card = &entry["card"];
        put(
            &mut item,
            "cardholder",
            "text",
            "",
            "",
            &text(&card["cardholderName"]),
        );
        put(
            &mut item,
            "number",
            "concealed",
            "",
            "",
            &text(&card["number"]),
        );
        put(&mut item, "cvv", "concealed", "", "", &text(&card["code"]));
        if let (Ok(month), Ok(year)) = (
            text(&card["expMonth"]).parse::<u32>(),
            text(&card["expYear"]).parse::<u32>(),
        ) {
            let year = if year < 100 { year + 2000 } else { year };
            put(
                &mut item,
                "expiry",
                "month",
                "",
                "",
                &format!("{year:04}-{month:02}"),
            );
        }
        put(&mut item, "", "text", "Brand", "", &text(&card["brand"]));
        let identity = &entry["identity"];
        let name = ["firstName", "middleName", "lastName"]
            .iter()
            .map(|k| text(&identity[*k]))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        put(&mut item, "name", "text", "", "", &name);
        put(
            &mut item,
            "email",
            "email",
            "",
            "",
            &text(&identity["email"]),
        );
        put(
            &mut item,
            "phone",
            "phone",
            "",
            "",
            &text(&identity["phone"]),
        );
        let address = [
            "address1",
            "address2",
            "address3",
            "city",
            "state",
            "postalCode",
            "country",
        ]
        .iter()
        .map(|k| text(&identity[*k]))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        put(&mut item, "address", "text", "", "", &address);
        put(
            &mut item,
            "id_number",
            "concealed",
            "",
            "",
            &text(&identity["ssn"]),
        );
        for (key, label, kind) in [
            ("company", "Company", "text"),
            ("username", "Username", "text"),
            ("passportNumber", "Passport", "concealed"),
            ("licenseNumber", "License", "concealed"),
        ] {
            put(&mut item, "", kind, label, "", &text(&identity[key]));
        }
        let ssh = &entry["sshKey"];
        put(
            &mut item,
            "private_key",
            "concealed",
            "",
            "",
            &text(&ssh["privateKey"]),
        );
        put(
            &mut item,
            "public_key",
            "text",
            "",
            "",
            &text(&ssh["publicKey"]),
        );
        put(
            &mut item,
            "fingerprint",
            "text",
            "",
            "",
            &text(&ssh["keyFingerprint"]),
        );
        for field in entry["fields"].as_array().into_iter().flatten() {
            let kind = match field["type"].as_u64() {
                Some(1) => "concealed",
                Some(0 | 2) => "text",
                _ => {
                    if populated(&field["value"]) || populated(&field["linkedId"]) {
                        omit("unknown_field", "unsupported_custom_field_type");
                    }
                    continue;
                }
            };
            put(
                &mut item,
                "",
                kind,
                &text(&field["name"]),
                "",
                &text(&field["value"]),
            );
        }
        let before = items.len();
        keep(item, &mut items, &mut skipped);
        if items.len() == before {
            omit("invalid_item", "invalid_item");
        }
        incomplete += usize::from(
            items.len() > before
                && omissions
                    .iter()
                    .any(|o| o.source_item_index == source_item_index),
        );
    }
    Ok(Imported {
        digest: String::new(),
        format: "bitwarden",
        items,
        skipped,
        incomplete,
        omissions,
        attachments: vec![],
        archive: None,
    })
}

// CSV

/// RFC 4180 records: reject broken quoting instead of merging or shifting secrets.
fn records(text: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let (mut row, mut field) = (Vec::new(), String::new());
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => {
                quoted = false;
                closed = true;
            }
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() && !closed => quoted = true,
            (false, ',') => {
                row.push(std::mem::take(&mut field));
                closed = false;
            }
            (false, '\r' | '\n') => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                closed = false;
            }
            (false, '"') => return Err(invalid("format")),
            (false, _) if closed => return Err(invalid("format")),
            (false, c) => field.push(c),
        }
    }
    ensure(!quoted, invalid("format"))?;
    if closed || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.is_empty()));
    Ok(rows)
}

fn csv(text: &str) -> Result<Imported> {
    let rows = records(text)?;
    let (header, rows) = rows.split_first().ok_or_else(|| invalid("format"))?;
    let column = |names: &[&str]| {
        header
            .iter()
            .position(|h| names.contains(&h.trim().to_lowercase().as_str()))
    };
    let title = column(&["title", "name"]);
    let url = column(&["url", "login_uri", "website", "web site", "uri"]);
    let username = column(&["username", "login_username", "user name", "login name"]);
    let password = column(&["password", "login_password"]);
    let otp = column(&["otpauth", "login_totp", "totp", "otp", "one-time password"]);
    let notes = column(&["notes", "note", "extra", "comments"]);
    let tags = column(&["tags", "folder", "grouping", "group"]);
    let favorite = column(&["favorite", "fav"]);
    let archived = column(&["archived"]);
    let kind = column(&["type"]);
    let extra = column(&["fields"]);
    ensure(
        [title, url, username, password].iter().any(Option::is_some),
        invalid("format"),
    )?;
    let (mut items, mut skipped) = (Vec::new(), 0);
    let mut omissions = Vec::new();
    let mut incomplete = 0;
    let mapped: std::collections::BTreeSet<_> = [
        title, url, username, password, otp, notes, tags, favorite, archived, kind, extra,
    ]
    .into_iter()
    .flatten()
    .collect();
    for (source_item_index, row) in rows.iter().enumerate() {
        let mut omit = |category: &str, reason: &str| {
            omissions.push(Omission {
                source_item_index,
                category: category.into(),
                reason: reason.into(),
            })
        };
        if row.len() != header.len() {
            skipped += 1;
            omit("invalid_item", "row_length_or_archived");
            continue;
        }
        let get = |at: Option<usize>| at.and_then(|i| row.get(i)).map_or("", |s| s.as_str());
        let yes = |at| matches!(get(at).trim().to_lowercase().as_str(), "1" | "true" | "yes");
        if yes(archived) {
            skipped += 1;
            omit("invalid_item", "row_length_or_archived");
            continue;
        }
        let login = !get(username).is_empty() || !get(password).is_empty() || !get(url).is_empty();
        let item_kind = match get(kind) {
            "note" | "securenote" => "note",
            _ if login => "login",
            _ => "note",
        };
        let mut item = Item::new(item_kind, get(title), 0);
        put(&mut item, "username", "text", "", "", get(username));
        put(&mut item, "password", "concealed", "", "", get(password));
        put(&mut item, "url", "url", "", "", get(url));
        put(&mut item, "", "otp", "", "", get(otp));
        item.notes = get(notes).into();
        item.favorite = yes(favorite);
        item.tags = get(tags)
            .split([',', ';'])
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        // Bitwarden CSV discards custom field types and allows multiline
        // values. Keep the complete cell concealed; splitting it into
        // lines loses data and can expose a field that was hidden.
        put(&mut item, "", "concealed", "Custom fields", "", get(extra));
        if row
            .iter()
            .enumerate()
            .any(|(i, v)| !mapped.contains(&i) && !v.is_empty())
        {
            omit("unknown_field", "unmapped_csv_columns");
        }
        if !get(extra).is_empty() {
            omit("field_metadata", "custom_field_types_not_in_csv");
        }
        let before = items.len();
        keep(item, &mut items, &mut skipped);
        if items.len() == before {
            omit("invalid_item", "invalid_item");
        }
        incomplete += usize::from(
            items.len() > before
                && omissions
                    .iter()
                    .any(|o| o.source_item_index == source_item_index),
        );
    }
    Ok(Imported {
        digest: String::new(),
        format: "csv",
        items,
        skipped,
        incomplete,
        omissions,
        attachments: vec![],
        archive: None,
    })
}

fn digest(items: &[Item]) -> String {
    hex::encode(*sha256(&Zeroizing::new(canonical(&items))))
}

fn key(item: &Item) -> String {
    // Imports lacking timestamps receive them on save. All user content,
    // including secrets, must match before an item is unticked by default.
    let mut content = item.clone();
    content.created_at = 0;
    content.updated_at = 0;
    content.resolved_conflicts = None;
    digest(&[content])
}

fn shown<'a>(item: &'a Item, id: &str) -> &'a str {
    item.field(id)
        .filter(|f| !f.hidden())
        .map_or("", |f| f.value.as_str())
}

impl Imported {
    /// Build a preview against the current items. Hidden values are used
    /// only for duplicate detection and never included in the result.
    pub fn preview(&self, entries: &[super::VaultEntry]) -> Preview {
        let mut here: BTreeSet<_> = entries.iter().map(|e| key(&e.item)).collect();
        Preview {
            format: self.format,
            digest: self.digest.clone(),
            skipped: self.skipped,
            incomplete: self.incomplete,
            omissions: self.omissions.clone(),
            attachments: self.attachments.clone(),
            items: self
                .items
                .iter()
                .enumerate()
                .map(|(index, item)| PreviewItem {
                    index,
                    kind: item.kind.clone(),
                    title: item.title.clone(),
                    username: shown(item, "username").into(),
                    url: shown(item, "url").into(),
                    tags: item.tags.clone(),
                    fields: item.fields.iter().filter(|f| !f.value.is_empty()).count(),
                    duplicate: !here.insert(key(item)),
                })
                .collect(),
        }
    }

    /// Stream explicitly selected attachments from the already hashed source snapshot.
    /// Run on a blocking worker; `emit` provides bounded backpressure and cancellation.
    pub fn stream_selected(
        self,
        digest_seen: &str,
        picks: &[usize],
        attachments: &[String],
        mut emit: impl FnMut(AttachmentEvent) -> Result<()>,
    ) -> Result<Vec<Item>> {
        ensure(self.digest == digest_seen, Error::VersionConflict)?;
        let chosen: BTreeSet<_> = picks.iter().copied().collect();
        ensure(
            chosen.len() == picks.len() && chosen.iter().all(|i| *i < self.items.len()),
            invalid("picks"),
        )?;
        let selected: BTreeSet<_> = attachments.iter().collect();
        ensure(selected.len() == attachments.len(), invalid("attachments"))?;
        let candidates = selected
            .iter()
            .map(|id| {
                self.attachments
                    .iter()
                    .find(|a| {
                        &a.entry_id == *id
                            && a.supported
                            && a.item_index.is_some_and(|i| chosen.contains(&i))
                    })
                    .ok_or_else(|| invalid("attachment selection"))
            })
            .collect::<Result<Vec<_>>>()?;
        ensure(
            candidates.iter().try_fold(0u64, |n, a| {
                n.checked_add(a.size).ok_or(Error::QuotaExceeded)
            })? <= 256 * 1024 * 1024,
            Error::QuotaExceeded,
        )?;
        if !candidates.is_empty() {
            let archive = self
                .archive
                .as_ref()
                .ok_or_else(|| invalid("attachments"))?;
            let mut zip =
                zip::ZipArchive::new(Cursor::new(&archive[..])).map_err(|_| invalid("archive"))?;
            for a in candidates {
                let item_index = chosen
                    .iter()
                    .position(|i| Some(*i) == a.item_index)
                    .ok_or_else(|| invalid("attachment item"))?;
                emit(AttachmentEvent::Start {
                    item_index,
                    name: a.name.clone(),
                    size: a.size,
                })?;
                let mut file = zip
                    .by_name(&a.member)
                    .map_err(|_| invalid("attachment member"))?;
                let mut remaining = a.size;
                while remaining > 0 {
                    let length = remaining.min(crate::files::CHUNK as u64) as usize;
                    let mut buffer = Zeroizing::new(vec![0; length]);
                    file.read_exact(&mut buffer)
                        .map_err(|_| Error::IntegrityFailed)?;
                    remaining -= length as u64;
                    emit(AttachmentEvent::Chunk(buffer))?;
                }
                let mut extra = [0];
                ensure(
                    file.read(&mut extra).map_err(|_| Error::IntegrityFailed)? == 0,
                    Error::IntegrityFailed,
                )?;
                emit(AttachmentEvent::End)?;
            }
        }
        Ok(self
            .items
            .into_iter()
            .enumerate()
            .filter_map(|(i, item)| chosen.contains(&i).then_some(item))
            .collect())
    }

    /// Select the preview's items after checking the source file's digest.
    /// # Errors
    /// Returns `Error::VersionConflict` for changed content and
    /// `Error::InvalidInput("picks")` for an index the export lacks.
    pub fn select(self, digest_seen: &str, picks: &[usize]) -> Result<Vec<Item>> {
        ensure(self.digest == digest_seen, Error::VersionConflict)?;
        let picked: BTreeSet<usize> = picks.iter().copied().collect();
        ensure(
            picked.iter().all(|i| *i < self.items.len()),
            invalid("picks"),
        )?;
        Ok(self
            .items
            .into_iter()
            .enumerate()
            .filter(|(i, _)| picked.contains(i))
            .map(|(_, item)| item)
            .collect())
    }
}

impl Client {
    /// Read an export and show what importing it would add.
    ///
    /// # Errors
    /// As [`read`], and `Error::Locked` while locked.
    pub fn vault_import_preview(&self, path: &Path) -> Result<Preview> {
        self.roots()?;
        let imported = read(path)?;
        Ok(imported.preview(&self.vault_list(&Filter::default())?))
    }

    /// Add the items `picks` names of an export whose preview had `digest`.
    /// Returns the new IDs.
    ///
    /// # Errors
    /// Returns `Error::VersionConflict` when the file changed since its
    /// preview and `Error::InvalidInput("picks")` for an index it lacks.
    pub fn vault_import(
        &mut self,
        path: &Path,
        digest_seen: &str,
        picks: &[usize],
    ) -> Result<Vec<String>> {
        self.roots()?;
        self.vault_add_all(read(path)?.select(digest_seen, picks)?)
    }
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
