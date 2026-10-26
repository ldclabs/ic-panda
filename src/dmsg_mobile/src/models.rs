use dmsg_client::{Status, vault::VaultEntry};

#[derive(Clone, uniffi::Record)]
pub struct SessionSnapshot {
    pub generation: u64,
    pub sequence: u64,
    pub state: String,
    pub account: Option<String>,
    pub device_id: Option<String>,
    pub pending_sync: u64,
    pub conflicts: u64,
    pub approvals_waiting: u64,
    pub fast_unlock: bool,
    pub setup_pending: bool,
    pub last_sync_at_ms: Option<u64>,
    pub sync_error: Option<String>,
}

impl SessionSnapshot {
    pub fn from_status(value: Status, generation: u64, sequence: u64) -> Self {
        Self {
            generation,
            sequence,
            state: value.state.into(),
            account: value.account,
            device_id: value.device_id,
            pending_sync: value.pending_sync,
            conflicts: value.conflicts,
            approvals_waiting: value.approvals_waiting,
            fast_unlock: value.fast_unlock,
            setup_pending: value.setup_pending,
            last_sync_at_ms: value.last_sync.as_ref().map(|s| s.at),
            sync_error: value.last_sync.and_then(|s| s.error),
        }
    }
}

#[derive(uniffi::Record)]
pub struct LoginStart {
    pub url: String,
    pub deadline_ms: u64,
}

#[derive(Clone, uniffi::Record)]
pub struct VaultField {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub value: String,
    pub concealed: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct VaultItem {
    pub id: String,
    pub revision: String,
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub notes: String,
    pub favorite: bool,
    pub synced: bool,
    pub conflict: bool,
    pub reserved: bool,
    pub files: Vec<String>,
    pub fields: Vec<VaultField>,
}

impl VaultItem {
    pub fn summary(entry: VaultEntry) -> Self {
        let mut item = Self::from(entry);
        item.subtitle = item.subtitle.chars().take(160).collect();
        item.fields.clear();
        item.notes.clear();
        item
    }
}

impl From<VaultEntry> for VaultItem {
    fn from(entry: VaultEntry) -> Self {
        let reserved = entry.item.reserved();
        let entry = entry.masked(false);
        let subtitle = entry
            .item
            .fields
            .iter()
            .find(|f| !f.hidden() && !f.value.is_empty())
            .map(|f| f.value.clone())
            .unwrap_or_default();
        Self {
            id: entry.id,
            revision: entry.revision,
            kind: entry.item.kind,
            title: entry.item.title,
            subtitle,
            notes: entry.item.notes,
            favorite: entry.item.favorite,
            synced: entry.synced,
            conflict: entry.conflict,
            reserved,
            files: entry.item.files,
            fields: entry
                .item
                .fields
                .into_iter()
                .map(|f| {
                    let concealed = f.hidden();
                    VaultField {
                        id: f.id,
                        kind: f.kind,
                        label: f.label,
                        value: f.value,
                        concealed,
                    }
                })
                .collect(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct VaultPage {
    pub generation: u64,
    pub items: Vec<VaultItem>,
    pub has_more: bool,
}

#[derive(uniffi::Record)]
pub struct VaultDetail {
    pub generation: u64,
    pub item: VaultItem,
}

#[derive(uniffi::Record)]
pub struct FieldEdit {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub value: Option<String>,
}

#[derive(uniffi::Record)]
pub struct VaultEdit {
    pub title: String,
    pub kind: String,
    pub notes: String,
    pub favorite: bool,
    pub fields: Vec<FieldEdit>,
    pub files: Option<Vec<String>>,
}

/// Core-produced display JSON for evolving, nested protocol objects (approval
/// payloads, file versions and security state). No arbitrary RPC dispatcher.
#[derive(uniffi::Record)]
pub struct ObjectSnapshot {
    pub generation: u64,
    pub json: String,
}

#[derive(uniffi::Record)]
pub struct SecretValue {
    pub generation: u64,
    pub value: String,
    pub remaining_ms: Option<u64>,
}

#[derive(uniffi::Record)]
pub struct ChannelItem {
    pub id: String,
    pub name: String,
    pub unread: u64,
    pub draft: String,
    pub preview: String,
}

#[derive(uniffi::Record)]
pub struct ChannelPage {
    pub generation: u64,
    pub items: Vec<ChannelItem>,
}

#[derive(uniffi::Record)]
pub struct MessageItem {
    pub id: String,
    pub sequence: u64,
    pub account: String,
    pub text: String,
    pub created_at_ms: u64,
    pub attachment_name: Option<String>,
    pub attachment_mime: Option<String>,
    pub attachment_size: Option<u64>,
    pub unreadable: bool,
}

#[derive(uniffi::Record)]
pub struct MessagePage {
    pub generation: u64,
    pub items: Vec<MessageItem>,
    pub has_more: bool,
}

#[derive(uniffi::Record)]
pub struct OutgoingMessage {
    pub id: String,
    pub text: String,
    pub attachment_name: Option<String>,
    pub attempted: bool,
    pub error: Option<String>,
}

#[derive(uniffi::Record)]
pub struct OutboxPage {
    pub generation: u64,
    pub items: Vec<OutgoingMessage>,
}
