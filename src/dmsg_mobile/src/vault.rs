use crate::{MobileEngine, ObjectSnapshot, error::Result};
use dmsg_client::vault::{Changes, Filter, Keep, Side, reading};
use dmsg_types::{Error, ensure, ensure_valid};
use std::collections::BTreeMap;

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
