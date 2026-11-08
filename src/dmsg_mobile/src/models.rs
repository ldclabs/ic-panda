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
    /// II authorization expires independently of the local unlocked session.
    pub login_expires_at_ms: Option<u64>,
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
            login_expires_at_ms: value.login_expires_at,
            setup_pending: value.setup_pending,
            last_sync_at_ms: value.last_sync.as_ref().and_then(|s| s.succeeded_at()),
            sync_error: value.last_sync.and_then(|s| s.error.or(s.upload_error)),
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
    pub section: String,
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
    pub tags: Vec<String>,
    pub synced: bool,
    pub conflict: bool,
    pub reserved: bool,
    pub files: Vec<String>,
    pub fields: Vec<VaultField>,
    pub deleted: bool,
    pub created_at: u64,
    pub updated_at: u64,
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
        let subtitle = dmsg_client::vault::reading::subtitle(&entry.item);
        Self {
            id: entry.id,
            revision: entry.revision,
            kind: entry.item.kind,
            title: entry.item.title,
            subtitle,
            notes: entry.item.notes,
            favorite: entry.item.favorite,
            tags: entry.item.tags,
            synced: entry.synced,
            conflict: entry.conflict,
            reserved,
            deleted: entry.deleted,
            created_at: entry.item.created_at,
            updated_at: entry.item.updated_at,
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
                        section: f.section,
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

#[cfg(test)]
mod vault_display_tests {
    use super::*;
    use dmsg_client::{
        testing::{TempDir, unlocked},
        vault::{Changes, FieldChange},
    };

    #[test]
    fn native_details_preserve_tombstones_and_timestamps_without_revealing_values() {
        let directory = TempDir::new();
        let mut client = unlocked(&directory.0, false, None);
        let entry = client
            .vault_create(
                "login",
                &Changes {
                    title: Some("Example".into()),
                    fields: Some(vec![FieldChange {
                        id: Some("password".into()),
                        value: Some("private value".into()),
                        ..Default::default()
                    }]),
                    ..Default::default()
                },
            )
            .unwrap();
        let created = entry.item.created_at;
        let detail = VaultItem::from(entry.clone());
        assert!(!detail.deleted);
        assert_eq!(detail.created_at, created);
        assert!(detail.fields.iter().all(|f| f.value != "private value"));
        let deleted = VaultItem::from(client.vault_delete(&entry.id).unwrap());
        assert!(deleted.deleted);
        assert_eq!(deleted.created_at, created);
        let restored = VaultItem::from(client.vault_restore(&entry.id).unwrap());
        assert!(!restored.deleted);
        assert_ne!(restored.revision, deleted.revision);
    }
}

#[derive(uniffi::Record)]
pub struct FieldEdit {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub section: String,
    pub value: Option<String>,
}

#[derive(uniffi::Record)]
pub struct VaultEdit {
    pub title: String,
    pub kind: String,
    pub notes: String,
    pub favorite: bool,
    pub tags: Vec<String>,
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
    pub last_at: u64,
    pub muted: bool,
    pub status: String,
    pub can_publish: bool,
    pub can_invite: bool,
    pub pending_sends: u64,
    pub read_through: u64,
    /// The relay holds messages this device has yet to read.
    pub news: bool,
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
    pub digest: String,
    pub reply_sequence: Option<u64>,
    pub edited: bool,
    pub retracted: bool,
    pub attachment_deleted: bool,
    pub hidden: bool,
}

#[derive(uniffi::Record)]
pub struct MessagePage {
    pub generation: u64,
    pub items: Vec<MessageItem>,
    pub has_more: bool,
    pub next_before: Option<u64>,
}

#[derive(uniffi::Record)]
pub struct SentMessage {
    pub generation: u64,
    pub message: MessageItem,
}

#[derive(uniffi::Record)]
pub struct OutgoingMessage {
    pub id: String,
    pub text: String,
    pub attachment_name: Option<String>,
    pub attempted: bool,
    pub error: Option<String>,
}

impl From<dmsg_client::outbox::Intent> for OutgoingMessage {
    fn from(intent: dmsg_client::outbox::Intent) -> Self {
        Self {
            id: intent.id,
            text: intent.text,
            attachment_name: intent.attachment.map(|f| f.name),
            attempted: intent.attempted,
            error: intent.error,
        }
    }
}

#[derive(uniffi::Record)]
pub struct RecoverableMessage {
    pub channel: String,
    pub message: OutgoingMessage,
}

#[derive(uniffi::Record)]
pub struct OutboxPage {
    pub generation: u64,
    pub items: Vec<OutgoingMessage>,
}

impl From<dmsg_client::channel::Message> for MessageItem {
    fn from(m: dmsg_client::channel::Message) -> Self {
        Self {
            id: m.message_id,
            sequence: m.seq,
            account: m.account,
            text: m.text,
            created_at_ms: m.created_at,
            attachment_name: m.file.as_ref().map(|f| f.name.clone()),
            attachment_mime: m.file.as_ref().map(|f| f.mime.clone()),
            attachment_size: m.file.as_ref().map(|f| f.size),
            unreadable: m.unreadable,
            digest: m.digest,
            reply_sequence: m.reply_to.map(|r| r.seq),
            edited: m.edited_at.is_some(),
            retracted: m.retracted,
            attachment_deleted: m.file_deleted,
            hidden: m.hidden,
        }
    }
}

#[derive(uniffi::Record)]
pub struct MessageDraft {
    pub channel: String,
    pub text: String,
}

#[derive(Clone, uniffi::Record)]
pub struct StagingDeployment {
    pub relay: String,
    pub handle: String,
    pub user_homes: Vec<String>,
    pub wallet_hub: Option<String>,
    pub connect_url: String,
}
