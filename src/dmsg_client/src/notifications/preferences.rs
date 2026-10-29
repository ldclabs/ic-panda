//! Versioned channel preferences. The existing encrypted keyring keeps its old
//! wire format; the optional preference object carries timing and server CAS.
use crate::{
    Client,
    codec::{MAX_RECORD, canonical, decode, digest},
    hold::{Hold, Unlocked},
    now_ms,
    store::Store,
};
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;

const KIND: &str = "notification_preferences";

/// Minimal local draft, readable while locked. `expected` never decreases.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ResourcePreference {
    pub revision: u64,
    pub acknowledged: u64,
    pub expected: u64,
    pub blocked: Option<u64>,
    pub muted: bool,
    #[serde(default)]
    pub hidden: bool,
    pub until: Option<u64>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct EncryptedPreference {
    channel: String,
    revision: u64,
    muted: bool,
    hidden: bool,
    until: Option<u64>,
}

fn key(channel: &str) -> String {
    format!("notification:resource:{channel}")
}
fn object_id(account: &str, channel: &str) -> String {
    hex::encode(*digest(
        "dmsg/notification/preference/v1",
        &(account, channel),
    ))
}

pub(crate) fn stored(store: &Store, channel: &str) -> Result<Option<ResourcePreference>> {
    store.meta(&key(channel))
}

/// A confirmed higher server revision supersedes older encrypted/local copies.
/// An explicit outstanding local draft remains reviewable after a conflict.
pub(crate) fn adopt(
    store: &Store,
    channel: &str,
    revision: u64,
    muted: bool,
    hidden: bool,
    until: Option<u64>,
) -> Result<()> {
    let mut value = stored(store, channel)?.unwrap_or_default();
    if revision <= value.expected {
        return Ok(());
    }
    if value.revision > value.acknowledged
        && (value.muted != muted || value.hidden != hidden || value.until != until)
    {
        value.blocked = Some(value.revision);
    } else {
        value.muted = muted;
        value.hidden = hidden;
        value.until = until;
        value.acknowledged = value.revision;
        value.blocked = None;
    }
    value.expected = revision;
    store.set_meta(&key(channel), &value)
}

impl Client {
    pub(crate) fn channel_notification_preference(
        &self,
        channel: &str,
        fallback: bool,
    ) -> Result<ResourcePreference> {
        let account = self.account()?.id.to_string();
        let id = object_id(&account, channel);
        match self.current(&id) {
            Ok((_, record)) if !record.tombstone => {
                ensure(record.kind == KIND, Error::IntegrityFailed)?;
                let remote: EncryptedPreference =
                    decode(&record.open(self.environment(), self.roots()?)?, MAX_RECORD)?;
                ensure(remote.channel == channel, Error::IntegrityFailed)?;
                adopt(
                    &self.workspace.store,
                    channel,
                    remote.revision,
                    remote.muted,
                    remote.hidden,
                    remote.until,
                )?;
            }
            Ok(_) | Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
        Ok(
            stored(&self.workspace.store, channel)?.unwrap_or(ResourcePreference {
                muted: fallback,
                ..Default::default()
            }),
        )
    }

    pub(crate) fn edit_notification_resource(
        &self,
        channel: &str,
        muted: bool,
        until: Option<u64>,
    ) -> Result<()> {
        ensure(
            crate::channel_log::is_hash(channel),
            Error::InvalidInput("channel".into()),
        )?;
        let mut preference = self.channel_notification_preference(channel, muted)?;
        preference.revision = preference
            .revision
            .checked_add(1)
            .ok_or(Error::VersionConflict)?;
        preference.muted = muted;
        preference.until = until;
        preference.blocked = None;
        self.workspace.store.set_meta(&key(channel), &preference)
    }

    pub(crate) fn edit_notification_hidden(&self, channel: &str, hidden: bool) -> Result<()> {
        let mut preference = self.channel_notification_preference(channel, false)?;
        preference.revision = preference
            .revision
            .checked_add(1)
            .ok_or(Error::VersionConflict)?;
        preference.hidden = hidden;
        preference.blocked = None;
        self.workspace.store.set_meta(&key(channel), &preference)
    }

    /// Mute for an explicit time, or indefinitely with None. Expiry covers old
    /// messages; it does not mark them read or replay their notifications.
    pub fn channel_notification_mute(
        &mut self,
        channel: &str,
        until: Option<u64>,
    ) -> Result<crate::channel::ChannelView> {
        let now = now_ms();
        ensure(
            until.is_none_or(|at| at > now),
            Error::InvalidInput("mute deadline".into()),
        )?;
        self.channel_settings(channel, None, Some(true), None)?;
        self.edit_notification_resource(channel, true, until)?;
        self.reconcile_channel_notifications(now)?;
        self.channel_view(channel)
    }

    fn save_notification_resource(
        &mut self,
        channel: &str,
        value: &ResourcePreference,
    ) -> Result<()> {
        let id = object_id(&self.account()?.id.to_string(), channel);
        let parent = self.workspace.store.head(&id)?;
        let record = EncryptedPreference {
            channel: channel.into(),
            revision: value.expected,
            muted: value.muted,
            hidden: value.hidden,
            until: value.until,
        };
        match self.current(&id) {
            Ok((_, current)) if !current.tombstone => {
                ensure(current.kind == KIND, Error::IntegrityFailed)?;
                let old: EncryptedPreference = decode(
                    &current.open(self.environment(), self.roots()?)?,
                    MAX_RECORD,
                )?;
                if old == record || old.revision > record.revision {
                    return Ok(());
                }
            }
            Ok(_) | Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
        self.write(KIND, id, parent, false, &canonical(&record), None)?;
        Ok(())
    }
}

/// Upload small mute drafts separately from account/device preferences.
pub async fn sync(h: &impl Hold, context: &super::remote::ReceiveContext) -> Result<()> {
    let h = Unlocked::new(h).await?;
    let channels = h.with(|c| c.channel_list()).await?;
    for channel in channels {
        let pending = h
            .with(|c| {
                ensure(
                    c.account()?.id.to_string() == context.account,
                    Error::Locked,
                )?;
                let mut value = c.channel_notification_preference(&channel.id, channel.muted)?;
                // Import a legacy mute once. A missing old false value is not an edit.
                if stored(&c.workspace.store, &channel.id)?.is_none()
                    && (value.muted || channel.hidden)
                {
                    value.hidden = channel.hidden;
                    value.revision = 1;
                    c.workspace.store.set_meta(&key(&channel.id), &value)?;
                }
                if value.expected > 0 && value.revision <= value.acknowledged {
                    c.save_notification_resource(&channel.id, &value)?;
                }
                Ok(
                    (value.revision > value.acknowledged && value.blocked != Some(value.revision))
                        .then_some(value),
                )
            })
            .await?;
        let Some(mut preference) = pending else {
            continue;
        };
        let payload = json!({"scope":"resource","installation":context.installation,"expected_revision":preference.expected,
            "value":{"resource":channel.id,"muted":preference.muted,"hidden":preference.hidden,"until":preference.until}});
        let result = h
            .send(|c| {
                let state = c.fresh_state().ok_or(Error::PolicyStale)?;
                c.relay().prepare_raw(
                    &c.signer(state)?,
                    "/v1/notifications/settings",
                    &payload,
                    now_ms(),
                )
            })
            .await;
        let (revision, acknowledged) = match result {
            Ok(value) => (
                value["revision"].as_u64().ok_or(Error::IntegrityFailed)?,
                true,
            ),
            Err(Error::VersionConflict | Error::ExecutionUnknown) => {
                let remote = h
                    .get(&format!(
                        "/v1/notifications/settings?installation={}&resource={}",
                        context.installation, channel.id
                    ))
                    .await?;
                (
                    remote["revision"].as_u64().ok_or(Error::IntegrityFailed)?,
                    remote["value"] == payload["value"],
                )
            }
            Err(error) => return Err(error),
        };
        h.with(|c| {
            let mut current = stored(&c.workspace.store, &channel.id)?.ok_or(Error::NotFound)?;
            if current.revision != preference.revision {
                current.expected = current.expected.max(revision);
                if acknowledged {
                    current.acknowledged = current.acknowledged.max(preference.revision);
                } else {
                    current.blocked = Some(current.revision);
                }
                return c.workspace.store.set_meta(&key(&channel.id), &current);
            }
            preference.expected = revision;
            if acknowledged {
                preference.acknowledged = preference.revision;
                preference.blocked = None;
                c.save_notification_resource(&channel.id, &preference)?;
            } else {
                preference.blocked = Some(preference.revision);
            }
            c.workspace.store.set_meta(&key(&channel.id), &preference)?;
            c.reconcile_channel_notifications(now_ms())
        })
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notifications_encrypted_preference_restores_a_confirmed_revision_without_extending_the_channel_wire()
     {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let channel = "bb".repeat(32);
        c.save_notification_resource(
            &channel,
            &ResourcePreference {
                expected: 5,
                muted: true,
                until: Some(1234),
                ..Default::default()
            },
        )
        .unwrap();
        let restored = c.channel_notification_preference(&channel, false).unwrap();
        assert!(restored.muted);
        assert_eq!(restored.expected, 5);
        assert_eq!(restored.until, Some(1234));
        c.save_notification_resource(
            &channel,
            &ResourcePreference {
                expected: 3,
                muted: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            c.channel_notification_preference(&channel, false)
                .unwrap()
                .muted
        );
    }
    #[test]
    fn notifications_resource_revisions_do_not_overwrite_a_newer_mute_or_a_pending_draft() {
        let dir = crate::testing::TempDir::new();
        let store = Store::open(&dir.path("workspace.sqlite")).unwrap();
        let channel = "aa".repeat(32);
        adopt(&store, &channel, 4, true, false, Some(1000)).unwrap();
        adopt(&store, &channel, 2, false, false, None).unwrap();
        let mut value = stored(&store, &channel).unwrap().unwrap();
        assert!(value.muted);
        assert_eq!(value.expected, 4);
        value.revision = 1;
        value.muted = false;
        store.set_meta(&key(&channel), &value).unwrap();
        adopt(&store, &channel, 5, true, false, None).unwrap();
        let value = stored(&store, &channel).unwrap().unwrap();
        assert!(!value.muted);
        assert_eq!(value.blocked, Some(1));
        assert_eq!(value.expected, 5);
    }
}
