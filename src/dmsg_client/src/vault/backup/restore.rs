use super::{
    Stage, active, cancel,
    content::{self, Body, Entry, FileEvent},
    local_file, wire,
};
use crate::{
    Client, codec::canonical, content::VAULT, hold::Hold, secrets::staging::StagedFile,
    upload::hex_id,
};
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Read, Seek, Write},
    path::Path,
    sync::atomic::Ordering,
};

const SOURCE_PREFIX: &str = "vault-backup-source:";
const RECEIPT_PREFIX: &str = "vault-backup-receipt:";

pub(super) struct Reviewed {
    body: Body,
    files: Vec<StagedFile>,
    digest: String,
    decisions: Vec<Decision>,
}

#[derive(Serialize, PartialEq, Eq)]
struct Decision {
    status: &'static str,
    target: Option<(String, String)>,
}

struct Existing {
    revision: String,
    deleted: bool,
    fingerprint: Option<[u8; 32]>,
    files: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize)]
struct Origin {
    id: String,
    revision: String,
    fingerprint: [u8; 32],
    files: Vec<(String, String)>,
}

fn source_key(c: &Client, body: &Body, source: &str) -> Result<String> {
    Ok(format!(
        "{SOURCE_PREFIX}{}",
        hex::encode(
            c.keys()?
                .derive(&(
                    "dmsg/vault-backup/source/1",
                    &body.meta.environment,
                    &body.meta.account,
                    source
                ))
                .as_slice()
        )
    ))
}

fn classify(c: &mut Client, body: &Body) -> Result<Vec<Decision>> {
    let mut existing = BTreeMap::new();
    let mut titles = BTreeMap::new();
    let mut fingerprints: BTreeMap<[u8; 32], Vec<String>> = BTreeMap::new();
    let mut metas = BTreeMap::new();
    for (id, rev) in c.workspace.store.heads(VAULT)? {
        let e = c.vault_version(&id, &rev)?;
        if e.item.reserved() {
            continue;
        }
        let deleted = e.deleted;
        let mut file_versions = Vec::new();
        let fingerprint = if deleted {
            None
        } else {
            for f in &e.item.files {
                if !metas.contains_key(f) {
                    let value = match local_file(c, f) {
                        Ok(file) => Some(file),
                        Err(Error::NotFound) => None,
                        Err(e) => return Err(e),
                    };
                    metas.insert(f.clone(), value);
                }
                if let Some(info) = &metas[f] {
                    file_versions.push((f.clone(), info.version.clone()));
                }
            }
            let refs: Vec<_> = e
                .item
                .files
                .iter()
                .map(|id| metas[id].as_ref().map(|f| &f.meta))
                .collect();
            titles
                .entry(e.item.title.clone())
                .or_insert_with(|| id.clone());
            let hash = refs
                .iter()
                .all(Option::is_some)
                .then(|| Entry::new(e, vec![]).fingerprint(&refs));
            if let Some(hash) = hash {
                fingerprints.entry(hash).or_default().push(id.clone());
            }
            hash
        };
        existing.insert(
            id,
            Existing {
                revision: rev,
                deleted,
                fingerprint,
                files: file_versions,
            },
        );
    }
    let same_account = body.meta.environment == c.workspace.meta.environment
        && body.meta.account == c.account()?.id.to_string();
    let mut used = BTreeSet::new();
    let mut origins = Vec::with_capacity(body.entries.len());
    for e in &body.entries {
        let saved = c
            .workspace
            .private::<Origin>(c.keys()?, &source_key(c, body, &e.id)?)?;
        let id = saved
            .as_ref()
            .map(|s| s.id.clone())
            .or_else(|| (same_account && existing.contains_key(&e.id)).then(|| e.id.clone()));
        if let Some(id) = &id {
            used.insert(id.clone());
        }
        origins.push((id, saved));
    }
    let mut decisions = Vec::with_capacity(body.entries.len());
    for (e, (origin, saved)) in body.entries.iter().zip(origins) {
        let refs: Vec<_> = e
            .attachment_refs
            .iter()
            .map(|f| f.map(|i| &body.files[i]))
            .collect();
        let hash = e.fingerprint(&refs);
        let (status, id) = if let Some(id) = origin {
            let status = match existing.get(&id) {
                None => "deleted",
                Some(target) if target.deleted => "deleted",
                Some(target)
                    if saved.as_ref().is_some_and(|s| {
                        s.fingerprint == hash
                            && s.revision == target.revision
                            && s.files == target.files
                    }) =>
                {
                    "same"
                }
                Some(target) if target.fingerprint == Some(hash) => "same",
                _ => "changed",
            };
            (status, Some(id))
        } else if let Some(id) = fingerprints.get_mut(&hash).and_then(|ids| {
            while let Some(id) = ids.pop() {
                if !used.contains(&id) {
                    return Some(id);
                }
            }
            None
        }) {
            ("same", Some(id))
        } else if let Some(id) = titles.get(&e.title) {
            ("changed", Some(id.clone()))
        } else {
            ("new", None)
        };
        if let Some(id) = &id {
            used.insert(id.clone());
        }
        let target = id.map(|id| {
            let rev = existing
                .get(&id)
                .map(|t| t.revision.clone())
                .unwrap_or_default();
            (id, rev)
        });
        decisions.push(Decision { status, target });
    }
    Ok(decisions)
}

/// Verify an immutable private file snapshot and stage only ciphertext for its
/// attachments. The preview contains counts, titles and decisions.
pub async fn inspect<H: Hold>(h: &H, id: &str, path: &Path, password: &str) -> Result<Value> {
    let password = wire::password(password, false)?;
    let (guard, temp) = h
        .with(|c| {
            let guard = super::prepare(c, id, true)?;
            Ok((
                guard,
                tempfile::tempfile_in(c.slot_dir(&c.slot)).map_err(wire::file_io)?,
            ))
        })
        .await?;
    let path = path.to_owned();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let reading = guard.clone();
    let worker = tokio::task::spawn_blocking(move || {
        let mut temp = temp;
        let mut input = File::open(path).map_err(wire::file_io)?;
        let meta = input.metadata().map_err(wire::file_io)?;
        ensure(
            meta.is_file() && meta.len() <= wire::MAX_ARCHIVE as u64,
            Error::QuotaExceeded,
        )?;
        let mut buf = vec![0; crate::files::CHUNK];
        let mut copied = 0;
        loop {
            guard.check()?;
            let n = input.read(&mut buf).map_err(wire::file_io)?;
            if n == 0 {
                break;
            }
            copied += n;
            ensure(copied <= wire::MAX_ARCHIVE, Error::QuotaExceeded)?;
            temp.write_all(&buf[..n]).map_err(wire::file_io)?;
        }
        temp.rewind().map_err(wire::file_io)?;
        content::read(
            temp,
            &password,
            |event| tx.blocking_send(event).map_err(|_| Error::Expired),
            || guard.check(),
        )
    });
    let mut staged = Vec::new();
    let mut current = None;
    while let Some(event) = rx.recv().await {
        h.with(|c| {
            reading.check()?;
            let guard = active(c, id, true)?.guard.clone();
            match event {
                FileEvent::Begin(f) => {
                    current = Some(c.secret_stage_start(&f.name, &f.mime, f.size)?)
                }
                FileEvent::Chunk(bytes) => {
                    c.secret_stage_part(current.as_mut().ok_or(Error::IntegrityFailed)?, &bytes)?;
                    guard.progress.fetch_add(bytes.len(), Ordering::Release);
                }
                FileEvent::End => {
                    let mut f = current.take().ok_or(Error::IntegrityFailed)?;
                    c.secret_stage_finish(&mut f, None)?;
                    staged.push(f);
                }
            }
            Ok(())
        })
        .await?;
    }
    let (body, digest) = worker.await.map_err(|_| Error::Expired)??;
    h.with(|c| {
        reading.check()?;
        active(c,id,true)?;
        let decisions=classify(c,&body)?;
        let rows:Vec<_>=body.entries.iter().zip(&decisions).enumerate().map(|(index,(e,d))|
            json!({"index":index,"title":e.title,"kind":e.kind,"files":e.attachment_refs,"status":d.status})).collect();
        let result=json!({"id":id,"digest":digest,"source":body.meta.account,"account":c.account()?.id.to_string(),"at":body.meta.created_at,"items":rows,"files":body.files.len(),"omitted":body.meta.omitted_refs});
        active(c,id,true)?.state=Stage::Reviewed(Reviewed{body,files:staged,digest,decisions});
        Ok(result)
    }).await
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    inputs: String,
    result: Value,
}

fn receipt_key(c: &Client, id: &str) -> Result<String> {
    hex_id(id)?;
    Ok(format!(
        "{RECEIPT_PREFIX}{}",
        hex::encode(
            c.keys()?
                .derive(&("dmsg/vault-backup/receipt/1", id))
                .as_slice()
        )
    ))
}

/// The original local outcome after a lost response; null means uncommitted.
pub fn receipt(c: &Client, id: &str) -> Result<Value> {
    Ok(c.workspace
        .private::<Receipt>(c.keys()?, &receipt_key(c, id)?)?
        .map(|r| r.result)
        .unwrap_or(Value::Null))
}

/// Publish selected entries, remapped attachments and an exact retry receipt
/// in one workspace transaction. An existing item is never overwritten.
pub fn commit(
    c: &mut Client,
    id: &str,
    digest: &str,
    picks: Vec<usize>,
    folder: &str,
) -> Result<Value> {
    let unique: BTreeSet<_> = picks.iter().copied().collect();
    ensure_valid(
        !unique.is_empty() && unique.len() == picks.len() && unique.len() <= content::ITEMS,
        "backup selection",
    )?;
    let key = receipt_key(c, id)?;
    let inputs = hex::encode(*crate::codec::sha256(&canonical(&(
        digest, &unique, folder,
    ))));
    if let Some(original) = c.workspace.private::<Receipt>(c.keys()?, &key)? {
        ensure(original.inputs == inputs, Error::IdempotencyConflict)?;
        return Ok(original.result);
    }
    let op = active(c, id, true)?;
    let Stage::Reviewed(review) = std::mem::replace(&mut op.state, Stage::Working) else {
        return Err(Error::Expired);
    };
    let apply = |c: &mut Client| -> Result<Value> {
        ensure(review.digest == digest, Error::VersionConflict)?;
        let latest = classify(c, &review.body)?;
        let mut files = BTreeSet::new();
        let mut omitted = 0;
        let mut items = Vec::new();
        let mut keys = Vec::new();
        for i in &unique {
            let d = latest
                .get(*i)
                .ok_or_else(|| dmsg_types::invalid("backup selection"))?;
            ensure(*d == review.decisions[*i], Error::VersionConflict)?;
            ensure(d.status != "same", Error::VersionConflict)?;
            let e = &review.body.entries[*i];
            let mut refs = Vec::new();
            for f in &e.attachment_refs {
                if let Some(f) = f {
                    refs.push(review.files[*f].id().to_owned());
                    files.insert(*f);
                } else {
                    omitted += 1;
                }
            }
            let source_files = e
                .attachment_refs
                .iter()
                .map(|f| f.map(|i| &review.body.files[i]))
                .collect::<Vec<_>>();
            let imported_files = e
                .attachment_refs
                .iter()
                .flatten()
                .map(|i| {
                    let f = &review.files[*i];
                    (f.id().to_owned(), f.version().to_owned())
                })
                .collect::<Vec<_>>();
            keys.push((
                source_key(c, &review.body, &e.id)?,
                e.fingerprint(&source_files),
                imported_files,
            ));
            items.push(e.item(refs));
        }
        let staged: Vec<_> = files.iter().map(|i| review.files[*i].clone()).collect();
        let result = json!({"items":items.len(),"files":staged.len(),"omitted":omitted,"skipped":review.body.entries.len()-items.len()});
        c.publish_import(
            items,
            staged,
            folder,
            Client::vault_write_exact_items,
            |c, ids| {
                for ((k, fingerprint, files), target) in keys.into_iter().zip(ids) {
                    let revision = c
                        .workspace
                        .store
                        .head(target)?
                        .ok_or(Error::IntegrityFailed)?;
                    c.workspace.put_private(
                        c.keys()?,
                        &k,
                        &Origin {
                            id: target.clone(),
                            revision,
                            fingerprint,
                            files,
                        },
                    )?;
                }
                c.workspace.put_private(
                    c.keys()?,
                    &key,
                    &Receipt {
                        inputs,
                        result: result.clone(),
                    },
                )
            },
        )?;
        Ok(result)
    };
    let result = apply(c);
    if result.is_ok() {
        cancel(c, id);
    } else {
        active(c, id, true)?.state = Stage::Reviewed(review);
    }
    result
}
