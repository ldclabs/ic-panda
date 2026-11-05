//! The login items a person chose for the system's password AutoFill, kept
//! sealed on this device so the exported snapshot can follow them. Only the
//! choice itself goes through system authentication.

use super::VaultEntry;
use crate::client::Client;
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};

const KEY: &str = "autofill-selection";

/// What the person last confirmed.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Selection {
    /// Item IDs, at most what a snapshot holds.
    pub items: Vec<String>,
    /// Exported copies open only with biometrics.
    pub biometry_only: bool,
    /// Of the values last handed out.
    digest: String,
}

/// A login as the snapshot holds it.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The item's title.
    pub title: String,
    /// The `username` field.
    pub username: String,
    /// The `password` field, never empty.
    pub password: String,
    /// The `url` field.
    pub url: String,
}

impl Entry {
    /// Only a current login with a password may leave for AutoFill.
    ///
    /// # Errors
    /// `Forbidden` for anything else, `InvalidInput` for a missing password.
    pub fn of(entry: &VaultEntry) -> Result<Self> {
        ensure(
            !entry.deleted && !entry.item.reserved() && entry.item.kind == "login",
            Error::Forbidden,
        )?;
        let field = |id| entry.item.field(id).map_or("", |f| f.value.as_str());
        ensure_valid(!field("password").is_empty(), "password")?;
        Ok(Self {
            title: entry.item.title.clone(),
            username: field("username").into(),
            password: field("password").into(),
            url: field("url").into(),
        })
    }
}

/// The chosen logins' current values.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Refresh {
    /// They moved since they last left, or a refresh was forced.
    pub changed: bool,
    /// The logins that are still there; none when not `changed`.
    pub entries: Vec<Entry>,
    /// Exported copies open only with biometrics.
    pub biometry_only: bool,
}

fn digest(entries: &[Entry]) -> String {
    hex::encode(*dmsg_protocol::digest("dmsg/autofill-export/2", &entries))
}

impl Client {
    /// Remember what the person just confirmed and handed out.
    ///
    /// # Errors
    /// Refuses a locked workspace.
    pub fn autofill_choose(
        &self,
        items: Vec<String>,
        biometry_only: bool,
        entries: &[Entry],
    ) -> Result<()> {
        self.workspace.put_private(
            self.keys()?,
            KEY,
            &Selection {
                items,
                biometry_only,
                digest: digest(entries),
            },
        )
    }

    /// What the person last confirmed, if anything.
    ///
    /// # Errors
    /// Refuses a locked workspace or a corrupt record.
    pub fn autofill_chosen(&self) -> Result<Option<Selection>> {
        self.workspace.private(self.keys()?, KEY)
    }

    /// The person removed the snapshot.
    ///
    /// # Errors
    /// Refuses a locked workspace.
    pub fn autofill_forget(&self) -> Result<()> {
        self.keys()?;
        self.workspace.delete_private(KEY)
    }

    /// The chosen logins' current values when they moved since they last
    /// left, or always with `force`; nothing was chosen when `None`. A chosen
    /// item that is gone or no longer a login drops out.
    ///
    /// # Errors
    /// Refuses a locked workspace or a corrupt record.
    pub fn autofill_refresh(&self, force: bool) -> Result<Option<Refresh>> {
        let Some(mut selection) = self.autofill_chosen()? else {
            return Ok(None);
        };
        let entries: Vec<Entry> = selection
            .items
            .iter()
            .filter_map(|id| Entry::of(&self.vault_show(id).ok()?).ok())
            .collect();
        let current = digest(&entries);
        let changed = force || current != selection.digest;
        if changed {
            selection.digest = current;
            self.workspace.put_private(self.keys()?, KEY, &selection)?;
        }
        Ok(Some(Refresh {
            changed,
            entries: if changed { entries } else { vec![] },
            biometry_only: selection.biometry_only,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempDir, unlocked};
    use crate::vault::{Changes, FieldChange};

    fn login(title: &str, password: &str) -> Changes {
        Changes {
            title: Some(title.into()),
            set: Some(vec![FieldChange {
                id: Some("password".into()),
                value: Some(password.into()),
                ..FieldChange::default()
            }]),
            ..Changes::default()
        }
    }

    #[test]
    fn a_refresh_follows_only_what_was_chosen_and_only_when_it_moved() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        assert_eq!(client.autofill_refresh(true).unwrap(), None);

        let first = client.vault_create("login", &login("one", "a")).unwrap().id;
        let second = client.vault_create("login", &login("two", "b")).unwrap().id;
        let other = client.vault_create("login", &login("no", "c")).unwrap().id;
        let chosen = vec![first.clone(), second.clone()];
        let entries: Vec<Entry> = chosen
            .iter()
            .map(|id| Entry::of(&client.vault_show(id).unwrap()).unwrap())
            .collect();
        client.autofill_choose(chosen, true, &entries).unwrap();

        let quiet = client.autofill_refresh(false).unwrap().unwrap();
        assert!(!quiet.changed && quiet.entries.is_empty() && quiet.biometry_only);
        assert_eq!(
            client.autofill_refresh(true).unwrap().unwrap().entries,
            entries
        );

        client.vault_edit(&other, None, &login("no", "d")).unwrap();
        assert!(!client.autofill_refresh(false).unwrap().unwrap().changed);

        client.vault_edit(&first, None, &login("one", "e")).unwrap();
        let moved = client.autofill_refresh(false).unwrap().unwrap();
        assert!(moved.changed);
        assert_eq!(moved.entries[0].password, "e");
        assert!(!client.autofill_refresh(false).unwrap().unwrap().changed);

        client.vault_delete(&second).unwrap();
        let shrunk = client.autofill_refresh(false).unwrap().unwrap();
        assert!(shrunk.changed);
        assert_eq!(shrunk.entries.len(), 1);

        client.autofill_forget().unwrap();
        assert_eq!(client.autofill_refresh(true).unwrap(), None);
    }
}
