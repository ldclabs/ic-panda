//! Short-lived, GET-only file downloads. URLs stay in this operation's memory.
use crate::{cloud::Cloud, hold::Hold};
use dmsg_types::{Error, Result, ensure};
use futures_util::future::join_all;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

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
            object.ends_with(&format!("/objects/{}", self.upload_id)) && self.chunks.len() <= 128,
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
    let relay: Cloud = h
        .with(|c| {
            c.keys()?;
            Ok(c.relay())
        })
        .await?;
    let mut grants: BTreeMap<String, Grant> = BTreeMap::new();
    for group in parts.chunks(8) {
        h.with(|c| {
            c.keys()?;
            Ok(())
        })
        .await?;
        let mut fetch = Vec::new();
        for (path, digest) in group {
            let (object, part) = location(path)?;
            if grants
                .get(object)
                .is_none_or(|g| g.expires_at <= crate::now_ms() + 1000)
            {
                grants.insert(object.into(), grant(h, object).await?);
            }
            fetch.push((
                object.to_owned(),
                part.to_owned(),
                digest.clone(),
                grants[object].slot(object, part, digest)?,
            ));
        }
        let mut received: Vec<Option<Vec<u8>>> = vec![None; fetch.len()];
        for attempt in 0..2 {
            let pending: Vec<_> = fetch
                .iter()
                .enumerate()
                .filter(|(i, _)| received[*i].is_none())
                .collect();
            let results = join_all(pending.iter().map(|(_, (object, part, digest, _))| {
                let slot = grants[object].slot(object, part, digest);
                let relay = &relay;
                async move { relay.download_slot(&slot?, digest).await }
            }))
            .await;
            let mut renew = BTreeSet::new();
            for ((index, (object, _, _, _)), result) in pending.into_iter().zip(results) {
                match result {
                    Ok(bytes) => received[index] = Some(bytes),
                    Err(Error::Expired | Error::NotFound | Error::Unavailable(_))
                        if attempt == 0 =>
                    {
                        renew.insert(object.clone());
                    }
                    Err(Error::NotFound) => return Err(Error::IntegrityFailed),
                    Err(error) => return Err(error),
                }
            }
            if renew.is_empty() {
                break;
            }
            // One renewal per object even when all concurrent blocks fail together.
            for object in renew {
                grants.insert(object.clone(), grant(h, &object).await?);
            }
        }
        let results = received
            .into_iter()
            .map(|b| b.ok_or(Error::IntegrityFailed))
            .collect::<Result<Vec<_>>>()?;
        h.with(|c| {
            c.keys()?;
            keep(c, group, results)
        })
        .await?;
    }
    Ok(())
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
