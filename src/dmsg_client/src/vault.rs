//! The vault: items as encrypted versions (see [`crate::content`]). Files
//! are not vault items; they live in the file library ([`crate::library`]),
//! and an item only names the library files attached to it.
//!
//! An item has a type, a title, tags, notes and an ordered list of fields.
//! A new item starts with its type's fields ([`template`]), named by ID;
//! people add their own. Concealed and one-time-password fields leave the
//! daemon only on explicit request ([`VaultEntry::masked`]).
//!
//! Editing writes a version whose parent is the version edited; deleting
//! writes a tombstone and restoring writes a version on top of it, so
//! history is never overwritten and every version stays readable
//! ([`Client::vault_history`]). A version that lost a race is a conflict
//! copy: resolving it writes a new head that names it in
//! `resolvedConflicts`, keeping either side or a field-by-field merge.
//!
//! dMsg's own keys (Agent IDs and Agent Delegation controllers) are items
//! tagged `dmsg-…` that only the client itself writes: no edit, trash,
//! merge or copy reaches them, and no other item takes such a tag.

pub mod health;
pub mod import;
pub mod otp;
pub mod reading;
pub mod ssh;

use crate::client::Client;
use crate::codec::{canonical, decode, decode_value};
use crate::content::{EncryptedObject, MAX_OBJECT, VAULT, record};
use crate::crypto::random_key;
use crate::store::{LOCAL, ObjectRow, REPLACED, SYNCED};
use crate::upload::random_hex;
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use zeroize::Zeroizing;

/// The tag prefix of dMsg's own keys in the vault: Agent ID and controller
/// keys. No agent capability reaches an item tagged with it.
pub const RESERVED_TAG: &str = "dmsg-";
/// Item types, in the order a new item offers them.
pub const TYPES: [&str; 11] = [
    "login", "api", "key", "ssh", "wallet", "card", "identity", "server", "database", "license",
    "note",
];
/// Field types. `concealed` and `otp` are hidden; `date` is `YYYY-MM-DD`
/// and `month` is `YYYY-MM`.
pub const FIELD_TYPES: [&str; 8] = [
    "text",
    "concealed",
    "url",
    "email",
    "phone",
    "date",
    "month",
    "otp",
];
/// What a hidden value reads as when it is not revealed.
pub const MASK: &str = "********";

const MAX_FIELDS: usize = 64;
const MAX_FILES: usize = 20;
/// Notes, labels and values together, in bytes.
const MAX_TEXT: usize = 64 * 1024;

/// The fields a new item of `kind` starts with, as `(id, field type)`.
pub fn template(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "login" => &[
            ("username", "text"),
            ("password", "concealed"),
            ("url", "url"),
        ],
        "api" => &[
            ("username", "text"),
            ("credential", "concealed"),
            ("url", "url"),
            ("expires", "date"),
        ],
        "key" => &[("key", "concealed")],
        "ssh" => &[
            ("private_key", "concealed"),
            ("public_key", "text"),
            ("fingerprint", "text"),
        ],
        "wallet" => &[
            ("phrase", "concealed"),
            ("private_key", "concealed"),
            ("address", "text"),
            ("network", "text"),
            ("password", "concealed"),
        ],
        "card" => &[
            ("cardholder", "text"),
            ("number", "concealed"),
            ("expiry", "month"),
            ("cvv", "concealed"),
            ("pin", "concealed"),
        ],
        "identity" => &[
            ("name", "text"),
            ("email", "email"),
            ("phone", "phone"),
            ("address", "text"),
            ("birthday", "date"),
            ("id_number", "concealed"),
        ],
        "server" => &[
            ("url", "url"),
            ("username", "text"),
            ("password", "concealed"),
        ],
        "database" => &[
            ("engine", "text"),
            ("host", "text"),
            ("port", "text"),
            ("database", "text"),
            ("username", "text"),
            ("password", "concealed"),
        ],
        "license" => &[
            ("license_key", "concealed"),
            ("licensed_to", "text"),
            ("email", "email"),
            ("purchased", "date"),
            ("expires", "date"),
        ],
        _ => &[],
    }
}

/// The hidden field that holds a type's main secret: what the CLI's
/// `--secret-stdin` sets and what copying "the secret" copies.
pub fn primary_secret(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "login" | "server" | "database" => "password",
        "api" => "credential",
        "key" => "key",
        "ssh" => "private_key",
        "wallet" => "phrase",
        "card" => "number",
        "identity" => "id_number",
        "license" => "license_key",
        _ => return None,
    })
}

/// A random field ID.
fn field_id() -> String {
    hex::encode(&random_key()[..4])
}

/// One field of an item.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// Stable within the item: a template field's own ID, a random one, or
    /// an imported field's place (`f3`).
    pub id: String,
    /// One of [`FIELD_TYPES`].
    #[serde(rename = "type")]
    pub kind: String,
    /// The name people gave it; empty keeps a template field's own name.
    pub label: String,
    /// The heading it sits under; empty for the item's first section.
    pub section: String,
    /// The value; may span lines.
    pub value: String,
}

impl Field {
    /// A field without a label or section.
    pub fn new(id: &str, kind: &str, value: &str) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            label: String::new(),
            section: String::new(),
            value: value.into(),
        }
    }

    /// Concealed and one-time-password fields: shown only on request.
    pub fn hidden(&self) -> bool {
        matches!(self.kind.as_str(), "concealed" | "otp")
    }

    fn validate(&self) -> Result<()> {
        let chars = |s: &str| s.chars().count();
        ensure_valid(
            (1..=40).contains(&self.id.len())
                && self
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "field id",
        )?;
        ensure_valid(FIELD_TYPES.contains(&self.kind.as_str()), "field type")?;
        ensure_valid(chars(&self.label) <= 100, "field label")?;
        ensure_valid(chars(&self.section) <= 100, "field section")?;
        ensure_valid(chars(&self.value) <= 10_000, "field value")?;
        if self.value.is_empty() {
            return Ok(());
        }
        match self.kind.as_str() {
            "otp" => otp::parse(&self.value).map(|_| ()),
            "date" => ensure_valid(health::days(&self.value).is_some(), "field date"),
            "month" => ensure_valid(health::month_end(&self.value).is_some(), "field month"),
            _ => Ok(()),
        }
    }
}

/// A vault item's plaintext.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Item {
    /// One of [`TYPES`].
    #[serde(rename = "type")]
    pub kind: String,
    /// Title, 1 to 200 characters.
    pub title: String,
    /// Up to 20 distinct tags of at most 40 characters.
    pub tags: Vec<String>,
    /// Free text.
    pub notes: String,
    /// The fields, in order.
    pub fields: Vec<Field>,
    /// Attached files of the library, by ID.
    pub files: Vec<String>,
    /// Starred.
    pub favorite: bool,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
    /// Last edit in Unix milliseconds.
    pub updated_at: u64,
    /// Conflict copies, as `id:revision`, this version resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_conflicts: Option<Vec<String>>,
}

/// An item as written before it had fields (until 2026-10-18): a user
/// name, one secret and a URL. Such versions stay in every account's
/// history, so they are read as [`Item`]s.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Previous {
    #[serde(rename = "type")]
    kind: String,
    title: String,
    tags: Vec<String>,
    body: String,
    username: String,
    secret: String,
    url: String,
    favorite: bool,
    created_at: u64,
    updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolved_conflicts: Option<Vec<String>>,
}

impl From<Previous> for Item {
    fn from(previous: Previous) -> Self {
        let mut item = Item::new(&previous.kind, &previous.title, previous.created_at);
        let secret = primary_secret(&previous.kind).unwrap_or("secret");
        for (id, kind, value) in [
            ("username", "text", previous.username),
            (secret, "concealed", previous.secret),
            ("url", "url", previous.url),
        ] {
            match item.fields.iter_mut().find(|f| f.id == id) {
                Some(field) => field.value = value,
                None if !value.is_empty() => item.fields.push(Field::new(id, kind, &value)),
                None => {}
            }
        }
        item.tags = previous.tags;
        item.notes = previous.body;
        item.favorite = previous.favorite;
        item.updated_at = previous.updated_at;
        item.resolved_conflicts = previous.resolved_conflicts;
        item
    }
}

/// Open an item's plaintext, in either format.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for anything else.
pub fn open_item(payload: &[u8]) -> Result<Item> {
    decode::<Item>(payload, MAX_OBJECT)
        .or_else(|_| decode::<Previous>(payload, MAX_OBJECT).map(Item::from))
}

/// The conflict copies an item's plaintext resolved, read without the rest
/// of its format, so that syncing never depends on the item format.
pub(crate) fn resolved_in(payload: &[u8]) -> Vec<String> {
    use cbor2::Value;
    let Ok(Value::Map(map)) = decode_value(payload, MAX_OBJECT) else {
        return Vec::new();
    };
    map.into_iter()
        .find_map(|entry| match entry {
            (Value::Text(key), Value::Array(list)) if key == "resolvedConflicts" => Some(list),
            _ => None,
        })
        .into_iter()
        .flatten()
        .filter_map(|v| match v {
            Value::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

impl Item {
    /// A new item of `kind` with its template's fields, empty.
    pub fn new(kind: &str, title: &str, now: u64) -> Self {
        Self {
            kind: kind.into(),
            title: title.into(),
            tags: Vec::new(),
            notes: String::new(),
            fields: template(kind)
                .iter()
                .map(|(id, kind)| Field::new(id, kind, ""))
                .collect(),
            files: Vec::new(),
            favorite: false,
            created_at: now,
            updated_at: now,
            resolved_conflicts: None,
        }
    }

    /// The field `id`.
    pub fn field(&self, id: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.id == id)
    }

    /// The value of field `id`; empty when there is none.
    pub fn value(&self, id: &str) -> &str {
        self.field(id).map_or("", |f| f.value.as_str())
    }

    /// Whether this is one of dMsg's own keys, which agents never read,
    /// write or reveal and people only read.
    pub fn reserved(&self) -> bool {
        self.tags.iter().any(|t| t.starts_with(RESERVED_TAG))
    }

    /// Whether this edit of `old` keeps each of its hidden fields hidden: a
    /// field kept by ID under a type shown in the clear would read its
    /// value back.
    pub fn keeps_hidden(&self, old: &Item) -> bool {
        old.fields
            .iter()
            .filter(|f| f.hidden())
            .all(|f| self.field(&f.id).is_none_or(Field::hidden))
    }

    /// Trim the title, labels and sections; drop empty and repeated tags
    /// (ignoring case) and repeated files; give new fields an ID.
    pub fn normalize(&mut self) {
        self.title = self.title.trim().into();
        let mut seen = BTreeSet::new();
        self.tags = std::mem::take(&mut self.tags)
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
            .collect();
        for field in &mut self.fields {
            field.label = field.label.trim().into();
            field.section = field.section.trim().into();
            if field.id.is_empty() {
                field.id = field_id();
            }
            if matches!(
                field.kind.as_str(),
                "url" | "email" | "date" | "month" | "otp"
            ) {
                field.value = field.value.trim().into();
            }
        }
        let mut seen = BTreeSet::new();
        self.files.retain(|f| seen.insert(f.clone()));
        if self.kind == "ssh" {
            ssh::complete(self);
        }
    }

    /// Check the item limits of the vault.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` naming the field, or
    /// `Error::QuotaExceeded` when the notes, labels and values together
    /// exceed 64 KiB.
    pub fn validate(&self) -> Result<()> {
        let chars = |s: &str| s.chars().count();
        ensure_valid(TYPES.contains(&self.kind.as_str()), "type")?;
        ensure_valid(
            !self.title.trim().is_empty() && chars(self.title.trim()) <= 200,
            "title",
        )?;
        let mut seen = BTreeSet::new();
        ensure_valid(
            self.tags.len() <= 20
                && self.tags.iter().all(|t| {
                    !t.trim().is_empty() && chars(t) <= 40 && seen.insert(t.to_lowercase())
                }),
            "tags",
        )?;
        ensure_valid(chars(&self.notes) <= 20_000, "notes")?;
        ensure_valid(self.fields.len() <= MAX_FIELDS, "fields")?;
        let mut ids = BTreeSet::new();
        for field in &self.fields {
            field.validate()?;
            ensure_valid(ids.insert(field.id.as_str()), "field id")?;
        }
        let mut files = BTreeSet::new();
        ensure_valid(
            self.files.len() <= MAX_FILES
                && self.files.iter().all(|f| {
                    f.len() == 64 && f.bytes().all(|b| b.is_ascii_hexdigit()) && files.insert(f)
                }),
            "files",
        )?;
        let text = self.notes.len()
            + self
                .fields
                .iter()
                .map(|f| f.label.len() + f.section.len() + f.value.len())
                .sum::<usize>();
        ensure(text <= MAX_TEXT, Error::QuotaExceeded)
    }

    /// Whether `query` is in the title, tags, notes, a field's label or a
    /// shown field's value, ignoring case. Hidden values are never searched.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        let has = |text: &str| text.to_lowercase().contains(&query);
        has(&self.title)
            || has(&self.notes)
            || self.tags.iter().any(|t| has(t))
            || self
                .fields
                .iter()
                .any(|f| has(&f.label) || (!f.hidden() && has(&f.value)))
    }

    fn resolved(&self) -> BTreeSet<String> {
        self.resolved_conflicts.iter().flatten().cloned().collect()
    }

    /// The same content, ignoring when it was last edited and what it
    /// resolved.
    fn same(&self, other: &Item) -> bool {
        let strip = |item: &Item| Item {
            updated_at: 0,
            resolved_conflicts: None,
            ..item.clone()
        };
        strip(self) == strip(other)
    }
}

/// A field as a change names it: an ID or a label to find it by, and the
/// parts that change.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FieldChange {
    /// The field's ID; a new field without one gets a random ID.
    #[serde(default)]
    pub id: Option<String>,
    /// Its type; a new field without one takes its template type, or text.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// Its label.
    #[serde(default)]
    pub label: Option<String>,
    /// Its section.
    #[serde(default)]
    pub section: Option<String>,
    /// Its value; left out, an existing field keeps its value.
    #[serde(default)]
    pub value: Option<String>,
}

impl FieldChange {
    /// The field named by ID, else by its name: its label, or a template
    /// field's ID when it has no label of its own.
    fn found_in(&self, fields: &[Field]) -> Option<usize> {
        match (&self.id, &self.label) {
            (Some(id), _) => fields.iter().position(|f| &f.id == id),
            (None, Some(label)) => fields.iter().position(|f| {
                let name = match f.label.trim() {
                    "" => f.id.as_str(),
                    own => own,
                };
                name.eq_ignore_ascii_case(label.trim())
            }),
            (None, None) => None,
        }
    }

    fn apply(&self, field: &mut Field) {
        if let Some(kind) = &self.kind {
            field.kind = kind.clone();
        }
        if let Some(label) = &self.label {
            field.label = label.clone();
        }
        if let Some(section) = &self.section {
            field.section = section.clone();
        }
        if let Some(value) = &self.value {
            field.value = value.clone();
        }
    }

    fn create(&self, item_kind: &str) -> Field {
        let id = self.id.clone().unwrap_or_default();
        let kind = template(item_kind)
            .iter()
            .find(|(t, _)| *t == id)
            .map_or("text", |(_, kind)| kind);
        let mut field = Field::new(&id, kind, "");
        self.apply(&mut field);
        field
    }
}

/// What an edit changes. Everything left out stays.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Changes {
    /// The title.
    #[serde(default)]
    pub title: Option<String>,
    /// The notes.
    #[serde(default)]
    pub notes: Option<String>,
    /// All tags.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Tags to add.
    #[serde(default)]
    pub add_tags: Option<Vec<String>>,
    /// Tags to take off.
    #[serde(default)]
    pub remove_tags: Option<Vec<String>>,
    /// Starred.
    #[serde(default)]
    pub favorite: Option<bool>,
    /// All attached files.
    #[serde(default)]
    pub files: Option<Vec<String>>,
    /// All fields, in order: a field named by an ID it already has keeps
    /// what the change leaves out, its hidden value included; a field left
    /// out is removed.
    #[serde(default)]
    pub fields: Option<Vec<FieldChange>>,
    /// Fields to change by ID or label, or to add after the others.
    #[serde(default)]
    pub set: Option<Vec<FieldChange>>,
    /// Fields to remove, by ID or label.
    #[serde(default)]
    pub remove: Option<Vec<String>>,
}

impl Changes {
    /// Apply the changes to `item`, unchecked.
    pub fn apply(&self, item: &mut Item) {
        if let Some(title) = &self.title {
            item.title = title.clone();
        }
        if let Some(notes) = &self.notes {
            item.notes = notes.clone();
        }
        if let Some(tags) = &self.tags {
            item.tags = tags.clone();
        }
        item.tags.extend(self.add_tags.iter().flatten().cloned());
        if let Some(remove) = &self.remove_tags {
            item.tags.retain(|t| {
                !remove
                    .iter()
                    .any(|r| r.trim().eq_ignore_ascii_case(t.trim()))
            });
        }
        if let Some(favorite) = self.favorite {
            item.favorite = favorite;
        }
        if let Some(files) = &self.files {
            item.files = files.clone();
        }
        if let Some(list) = &self.fields {
            let old = std::mem::take(&mut item.fields);
            for change in list {
                let field = match change
                    .id
                    .as_ref()
                    .and_then(|id| old.iter().find(|f| &f.id == id))
                {
                    Some(existing) => {
                        let mut field = existing.clone();
                        change.apply(&mut field);
                        field
                    }
                    None => change.create(&item.kind),
                };
                item.fields.push(field);
            }
        }
        for change in self.set.iter().flatten() {
            match change.found_in(&item.fields) {
                // A field found by its name keeps it as it is.
                Some(at) if change.id.is_none() => FieldChange {
                    label: None,
                    ..change.clone()
                }
                .apply(&mut item.fields[at]),
                Some(at) => change.apply(&mut item.fields[at]),
                None => item.fields.push(change.create(&item.kind)),
            }
        }
        for name in self.remove.iter().flatten() {
            item.fields
                .retain(|f| &f.id != name && !f.label.trim().eq_ignore_ascii_case(name.trim()));
        }
    }
}

/// One version of an item, opened.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct VaultEntry {
    /// Item ID.
    pub id: String,
    /// This version's revision.
    pub revision: String,
    /// Plaintext.
    pub item: Item,
    /// The relay holds this version.
    pub synced: bool,
    /// A conflict copy beside the item's head.
    pub conflict: bool,
    /// The item is in the trash.
    pub deleted: bool,
    /// When this version was written, in Unix milliseconds: for an item in
    /// the trash, when it was deleted.
    pub at: u64,
}

impl VaultEntry {
    pub(crate) fn new(record: &EncryptedObject, item: Item, synced: bool, conflict: bool) -> Self {
        Self {
            id: record.id.clone(),
            revision: record.revision.clone(),
            item,
            synced,
            conflict,
            deleted: record.tombstone,
            at: record.created_at,
        }
    }

    /// The entry as it may leave the daemon: hidden values only when
    /// revealed.
    pub fn masked(mut self, reveal: bool) -> Self {
        if !reveal {
            for field in &mut self.item.fields {
                if field.hidden() && !field.value.is_empty() {
                    field.value = MASK.into();
                }
            }
        }
        self
    }
}

/// One version in an item's history.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// The revision.
    pub revision: String,
    /// The revision it replaced.
    pub parent: Option<String>,
    /// When it was written, in Unix milliseconds.
    pub at: u64,
    /// The device that wrote it, hex.
    pub device: String,
    /// A deletion.
    pub deleted: bool,
    /// A conflict copy.
    pub conflict: bool,
    /// The relay holds it.
    pub synced: bool,
    /// The item's current version.
    pub current: bool,
}

/// A conflict copy beside the item's current version.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// The copy.
    pub copy: VaultEntry,
    /// The current version.
    pub head: VaultEntry,
    /// The hidden fields whose values differ, by ID; a hidden field on one
    /// side only counts.
    pub hidden: Vec<String>,
}

impl Conflict {
    /// Both sides as they may leave the daemon.
    pub fn masked(self, reveal: bool) -> Self {
        Self {
            copy: self.copy.masked(reveal),
            head: self.head.masked(reveal),
            hidden: self.hidden,
        }
    }
}

/// A side of a conflict.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// The current version.
    Current,
    /// The conflict copy.
    Theirs,
}

/// How a conflict copy is settled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keep {
    /// The current version.
    Current,
    /// The copy.
    Theirs,
    /// The current version with the parts `picks` names taken from the
    /// copy: `title`, `tags`, `notes`, `favorite`, `files` or
    /// `field:<id>` (a field only the copy has is added, one it lacks is
    /// removed); `notes`, when given, are the merged text.
    Merge {
        /// Which side each part comes from; current when left out.
        picks: BTreeMap<String, Side>,
        /// Merged notes.
        notes: Option<String>,
    },
}

fn merge(
    current: &Item,
    theirs: &Item,
    picks: &BTreeMap<String, Side>,
    notes: Option<&str>,
) -> Item {
    let mut item = current.clone();
    for (part, side) in picks {
        if *side != Side::Theirs {
            continue;
        }
        match part.as_str() {
            "title" => item.title = theirs.title.clone(),
            "tags" => item.tags = theirs.tags.clone(),
            "notes" => item.notes = theirs.notes.clone(),
            "favorite" => item.favorite = theirs.favorite,
            "files" => item.files = theirs.files.clone(),
            part => {
                let Some(id) = part.strip_prefix("field:") else {
                    continue;
                };
                match (
                    item.fields.iter().position(|f| f.id == id),
                    theirs.field(id),
                ) {
                    (Some(at), Some(field)) => item.fields[at] = field.clone(),
                    (Some(at), None) => {
                        item.fields.remove(at);
                    }
                    (None, Some(field)) => item.fields.push(field.clone()),
                    (None, None) => {}
                }
            }
        }
    }
    if let Some(notes) = notes {
        item.notes = notes.into();
    }
    item
}

/// Which items [`Client::vault_list`] returns.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// Deleted items instead of current ones.
    pub trash: bool,
    /// Case-insensitive text in the title, tags, notes, field labels or
    /// shown field values.
    pub query: Option<String>,
}

impl Client {
    fn open_row(&self, row: &ObjectRow) -> Result<(EncryptedObject, Item)> {
        let record = record(row)?;
        ensure(record.kind == VAULT, Error::NotFound)?;
        let item = open_item(&record.open(self.environment(), self.roots()?)?)?;
        Ok((record, item))
    }

    fn entry(&self, row: &ObjectRow) -> Result<VaultEntry> {
        let (record, item) = self.open_row(row)?;
        Ok(VaultEntry::new(
            &record,
            item,
            row.state == SYNCED,
            row.conflict,
        ))
    }

    fn version_row(&self, id: &str, revision: &str) -> Result<ObjectRow> {
        self.workspace
            .store
            .object(&format!("{id}:{revision}"))?
            .filter(|row| row.kind == VAULT)
            .ok_or(Error::NotFound)
    }

    fn put_item(
        &mut self,
        id: String,
        parent: Option<String>,
        tombstone: bool,
        item: &Item,
    ) -> Result<VaultEntry> {
        let payload = Zeroizing::new(canonical(item));
        let (record, conflict) = self.write(VAULT, id, parent, tombstone, &payload, None)?;
        Ok(VaultEntry::new(&record, item.clone(), false, conflict))
    }

    /// Add an item of `kind`: its template's fields with `changes` applied.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked, `Error::InvalidInput` naming
    /// what is invalid, and `Error::Forbidden` for a `dmsg-` tag.
    pub fn vault_create(&mut self, kind: &str, changes: &Changes) -> Result<VaultEntry> {
        ensure_valid(TYPES.contains(&kind), "type")?;
        let mut item = Item::new(kind, "", crate::now_ms());
        changes.apply(&mut item);
        self.vault_add(item)
    }

    /// Add a whole item as it is, with a new ID.
    fn vault_add(&mut self, mut item: Item) -> Result<VaultEntry> {
        item.resolved_conflicts = None;
        item.normalize();
        ensure(!item.reserved(), Error::Forbidden)?;
        item.validate()?;
        self.put_item(random_hex(), None, false, &item)
    }

    /// Edit item `id`: `changes` applied to version `base`, by default the
    /// current one. When the head has moved past `base` the edit is kept
    /// as a conflict copy; an edit that changes nothing writes nothing.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked, `Error::NotFound` for an
    /// unknown or deleted item, `Error::Forbidden` for dMsg's own keys or
    /// a `dmsg-` tag, and `Error::InvalidInput` naming what is invalid.
    pub fn vault_edit(
        &mut self,
        id: &str,
        base: Option<&str>,
        changes: &Changes,
    ) -> Result<VaultEntry> {
        let (head, _) = self.current(id)?;
        let base = base.unwrap_or(&head.revision).to_string();
        let row = self.version_row(id, &base)?;
        let (record, old) = self.open_row(&row)?;
        ensure(!record.tombstone, Error::NotFound)?;
        ensure(!old.reserved(), Error::Forbidden)?;
        let mut item = old.clone();
        changes.apply(&mut item);
        item.normalize();
        ensure(!item.reserved(), Error::Forbidden)?;
        item.validate()?;
        if item.same(&old) {
            return Ok(VaultEntry::new(
                &record,
                old,
                row.state == SYNCED,
                row.conflict,
            ));
        }
        item.updated_at = crate::now_ms();
        self.put_item(id.into(), Some(base), false, &item)
    }

    /// Add a copy of item `id` titled `title`, hidden values included.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown or deleted item and
    /// `Error::Forbidden` for dMsg's own keys.
    pub fn vault_duplicate(&mut self, id: &str, title: &str) -> Result<VaultEntry> {
        let (row, record) = self.current(id)?;
        ensure(!record.tombstone, Error::NotFound)?;
        let (_, mut item) = self.open_row(&row)?;
        ensure(!item.reserved(), Error::Forbidden)?;
        let now = crate::now_ms();
        item.title = title.into();
        item.favorite = false;
        item.created_at = now;
        item.updated_at = now;
        self.vault_add(item)
    }

    /// Add items read from an export, each with a new ID. Returns their IDs.
    ///
    /// # Errors
    /// As [`Self::vault_create`]; a failure leaves no items or sync jobs.
    pub fn vault_add_all(&mut self, items: Vec<Item>) -> Result<Vec<String>> {
        let now = crate::now_ms();
        self.workspace
            .store
            .atomic(|_| self.vault_write_items(items, now))
    }

    /// Resume an explicitly staged import without duplicating its committed items.
    /// A different selection for the same source handle is refused.
    /// # Errors
    /// As [`Self::vault_add_all`], or `VersionConflict` for a changed selection.
    pub fn vault_add_all_once(&mut self, source: &str, items: Vec<Item>) -> Result<Vec<String>> {
        #[derive(Serialize, Deserialize)]
        struct Receipt {
            digest: String,
            ids: Vec<String>,
        }
        let key = format!(
            "vault-import:{}",
            hex::encode(*crate::codec::sha256(source.as_bytes()))
        );
        let digest = hex::encode(*crate::codec::sha256(&canonical(&items)));
        if let Some(receipt) = self.workspace.private::<Receipt>(self.keys()?, &key)? {
            ensure(receipt.digest == digest, Error::VersionConflict)?;
            return Ok(receipt.ids);
        }
        let now = crate::now_ms();
        self.workspace.store.atomic(|_| {
            let ids = self.vault_write_items(items, now)?;
            self.workspace.put_private(
                self.keys()?,
                &key,
                &Receipt {
                    digest,
                    ids: ids.clone(),
                },
            )?;
            Ok(ids)
        })
    }

    /// Write inside an existing workspace transaction, including attachment catalog writes.
    pub(crate) fn vault_write_items(&self, items: Vec<Item>, now: u64) -> Result<Vec<String>> {
        let account = self.account()?.id.to_string();
        let roots = self.roots()?;
        let generation = roots.generation;
        let root = roots.key(generation).ok_or(Error::RecoveryIncomplete)?;
        let device = hex::encode(*self.workspace.meta.device_id);
        let store = &self.workspace.store;
        let mut ids = Vec::with_capacity(items.len());
        for mut item in items {
            item.resolved_conflicts = None;
            if item.created_at == 0 {
                item.created_at = now;
            }
            if item.updated_at == 0 {
                item.updated_at = item.created_at;
            }
            item.normalize();
            ensure(!item.reserved(), Error::Forbidden)?;
            item.validate()?;
            let payload = Zeroizing::new(canonical(&item));
            ensure(payload.len() <= MAX_OBJECT, Error::QuotaExceeded)?;
            let record = EncryptedObject::seal(
                self.environment(),
                (generation, root),
                &account,
                &device,
                VAULT,
                random_hex(),
                None,
                false,
                &payload,
                now,
            );
            let job = self.prepare_job(&record, None, now)?;
            let row = ObjectRow {
                key: record.key.clone(),
                item: record.id.clone(),
                revision: record.revision.clone(),
                kind: VAULT.into(),
                frame: canonical(&record),
                state: LOCAL.into(),
                conflict: false,
                resolved: false,
                created_at: now,
            };
            store.put_object(&row)?;
            store.set_job(&row.key, &job)?;
            store.set_head(&row.item, &row.revision, VAULT)?;
            ids.push(record.id);
        }
        Ok(ids)
    }

    /// Add an item under a fixed ID, such as a key whose ID its purpose
    /// determines. Only the client's own keys are added this way.
    ///
    /// # Errors
    /// Returns `Error::IdempotencyConflict` when the ID is taken.
    pub(crate) fn vault_item_with_id(&mut self, id: &str, item: &Item) -> Result<VaultEntry> {
        ensure(
            self.workspace.store.head(id)?.is_none(),
            Error::IdempotencyConflict,
        )?;
        item.validate()?;
        self.put_item(id.into(), None, false, item)
    }

    /// Move an item to the trash: a tombstone on its current version.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown or deleted item and
    /// `Error::Forbidden` for dMsg's own keys.
    pub fn vault_delete(&mut self, id: &str) -> Result<VaultEntry> {
        let (row, record) = self.current(id)?;
        ensure(!record.tombstone, Error::NotFound)?;
        let (_, item) = self.open_row(&row)?;
        ensure(!item.reserved(), Error::Forbidden)?;
        self.put_item(id.into(), Some(record.revision), true, &item)
    }

    /// Restore a deleted item as a new version on top of its tombstone.
    ///
    /// # Errors
    /// Returns `Error::NotFound` unless the item is in the trash.
    pub fn vault_restore(&mut self, id: &str) -> Result<VaultEntry> {
        let (row, record) = self.current(id)?;
        ensure(record.tombstone, Error::NotFound)?;
        let (_, item) = self.open_row(&row)?;
        self.put_item(id.into(), Some(record.revision), false, &item)
    }

    /// The current version of every item, or of every deleted item.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn vault_list(&self, filter: &Filter) -> Result<Vec<VaultEntry>> {
        self.roots()?;
        let mut entries = Vec::new();
        for row in self.workspace.store.head_objects(VAULT)? {
            if let Some(entry) = self.vault_list_entry(&row, filter)? {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    fn vault_list_entry(&self, row: &ObjectRow, filter: &Filter) -> Result<Option<VaultEntry>> {
        let record = record(row)?;
        ensure(record.kind == VAULT, Error::NotFound)?;
        if record.tombstone != filter.trash {
            return Ok(None);
        }
        let item = open_item(&record.open(self.environment(), self.roots()?)?)?;
        Ok(filter
            .query
            .as_deref()
            .is_none_or(|q| item.matches(q))
            .then(|| VaultEntry::new(&record, item, row.state == SYNCED, row.conflict)))
    }

    /// The current version of one item, deleted or not.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown item.
    pub fn vault_show(&self, id: &str) -> Result<VaultEntry> {
        let (row, _) = self.current(id)?;
        self.entry(&row)
    }

    /// One version of an item, current or not.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown version.
    pub fn vault_version(&self, id: &str, revision: &str) -> Result<VaultEntry> {
        let row = self.version_row(id, revision)?;
        ensure(row.state != REPLACED, Error::NotFound)?;
        self.entry(&row)
    }

    /// Every version of an item, newest first: edits, deletions, restores
    /// and conflict copies. A version resealed under a newer root shows
    /// once, as its replacement.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` for an
    /// unknown item.
    pub fn vault_history(&self, id: &str) -> Result<Vec<Version>> {
        self.roots()?;
        let (head, _) = self.current(id)?;
        let mut versions = Vec::new();
        for row in self.workspace.store.versions(id)? {
            if row.kind != VAULT || row.state == REPLACED {
                continue;
            }
            let record = record(&row)?;
            versions.push(Version {
                current: row.revision == head.revision,
                revision: row.revision,
                parent: record.parent,
                at: record.created_at,
                device: record.device_id,
                deleted: record.tombstone,
                conflict: row.conflict,
                synced: row.state == SYNCED,
            });
        }
        versions.reverse();
        Ok(versions)
    }

    /// Make an earlier version's content current again, as a new version.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown version, an item in the
    /// trash or a deletion, and `Error::Forbidden` for dMsg's own keys.
    pub fn vault_revert(&mut self, id: &str, revision: &str) -> Result<VaultEntry> {
        let (head_row, head) = self.current(id)?;
        ensure(!head.tombstone, Error::NotFound)?;
        let (_, current) = self.open_row(&head_row)?;
        ensure(!current.reserved(), Error::Forbidden)?;
        let earlier = self.vault_version(id, revision)?;
        ensure(!earlier.deleted, Error::NotFound)?;
        let mut item = earlier.item;
        ensure(
            !item.reserved() && item.kind == current.kind,
            Error::Forbidden,
        )?;
        item.resolved_conflicts = current.resolved_conflicts.clone();
        if item.same(&current) {
            return self.vault_show(id);
        }
        item.updated_at = crate::now_ms();
        self.put_item(id.into(), Some(head.revision), false, &item)
    }

    /// Conflict copies waiting for a decision, each beside the item's
    /// current version: of a chain of copies, the newest stands for the
    /// rest.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn vault_conflicts(&self) -> Result<Vec<Conflict>> {
        self.roots()?;
        let rows = self.workspace.store.conflicts(VAULT)?;
        let parents: BTreeSet<String> = rows
            .iter()
            .filter_map(|row| {
                record(row)
                    .ok()?
                    .parent
                    .map(|p| format!("{}:{p}", row.item))
            })
            .collect();
        let mut conflicts = Vec::new();
        for row in rows.iter().filter(|row| !parents.contains(&row.key)) {
            let copy = self.entry(row)?;
            let head = self.vault_show(&copy.id)?;
            let mut hidden = BTreeSet::new();
            let value = |item: &Item, id: &str| item.field(id).map(|f| f.value.clone());
            for field in head.item.fields.iter().chain(&copy.item.fields) {
                if field.hidden() && value(&head.item, &field.id) != value(&copy.item, &field.id) {
                    hidden.insert(field.id.clone());
                }
            }
            conflicts.push(Conflict {
                copy,
                head,
                hidden: hidden.into_iter().collect(),
            });
        }
        Ok(conflicts)
    }

    /// Settle a conflict copy, `id:revision`: adopt it, keep the current
    /// version, or merge them. A new head records the copy, and the
    /// earlier copies it replaced, as resolved; a deleted head that is
    /// kept stays as it is.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a key that is not a conflict copy and
    /// `Error::InvalidInput` for a merge with a deleted side or of dMsg's
    /// own keys.
    pub fn vault_resolve(&mut self, key: &str, keep: &Keep) -> Result<VaultEntry> {
        let row = self
            .workspace
            .store
            .object(key)?
            .filter(|r| r.conflict && !r.resolved)
            .ok_or(Error::NotFound)?;
        let (copy, theirs) = self.open_row(&row)?;
        let (head_row, head) = self.current(&copy.id)?;
        let (_, current) = self.open_row(&head_row)?;
        let mut resolved = current.resolved();
        resolved.extend(theirs.resolved());
        let mut chain = vec![row.key.clone()];
        let mut parent = copy.parent.clone();
        while let Some(revision) = parent {
            let key = format!("{}:{revision}", copy.id);
            match self.workspace.store.object(&key)? {
                Some(earlier) if earlier.conflict => {
                    parent = record(&earlier)?.parent;
                    chain.push(key);
                }
                _ => break,
            }
        }
        resolved.extend(chain.iter().cloned());
        let (tombstone, mut item) = match keep {
            Keep::Current => (head.tombstone, current.clone()),
            Keep::Theirs => (copy.tombstone, theirs),
            Keep::Merge { picks, notes } => {
                ensure_valid(
                    !head.tombstone && !copy.tombstone && !current.reserved(),
                    "merge",
                )?;
                let mut item = merge(&current, &theirs, picks, notes.as_deref());
                item.normalize();
                ensure(!item.reserved(), Error::Forbidden)?;
                item.validate()?;
                item.updated_at = crate::now_ms();
                (false, item)
            }
        };
        let entry = if tombstone && head.tombstone {
            // A tombstone cannot follow a tombstone; the copy is only
            // dismissed on this device.
            VaultEntry::new(&head, current, head_row.state == SYNCED, false)
        } else {
            item.resolved_conflicts = Some(resolved.into_iter().collect());
            self.put_item(copy.id.clone(), Some(head.revision), tombstone, &item)?
        };
        for key in &chain {
            self.workspace.store.set_resolved(key)?;
        }
        Ok(entry)
    }
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
