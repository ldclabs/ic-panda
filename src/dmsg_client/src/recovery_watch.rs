//! Content-free recovery warnings from certified account security. These
//! observations remain readable while locked; they never authorize an action.

use crate::{Client, account::Security, client::Warm, hold::Hold, now_ms};
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const RECORD: &str = "recovery-observation";
/// Background checks supplement security reads made by ordinary operations.
pub const POLL_MS: u64 = 60_000;

#[derive(Serialize, Deserialize)]
struct Observation {
    account: String,
    digest: Option<String>,
    observed_at_ms: u64,
}

impl Client {
    /// Enrich only the matching certified incident with authenticated details.
    /// Subsequent public-only observations preserve this verified deadline.
    pub(crate) fn observe_recovery_deadline(
        &self,
        state: &crate::account::AccountState,
    ) -> Result<()> {
        use crate::notifications::{self, Target};
        let Some(pending) = &state.info.pending_recovery else {
            return Ok(());
        };
        let Some(account) = &self.workspace.meta.account else {
            return Ok(());
        };
        if account.id != state.info.account_id {
            return Ok(());
        }
        let hash = crate::codec::digest("dmsg/pending-recovery/v1", pending);
        if state.security.snapshot.pending_recovery_digest != Some(hash) {
            return Ok(());
        }
        let target = Target::Recovery(hex::encode(*hash));
        if let Some(mut item) = self
            .workspace
            .store
            .notification_item(&notifications::item_id(&account.id.to_string(), &target))?
            && item.open
            && item.version <= state.security.snapshot.account_version
        {
            item.deadline = Some(pending.execute_after);
            self.workspace.store.set_notification_item(&item)?;
        }
        Ok(())
    }

    /// Record only this workspace's verified security, never a peer's.
    pub(crate) fn observe_recovery(&self, security: &Security) -> Result<()> {
        let Some(account) = &self.workspace.meta.account else {
            return Ok(());
        };
        if account.id != security.snapshot.account_id || account.home != security.snapshot.home_user
        {
            return Ok(());
        }
        let previous: Option<Observation> = self.workspace.store.meta(RECORD)?;
        if previous.is_some_and(|p| p.observed_at_ms > security.certified_at) {
            return Ok(());
        }
        self.workspace.store.set_meta(
            RECORD,
            &Observation {
                account: account.id.to_string(),
                digest: security
                    .snapshot
                    .pending_recovery_digest
                    .map(|h| hex::encode(*h)),
                observed_at_ms: security.certified_at,
            },
        )?;
        use crate::notifications::{self, Item, Target};
        let digest = security
            .snapshot
            .pending_recovery_digest
            .map(|hash| hex::encode(*hash));
        // Only a fresh authenticated observation can resolve an old incident.
        for mut item in self.workspace.store.notification_items()? {
            if let Target::Recovery(old) = &item.target
                && item.open
                && Some(old) != digest.as_ref()
            {
                item.open = false;
                item.actionable = false;
                item.version = security.snapshot.account_version.max(item.version);
                item.updated_at = security.certified_at;
                notifications::upsert(&self.workspace.store, item)?;
            }
        }
        if let Some(digest) = digest {
            let mut item = Item::new(
                account.id.to_string(),
                Target::Recovery(digest),
                security.snapshot.account_version.max(1),
                security.certified_at,
            );
            item.actionable = true;
            notifications::upsert(&self.workspace.store, item)?;
        }
        Ok(())
    }

    /// Last verified recovery observation for the selected workspace. Null
    /// means unknown, not safe; an old pending warning survives an outage.
    /// No login, content key or background-online permission is needed.
    ///
    /// # Errors
    /// Returns storage errors when the observation cannot be read.
    pub fn recovery_warning(&self) -> Result<Value> {
        let Some(account) = &self.workspace.meta.account else {
            return Ok(Value::Null);
        };
        let Some(seen) = self.workspace.store.meta::<Observation>(RECORD)? else {
            return Ok(Value::Null);
        };
        if seen.account != account.id.to_string() {
            return Ok(Value::Null);
        }
        Ok(json!({
            "account": seen.account,
            "pending": seen.digest.is_some(),
            "digest": seen.digest,
            "observed_at_ms": seen.observed_at_ms,
            // Operational freshness, not the certificate's authorization TTL.
            "stale": now_ms() > seen.observed_at_ms.saturating_add(2 * POLL_MS),
        }))
    }
}

/// Refresh public certified security, including while locked. No private
/// recovery details, login or relay access is required. Network waits do not
/// hold the client; switching or resetting the workspace rejects the result.
///
/// # Errors
/// Returns certificate/network errors, or `Locked` if the session changed.
pub async fn refresh<H: Hold>(h: &H) -> Result<()> {
    let (generation, warmup) = h
        .with(|c| {
            Ok((
                c.session_generation(),
                c.warmup_for(Some(Warm::Home), false)?,
            ))
        })
        .await?;
    let Some(warmup) = warmup else {
        return Ok(());
    };
    let warmed = warmup.run().await?;
    h.with(|c| {
        ensure(c.session_generation() == generation, Error::Locked)?;
        c.warmed(warmed)?;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{client::Slot, testing};
    use dmsg_types::Hash;

    #[test]
    fn recovery_warning_survives_lock_restart_and_outage_but_not_account_switch() {
        let dir = testing::TempDir::new();
        let mut client = testing::unlocked(&dir.0, false, None);
        assert_eq!(client.recovery_warning().unwrap(), Value::Null);
        let mut security = testing::security(1, Default::default());
        security.certified_at = now_ms();
        security.snapshot.pending_recovery_digest = Some(Hash::new([7; 32]));
        client.observe(&testing::account().id, &security).unwrap();
        assert_eq!(client.recovery_warning().unwrap()["pending"], true);
        client.lock();
        assert!(!client.session_state().resident);
        let warning = client.recovery_warning().unwrap();
        drop(client);
        let mut client = Client::open(testing::config(false), &dir.0, None).unwrap();
        client.select(Slot::Account(testing::account().id)).unwrap();
        assert_eq!(client.recovery_warning().unwrap(), warning);
        client
            .select(Slot::Account("080g2081040g2081040g".parse().unwrap()))
            .unwrap();
        assert_eq!(client.recovery_warning().unwrap(), Value::Null);
        client.select(Slot::Account(testing::account().id)).unwrap();
        assert_eq!(client.recovery_warning().unwrap(), warning);

        // A certified cancellation removes the warning. A rollback cannot revive it.
        security.snapshot.account_version += 1;
        security.snapshot.pending_recovery_digest = None;
        client.observe(&testing::account().id, &security).unwrap();
        assert_eq!(client.recovery_warning().unwrap()["pending"], false);
        security.snapshot.account_version -= 1;
        security.snapshot.pending_recovery_digest = Some(Hash::new([8; 32]));
        assert!(client.observe(&testing::account().id, &security).is_err());
        assert_eq!(client.recovery_warning().unwrap()["pending"], false);
    }

    #[tokio::test]
    async fn failed_refresh_keeps_the_stale_pending_warning() {
        let dir = testing::TempDir::new();
        let mut client = testing::unlocked(&dir.0, false, None);
        let mut security = testing::security(1, Default::default());
        security.snapshot.pending_recovery_digest = Some(Hash::new([7; 32]));
        client.observe(&testing::account().id, &security).unwrap();
        client.lock();
        assert!(refresh(&client.hold()).await.is_err());
        let warning = client.recovery_warning().unwrap();
        assert_eq!(warning["pending"], true);
        assert_eq!(warning["stale"], true);
    }

    #[test]
    fn peers_and_older_certificates_cannot_replace_the_warning() {
        let dir = testing::TempDir::new();
        let client = testing::unlocked(&dir.0, false, None);
        let mut security = testing::security(1, Default::default());
        security.certified_at = now_ms();
        security.snapshot.pending_recovery_digest = Some(Hash::new([7; 32]));
        client.observe_recovery(&security).unwrap();
        let warning = client.recovery_warning().unwrap();
        security.snapshot.pending_recovery_digest = None;
        security.certified_at -= 1;
        client.observe_recovery(&security).unwrap();
        assert_eq!(client.recovery_warning().unwrap(), warning);
        security.certified_at += 2;
        security.snapshot.account_id = "080g2081040g2081040g".parse().unwrap();
        client.observe_recovery(&security).unwrap();
        assert_eq!(client.recovery_warning().unwrap(), warning);
    }
}
