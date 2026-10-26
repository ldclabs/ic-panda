//! Device-local safety choices and immutable report drafts. These never give
//! the relay content unless the person explicitly selected that disclosure.
use crate::{
    Client,
    hold::{Hold, Unlocked},
};
use dmsg_types::{AccountId, Error, Hash, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const BLOCKS: &str = "safety-blocks";
const REPORT: &str = "safety-report:";
#[derive(Default, Serialize, Deserialize)]
struct Blocks {
    accounts: BTreeSet<String>,
    pending: BTreeMap<String, (bool, Hash)>,
}

impl Client {
    /// Locally effective blocks, including choices waiting for connectivity.
    pub fn blocked_accounts(&self) -> Result<Vec<String>> {
        let value: Blocks = self
            .workspace
            .private(self.keys()?, BLOCKS)?
            .unwrap_or_default();
        Ok(value.accounts.into_iter().collect())
    }

    /// Queue one signed remote block change while applying it locally now.
    pub fn set_blocked_account(&self, account: &str, blocked: bool) -> Result<()> {
        let account = account
            .parse::<AccountId>()
            .map_err(|_| dmsg_types::invalid("account"))?
            .to_string();
        let mut value: Blocks = self
            .workspace
            .private(self.keys()?, BLOCKS)?
            .unwrap_or_default();
        ensure(
            value.accounts.len() < 1000 || value.accounts.contains(&account) || !blocked,
            Error::QuotaExceeded,
        )?;
        if blocked {
            value.accounts.insert(account.clone());
        } else {
            value.accounts.remove(&account);
        }
        value
            .pending
            .insert(account, (blocked, Hash::new(crate::crypto::random_id())));
        self.workspace.put_private(self.keys()?, BLOCKS, &value)
    }

    /// Store the exact user-selected report before networking starts.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_content_report(
        &self,
        kind: &str,
        target: &str,
        resource: &str,
        sequence: Option<u64>,
        reason: &str,
        note: &str,
        disclose: bool,
    ) -> Result<Value> {
        self.keys()?;
        target
            .parse::<AccountId>()
            .map_err(|_| dmsg_types::invalid("account"))?;
        ensure_valid(
            ["spam", "harassment", "illegal", "other"].contains(&reason) && note.len() <= 2000,
            "report",
        )?;
        let mut payload = json!({"target_account":target,"kind":kind,"resource":resource,"reason":reason,"note":note});
        match kind {
            "account" => ensure_valid(resource == target && !disclose, "account report")?,
            "channel" => {
                let sequence = sequence.ok_or_else(|| dmsg_types::invalid("message"))?;
                let message = self
                    .channel_messages_before(resource, sequence.saturating_add(1), 1)?
                    .pop()
                    .ok_or(Error::NotFound)?;
                ensure(
                    message.seq == sequence && message.account == target && !message.unreadable,
                    Error::IntegrityFailed,
                )?;
                payload["message"] = message.message_id.into();
                if disclose {
                    payload["disclosed_text"] = message.text.into();
                }
            }
            "inbox" => {
                let letter = self
                    .inbox_cached()?
                    .into_iter()
                    .find(|l| l.order_id == resource)
                    .ok_or(Error::NotFound)?;
                ensure(letter.sender == target, Error::IntegrityFailed)?;
                if disclose {
                    payload["disclosed_text"] = letter.text.into();
                }
            }
            _ => return Err(Error::UnsupportedProtocol),
        }
        let id = hex::encode(crate::crypto::random_id());
        let job = json!({"id":id,"payload":payload,"state":"pending","created_at":crate::now_ms()});
        self.workspace
            .put_private(self.keys()?, &format!("{REPORT}{id}"), &job)?;
        Ok(job)
    }

    /// Report drafts and receipts saved on this device; secret-free list view.
    pub fn content_reports(&self) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        for key in self.workspace.store.sealed_keys(REPORT)? {
            if let Some(job) = self.workspace.private::<Value>(self.keys()?, &key)? {
                result.push(
                    json!({"id":job["id"],"state":job["state"],"created_at":job["created_at"]}),
                );
            }
        }
        Ok(result)
    }
}

/// Synchronize explicit changes without replacing a newer local choice.
pub async fn sync_blocks<H: Hold>(h: &H) -> Result<Vec<String>> {
    let bound = Unlocked::new(h).await?;
    let (account, pending) = bound
        .with(|c| {
            let blocks: Blocks = c.workspace.private(c.keys()?, BLOCKS)?.unwrap_or_default();
            Ok((c.account()?.id.to_string(), blocks.pending))
        })
        .await?;
    for (target, (blocked, id)) in pending {
        bound
            .post(
                &format!("/v1/accounts/{account}/mobile/block"),
                "dmsg/mobile/block/v1",
                &json!({"target":target,"blocked":blocked}),
                &id,
            )
            .await?;
        bound
            .with(|c| {
                let mut blocks: Blocks =
                    c.workspace.private(c.keys()?, BLOCKS)?.unwrap_or_default();
                if blocks.pending.get(&target) == Some(&(blocked, id)) {
                    blocks.pending.remove(&target);
                }
                c.workspace.put_private(c.keys()?, BLOCKS, &blocks)
            })
            .await?;
    }
    let remote = bound
        .get(&format!("/v1/accounts/{account}/mobile/blocks"))
        .await?;
    let accounts: Vec<String> =
        serde_json::from_value(remote).map_err(|_| Error::IntegrityFailed)?;
    ensure(accounts.len() <= 1000, Error::IntegrityFailed)?;
    for account in &accounts {
        account
            .parse::<AccountId>()
            .map_err(|_| Error::IntegrityFailed)?;
    }
    bound
        .with(|c| {
            let mut blocks: Blocks = c.workspace.private(c.keys()?, BLOCKS)?.unwrap_or_default();
            blocks.accounts = accounts.into_iter().collect();
            for (target, (on, _)) in &blocks.pending {
                if *on {
                    blocks.accounts.insert(target.clone());
                } else {
                    blocks.accounts.remove(target);
                }
            }
            c.workspace.put_private(c.keys()?, BLOCKS, &blocks)?;
            c.blocked_accounts()
        })
        .await
}

/// Retry the original immutable report, never preparing a replacement on timeout.
pub async fn submit_report<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let bound = Unlocked::new(h).await?;
    let (account, mut job) = bound
        .with(|c| {
            Ok((
                c.account()?.id.to_string(),
                c.workspace
                    .private::<Value>(c.keys()?, &format!("{REPORT}{id}"))?
                    .ok_or(Error::NotFound)?,
            ))
        })
        .await?;
    let request = Hash::new(
        hex::decode(id)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| dmsg_types::invalid("report ID"))?,
    );
    let receipt = bound
        .post(
            &format!("/v1/accounts/{account}/mobile/report"),
            "dmsg/mobile/report/v1",
            &job["payload"],
            &request,
        )
        .await?;
    ensure(receipt["id"] == id, Error::IntegrityFailed)?;
    job["state"] = receipt["state"].clone();
    bound
        .with(|c| {
            c.workspace
                .put_private(c.keys()?, &format!("{REPORT}{id}"), &job)
        })
        .await?;
    Ok(receipt)
}
