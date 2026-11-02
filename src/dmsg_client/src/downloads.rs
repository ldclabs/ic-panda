//! Short-lived, GET-only file downloads. URLs stay in this operation's memory.
use crate::{cloud::Cloud, hold::Hold};
use dmsg_types::{Error, Result, ensure};
use futures_util::{StreamExt, stream::FuturesUnordered};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize, Clone)]
pub(crate) struct Slot {
    pub url: String,
    pub digest: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct Grant {
    upload_id: String,
    expires_at: u64,
    manifest: Slot,
    chunks: Vec<Slot>,
}

fn location(path: &str) -> Result<(&str, &str)> {
    let (object, part) = path.rsplit_once("/chunks/").ok_or(Error::IntegrityFailed)?;
    let (_, upload) = object
        .rsplit_once("/objects/")
        .ok_or(Error::IntegrityFailed)?;
    crate::upload::hex_id(upload)?;
    Ok((object, part))
}

impl Grant {
    fn slot(&self, object: &str, part: &str, digest: &str) -> Result<Slot> {
        ensure(
            object.ends_with(&format!("/objects/{}", self.upload_id))
                && self.chunks.len()
                    <= (crate::files::MAX_BACKUP / crate::files::CHUNK as u64) as usize,
            Error::IntegrityFailed,
        )?;
        let slot = if part == "manifest" {
            Some(&self.manifest)
        } else {
            part.parse::<usize>().ok().and_then(|n| self.chunks.get(n))
        }
        .ok_or(Error::IntegrityFailed)?;
        ensure(
            slot.digest == digest && slot.size <= crate::cloud::MAX_CHUNK as u64,
            Error::IntegrityFailed,
        )?;
        Ok(slot.clone())
    }
}

async fn grant<H: Hold>(h: &H, object: &str) -> Result<Grant> {
    serde_json::from_value(h.get(&format!("{object}/download")).await?)
        .map_err(|_| Error::IntegrityFailed)
}

/// Fetch bounded groups without holding the client lock. One authorization per
/// object, renewed only on expiry or a failed data request, never per block.
pub(crate) async fn chunks<H: Hold>(
    h: &H,
    parts: &[(String, String)],
    mut keep: impl FnMut(&mut crate::client::Client, &[(String, String)], Vec<Vec<u8>>) -> Result<()>
    + Send,
) -> Result<()> {
    let session = crate::hold::Unlocked::new(h).await?;
    let h = &session;
    let relay: Cloud = h.with(|c| Ok(c.relay())).await?;
    let mut grants: BTreeMap<String, Grant> = BTreeMap::new();
    let mut pending = FuturesUnordered::new();
    let mut next = 0;
    let mut attempts = vec![0; parts.len()];
    while next < parts.len() || !pending.is_empty() {
        while next < parts.len() && pending.len() < 8 {
            h.with(|_| Ok(())).await?;
            let (path, digest) = &parts[next];
            let (object, part) = location(path)?;
            if grants
                .get(object)
                .is_none_or(|g| g.expires_at <= crate::now_ms() + 1000)
            {
                grants.insert(object.into(), grant(h, object).await?);
            }
            let authorization = &grants[object];
            pending.push(fetch(
                relay.clone(),
                next,
                authorization.expires_at,
                authorization.slot(object, part, digest)?,
                digest.clone(),
            ));
            next += 1;
        }
        let Some((index, expiry, result)) = pending.next().await else {
            break;
        };
        let (path, digest) = &parts[index];
        match result {
            Ok(bytes) => {
                h.with(|c| keep(c, &parts[index..index + 1], vec![bytes]))
                    .await?
            }
            Err(Error::Expired | Error::NotFound | Error::Unavailable(_))
                if attempts[index] == 0 =>
            {
                attempts[index] += 1;
                let (object, part) = location(path)?;
                // Concurrent failures under the same grant share one renewal.
                if grants[object].expires_at == expiry {
                    grants.insert(object.into(), grant(h, object).await?);
                }
                let authorization = &grants[object];
                pending.push(fetch(
                    relay.clone(),
                    index,
                    authorization.expires_at,
                    authorization.slot(object, part, digest)?,
                    digest.clone(),
                ));
            }
            Err(Error::NotFound) => return Err(Error::IntegrityFailed),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

async fn fetch(
    relay: Cloud,
    index: usize,
    expiry: u64,
    slot: Slot,
    digest: String,
) -> (usize, u64, Result<Vec<u8>>) {
    (index, expiry, relay.download_slot(&slot, &digest).await)
}

pub(crate) async fn one<H: Hold>(h: &H, path: String, digest: &str) -> Result<Vec<u8>> {
    let mut bytes = None;
    chunks(h, &[(path, digest.into())], |_, _, mut fetched| {
        bytes = fetched.pop();
        Ok(())
    })
    .await?;
    bytes.ok_or(Error::IntegrityFailed)
}

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
