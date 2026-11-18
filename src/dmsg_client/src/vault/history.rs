//! Private, display-only descriptions of the changes in a page of vault history.
use super::{Item, Version};
use dmsg_types::{Error, Result, ensure_valid};
use serde::Serialize;
use std::collections::BTreeMap;

const ALIASES: &str = "vault:history:device-aliases";

/// Metadata and non-secret change categories for one immutable revision.
#[derive(Serialize)]
pub struct HistoryRow {
    /// Stored revision identity and state.
    #[serde(flatten)]
    pub version: Version,
    /// Stable category codes, never field values or user-authored labels.
    pub changes: Vec<&'static str>,
    /// Optional nickname stored on this installation.
    pub device_alias: Option<String>,
}

/// A bounded slice of the history for one observed head.
#[derive(Serialize)]
pub struct HistoryPage {
    /// Head revision when read.
    pub head: String,
    /// Rows newest first.
    pub versions: Vec<HistoryRow>,
    /// Further revisions follow the last row.
    pub more: bool,
}

fn changed_categories(previous: &Item, next: &Item) -> Vec<&'static str> {
    let mut changes = Vec::new();
    for (changed, name) in [
        (previous.title != next.title, "title"),
        (previous.kind != next.kind, "type"),
        (previous.notes != next.notes, "notes"),
        (previous.tags != next.tags, "tags"),
        (previous.favorite != next.favorite, "favorite"),
        (previous.files != next.files, "attachments"),
        (
            previous.resolved_conflicts != next.resolved_conflicts,
            "conflict",
        ),
    ] {
        if changed {
            changes.push(name);
        }
    }
    if previous.fields != next.fields {
        if previous.fields.iter().find(|f| f.id == "password")
            != next.fields.iter().find(|f| f.id == "password")
        {
            changes.push("password");
        }
        if previous
            .fields
            .iter()
            .filter(|f| f.id != "password")
            .ne(next.fields.iter().filter(|f| f.id != "password"))
            || previous
                .fields
                .iter()
                .map(|f| &f.id)
                .ne(next.fields.iter().map(|f| &f.id))
        {
            changes.push("fields");
        }
    }
    changes
}

impl crate::client::Client {
    /// List a bounded page of revisions. Compare each with its actual parent,
    /// including on a conflict branch. Only category names leave the core.
    pub fn vault_history_page(
        &self,
        id: &str,
        before: Option<&str>,
        limit: usize,
    ) -> Result<HistoryPage> {
        ensure_valid((1..=100).contains(&limit), "page")?;
        let versions = self.vault_history(id)?;
        let head = versions
            .iter()
            .find(|v| v.current)
            .ok_or(Error::NotFound)?
            .revision
            .clone();
        let start = match before {
            None => 0,
            Some(anchor) => {
                versions
                    .iter()
                    .position(|v| v.revision == anchor)
                    .ok_or(Error::VersionConflict)?
                    + 1
            }
        };
        let more = versions.len() - start > limit;
        let aliases = self.history_device_aliases()?;
        let versions = versions
            .into_iter()
            .skip(start)
            .take(limit)
            .map(|version| {
                let changes = self
                    .revision_categories(id, &version)
                    .unwrap_or_else(|_| vec!["unavailable"]);
                HistoryRow {
                    device_alias: aliases.get(&version.device).cloned(),
                    version,
                    changes,
                }
            })
            .collect();
        Ok(HistoryPage {
            head,
            versions,
            more,
        })
    }

    fn revision_categories(&self, id: &str, version: &Version) -> Result<Vec<&'static str>> {
        if version.deleted {
            return Ok(vec!["deleted"]);
        }
        let Some(parent) = &version.parent else {
            return Ok(vec!["created"]);
        };
        let previous = self.vault_version(id, parent)?;
        if previous.deleted {
            return Ok(vec!["restored"]);
        }
        let current = self.vault_version(id, &version.revision)?;
        Ok(changed_categories(&previous.item, &current.item))
    }

    fn history_device_aliases(&self) -> Result<BTreeMap<String, String>> {
        Ok(self
            .workspace
            .private(self.keys()?, ALIASES)?
            .unwrap_or_default())
    }

    /// A nickname belongs to this unlocked account on this installation.
    /// Changing it never edits device permissions or writes a synced vault item.
    pub fn set_history_device_alias(&self, device: &str, name: &str) -> Result<()> {
        crate::upload::hex_id(device)?;
        let name = name.trim();
        ensure_valid(
            name.chars().count() <= 80 && !name.chars().any(char::is_control),
            "device name",
        )?;
        let mut aliases = self.history_device_aliases()?;
        if name.is_empty() {
            aliases.remove(device);
        } else {
            aliases.insert(device.to_owned(), name.to_owned());
        }
        self.workspace.put_private(self.keys()?, ALIASES, &aliases)
    }
}
