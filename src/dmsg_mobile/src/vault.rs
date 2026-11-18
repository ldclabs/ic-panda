use crate::{MobileEngine, ObjectSnapshot, VaultDetail, VaultEdit, VaultItem, error::Result};
use dmsg_client::vault::{Changes, Filter, Keep, Side, reading};
use dmsg_types::{Error, ensure, ensure_valid};
use std::collections::BTreeMap;

#[derive(Clone, uniffi::Record)]
pub struct VaultDraftInput {
    pub id: String,
    pub item_id: Option<String>,
    pub base_revision: Option<String>,
    pub revision: u64,
    pub form: String,
    pub active: bool,
}

impl From<VaultDraftInput> for dmsg_client::vault::drafts::Draft {
    fn from(value: VaultDraftInput) -> Self {
        Self {
            id: value.id,
            item_id: value.item_id,
            base_revision: value.base_revision,
            revision: value.revision,
            form: value.form,
            active: value.active,
        }
    }
}

impl VaultEdit {
    pub(crate) fn changes(self) -> (String, Changes) {
        (
            self.kind,
            Changes {
                title: Some(self.title),
                notes: Some(self.notes),
                favorite: Some(self.favorite),
                tags: Some(self.tags),
                files: self.files,
                fields: Some(
                    self.fields
                        .into_iter()
                        .map(|f| dmsg_client::vault::FieldChange {
                            id: Some(f.id),
                            kind: Some(f.kind),
                            label: Some(f.label),
                            section: Some(f.section),
                            value: f.value,
                        })
                        .collect(),
                ),
                ..Default::default()
            },
        )
    }
}

#[derive(uniffi::Record)]
pub struct VaultBrowseRequest {
    pub query: String,
    pub kind: Option<String>,
    pub favorites: bool,
    pub tag: Option<String>,
    pub sort: String,
    pub offset: u32,
    pub limit: u32,
    pub trash: bool,
}

#[uniffi::export]
impl MobileEngine {
    pub async fn vault_history_page(
        &self,
        id: String,
        before: Option<String>,
        limit: u32,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move { c.vault_history_page(&id, before.as_deref(), limit as usize) })
        })
        .await
    }

    pub async fn set_history_device_alias(&self, device: String, name: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.set_history_device_alias(&device, &name) }))
            .await
            .map_err(Into::into)
    }

    pub async fn vault_drafts(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.vault_drafts() })).await
    }

    pub async fn checkpoint_vault_draft(&self, draft: VaultDraftInput) -> Result<bool> {
        self.host
            .run(move |c| Box::pin(async move { c.checkpoint_vault_draft(&draft.into()) }))
            .await
            .map_err(Into::into)
    }

    pub async fn discard_vault_draft(&self, id: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.discard_vault_draft(&id) }))
            .await
            .map_err(Into::into)
    }

    pub async fn commit_vault_draft(
        &self,
        draft: VaultDraftInput,
        edit: VaultEdit,
    ) -> Result<VaultDetail> {
        let generation = self.host.generation();
        let item = self
            .host
            .run(move |c| {
                Box::pin(async move {
                    let (kind, changes) = edit.changes();
                    c.commit_vault_draft(&draft.into(), &kind, &changes)
                        .map(VaultItem::from)
                })
            })
            .await?;
        Ok(VaultDetail { generation, item })
    }

    pub async fn browse_vault(&self, request: VaultBrowseRequest) -> Result<ObjectSnapshot> {
        let VaultBrowseRequest {
            query,
            kind,
            favorites,
            tag,
            sort,
            offset,
            limit,
            trash,
        } = request;
        ensure_valid(
            query.len() <= 1000 && tag.as_ref().is_none_or(|t| t.len() <= 400),
            "query",
        )?;
        self.object(move |c| {
            Box::pin(async move {
                reading::browse(
                    &c.cooperative_hold(),
                    &reading::BrowseQuery {
                        filter: Filter {
                            trash,
                            query: Some(query),
                        },
                        kind,
                        favorites,
                        tag,
                        sort,
                        offset: offset as usize,
                        limit: limit as usize,
                    },
                )
                .await
            })
        })
        .await
    }

    pub async fn favorite_vault_item(
        &self,
        id: String,
        revision: String,
        favorite: bool,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                c.vault_edit(
                    &id,
                    Some(&revision),
                    &Changes {
                        favorite: Some(favorite),
                        ..Default::default()
                    },
                )
                .map(|e| e.masked(false))
            })
        })
        .await
    }

    /// Picks name the parts taken from the conflicting version. Unselected
    /// parts retain the current version; both immutable histories remain.
    pub async fn merge_vault_conflict(
        &self,
        key: String,
        expected_revision: String,
        theirs: Vec<String>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(theirs.len() <= 69, "picks")?;
        let picks: BTreeMap<_, _> = theirs.into_iter().map(|key| (key, Side::Theirs)).collect();
        self.object(move |c| {
            Box::pin(async move {
                let (id, _) = key
                    .split_once(':')
                    .ok_or_else(|| dmsg_types::invalid("conflict"))?;
                ensure(
                    c.vault_show(id)?.revision == expected_revision,
                    Error::VersionConflict,
                )?;
                c.vault_resolve(&key, &Keep::Merge { picks, notes: None })
                    .map(|e| e.masked(false))
            })
        })
        .await
    }
}

/// Validate on-device without reading or saving a workspace secret.
#[uniffi::export]
pub fn validate_vault_otp(value: String) -> bool {
    dmsg_client::vault::otp::parse(&value).is_ok()
}
