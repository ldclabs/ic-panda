//! The authenticated device-local password picker reads only current login fields.

use super::VaultEntry;
use crate::{Client, content::VAULT};
use dmsg_types::Result;
use serde::{Deserialize, Serialize};

// Older devices also stored a list of selected IDs here. Preserve their
// authentication preference when upgrading to the searchable login catalog.
const KEY: &str = "autofill-selection";

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct LegacySelection {
    items: Vec<String>,
    biometry_only: bool,
    digest: String,
}

/// Fields available to the native password picker after device authentication.
#[derive(Serialize, Debug)]
pub struct Entry {
    /// Stable vault item ID.
    pub id: String,
    /// Login title.
    pub title: String,
    /// Account name.
    pub username: String,
    /// Password to fill.
    pub password: String,
    /// Login website, if supplied.
    pub url: String,
    /// Precomputed normalized and pinyin search keys derived strictly from non-secret
    /// display fields (title, username, url host) so system pickers support pinyin without
    /// bundling dictionaries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search_keys: Vec<String>,
}

impl Entry {
    fn from_login(entry: VaultEntry) -> Option<Self> {
        if entry.deleted || entry.item.reserved() || entry.item.kind != "login" {
            return None;
        }
        let field = |id| {
            entry
                .item
                .field(id)
                .map_or("", |f| f.value.as_str())
                .to_owned()
        };
        let password = field("password");
        if password.is_empty() {
            return None;
        }
        let username = field("username");
        let url = field("url");
        let host = url::Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        let search_keys = crate::search::search_keys(&[&entry.item.title, &username, &host]);
        Some(Self {
            id: entry.id,
            username,
            url,
            password,
            title: entry.item.title,
            search_keys,
        })
    }
}

/// A versioned view of this device’s available logins.
#[derive(Serialize)]
pub struct Catalog {
    /// Whether the native copy needs replacement.
    pub changed: bool,
    /// Version of the vault heads and authentication policy.
    pub digest: String,
    /// Effective authentication policy.
    pub biometry_only: bool,
    /// Current logins, populated only on change.
    pub entries: Vec<Entry>,
}

impl Client {
    /// Read the device preference, including a selection made before catalog support.
    pub fn autofill_biometry_only(&self) -> Result<bool> {
        let keys = self.keys()?;
        if let Some(only) = self.workspace.private(keys, "autofill-biometry")? {
            return Ok(only);
        }
        Ok(self
            .workspace
            .private::<LegacySelection>(keys, KEY)?
            .unwrap_or_default()
            .biometry_only)
    }

    /// Save the user's independently confirmed preference for this device.
    pub fn set_autofill_biometry(&self, biometry_only: bool) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, "autofill-biometry", &biometry_only)?;
        self.workspace.delete_private(KEY)
    }

    /// The published header holds the last successful digest. An unchanged
    /// catalog only reads revision IDs; failed publications are naturally retried.
    pub fn autofill_catalog(&self, known: &str, requires_biometry: bool) -> Result<Catalog> {
        self.roots()?;
        let biometry_only = requires_biometry || self.autofill_biometry_only()?;
        let heads = self.workspace.store.heads(VAULT)?;
        let digest = hex::encode(*dmsg_protocol::digest(
            "dmsg/autofill-catalog/1",
            &(&heads, biometry_only),
        ));
        let changed = digest != known;
        let mut entries = Vec::new();
        if changed {
            for (id, _) in heads {
                if let Some(login) = Entry::from_login(self.vault_show(&id)?) {
                    entries.push(login);
                }
            }
        }
        Ok(Catalog {
            changed,
            digest,
            biometry_only,
            entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempDir, unlocked};
    use crate::vault::{Changes, FieldChange};

    #[test]
    fn catalog_follows_all_logins_and_keeps_stable_ids_and_authentication_policy() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        let changes = |password: &str| Changes {
            title: Some("Same site".into()),
            set: Some(vec![FieldChange {
                id: Some("password".into()),
                value: Some(password.into()),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let mut ids = Vec::new();
        for _ in 0..51 {
            ids.push(client.vault_create("login", &changes("old")).unwrap().id);
        }
        client
            .vault_create(
                "note",
                &Changes {
                    title: Some("Note".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let legacy = LegacySelection {
            items: vec![ids[0].clone()],
            biometry_only: true,
            digest: "legacy".into(),
        };
        client
            .workspace
            .put_private(client.keys().unwrap(), KEY, &legacy)
            .unwrap();
        let first = client.autofill_catalog("", false).unwrap();
        assert!(first.biometry_only);
        assert_eq!(first.entries.len(), 51);
        assert!(
            !client
                .autofill_catalog(&first.digest, false)
                .unwrap()
                .changed
        );
        client.vault_edit(&ids[0], None, &changes("new")).unwrap();
        client.vault_delete(&ids[1]).unwrap();
        let next = client.autofill_catalog(&first.digest, false).unwrap();
        assert!(next.changed);
        assert_eq!(next.entries.len(), 50);
        assert_eq!(
            next.entries
                .iter()
                .find(|e| e.id == ids[0])
                .unwrap()
                .password,
            "new"
        );
        client.set_autofill_biometry(false).unwrap();
        assert!(
            client
                .autofill_catalog(&next.digest, false)
                .unwrap()
                .changed
        );
        assert!(!client.autofill_catalog(&next.digest, true).unwrap().changed);
    }
}
