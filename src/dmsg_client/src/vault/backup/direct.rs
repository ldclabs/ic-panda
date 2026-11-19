//! User-selected content from this installation's standalone workspace into a cloud account.
use super::{
    content::{Body, Entry, FileMeta, Meta},
    local_entry_id, restore,
};
use crate::{
    Client,
    codec::canonical,
    content::VAULT,
    files::{chunk_count, read_chunk},
    hold::Hold,
    local::Access,
    now_ms,
    secrets::staging::StagedFile,
    upload::{hex_id, random_hex},
    vault::reading::Session,
};
use dmsg_types::{Error, LOCAL_ACCOUNT_ID, Result, ensure, ensure_valid};
use serde_bytes::ByteArray;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Import {
    id: String,
    until: u64,
    session: Session,
    source: Client,
    body: Body,
    heads: Vec<(String, String)>,
    versions: Vec<(String, String)>,
    decisions: Vec<restore::Decision>,
    busy: bool,
    done_bytes: u64,
}

/// Inspect only a local source already authenticated by the person.
pub fn preview(c: &mut Client, access: Access) -> Result<Value> {
    super::access(c, true)?;
    let session = Session::start(c)?;
    let mut source = c.local_source(access)?;
    let mut versions = Vec::new();
    let mut media = Vec::new();
    let mut files = Vec::new();
    let mut current = BTreeMap::new();
    for f in source.files_search("", &Default::default())? {
        for v in source.files_versions(&f.id)? {
            ensure(versions.len() < super::content::FILES, Error::QuotaExceeded)?;
            let index = versions.len();
            if f.current == v.id {
                current.insert(f.id.clone(), index);
            }
            media.push(FileMeta {
                name: f.name.clone(),
                mime: v.mime.clone().unwrap_or_else(|| f.mime.clone()),
                size: v.size,
                sha256: ByteArray::new(*hex_id(&v.sha256)?),
            });
            files.push(json!({"index":index,"name":f.name,"path":f.path,"version":v.id,"current":v.id==f.current,"size":v.size,"local":v.local}));
            versions.push((f.id.clone(), v.id));
        }
    }
    let mut entries = Vec::new();
    let mut heads = Vec::new();
    for (id, rev) in source.workspace.store.heads(VAULT)? {
        let v = source.vault_version(&id, &rev)?;
        if v.deleted || v.item.reserved() {
            continue;
        }
        ensure(entries.len() < super::content::ITEMS, Error::QuotaExceeded)?;
        let refs = v
            .item
            .files
            .iter()
            .map(|f| current.get(f).copied())
            .collect();
        let mut e = Entry::new(v, refs);
        e.id = local_entry_id(&source, &id)?;
        heads.push((id, rev));
        entries.push(e);
    }
    let body = Body {
        meta: Meta {
            archive_id: ByteArray::new([0; 16]),
            created_at: now_ms(),
            environment: source.workspace.meta.environment.clone(),
            account: LOCAL_ACCOUNT_ID.to_string(),
            with_attachments: true,
            item_count: entries.len(),
            file_count: media.len(),
            omitted_refs: 0,
        },
        entries,
        files: media,
    };
    let decisions = restore::classify(c, &body)?;
    let id = random_hex();
    let summary:Vec<_>=body.entries.iter().zip(&decisions).enumerate().map(|(i,(e,d))| json!({"index":i,"title":e.title,"state":d.status,"attachments":e.attachment_refs})).collect();
    let result =
        json!({"id":id,"target":c.account()?.id.to_string(),"items":summary,"files":files});
    c.local_import = Some(Box::new(Import {
        id,
        until: now_ms() + super::TTL,
        session,
        source,
        body,
        heads,
        versions,
        decisions,
        busy: false,
        done_bytes: 0,
    }));
    Ok(result)
}

fn check<'a>(c: &'a mut Client, id: &str) -> Result<&'a mut Import> {
    super::access(c, true)?;
    let plan = c
        .local_import
        .as_ref()
        .filter(|p| p.id == id)
        .ok_or(Error::Expired)?;
    plan.session.check(c)?;
    if now_ms() >= plan.until {
        c.local_import = None;
        return Err(Error::Expired);
    }
    Ok(c.local_import.as_deref_mut().unwrap())
}

/// End the named preview and release its source keys.
pub fn cancel(c: &mut Client, id: &str) {
    if c.local_import.as_ref().is_some_and(|p| p.id == id) {
        c.local_import = None;
    }
}

/// Release expired source keys even when the preview is no longer polled.
pub(crate) fn expire(c: &mut Client) {
    if c.local_import.as_ref().is_some_and(|p| p.until <= now_ms()) {
        c.local_import = None;
    }
}

/// Progress or the original committed result after reconnecting.
pub fn status(c: &mut Client, id: &str) -> Result<Value> {
    let receipt = restore::receipt(c, id)?;
    if !receipt.is_null() {
        return Ok(json!({"state":"done","result":receipt}));
    }
    let p = check(c, id)?;
    Ok(json!({"state":if p.busy {"copying"} else {"preview"},"bytes":p.done_bytes}))
}

/// Commit exactly the reviewed file versions and ordinary items. Only target ciphertext is staged.
pub async fn commit<H: Hold>(
    h: &H,
    id: &str,
    items: Vec<usize>,
    files: Vec<usize>,
    folder: &str,
) -> Result<Value> {
    let picks: BTreeSet<_> = items.iter().copied().collect();
    let extra: BTreeSet<_> = files.iter().copied().collect();
    ensure_valid(
        picks.len() == items.len()
            && extra.len() == files.len()
            && (!picks.is_empty() || !extra.is_empty()),
        "migration selection",
    )?;
    let inputs = hex::encode(*crate::codec::sha256(&canonical(&(
        "local-import/1",
        &picks,
        &extra,
        folder,
    ))));
    let (already, required) = h
        .with(|c| {
            c.account()?;
            let key = restore::receipt_key(c, id)?;
            if let Some(r) = c.workspace.private::<restore::Receipt>(c.keys()?, &key)? {
                ensure(r.inputs == inputs, Error::IdempotencyConflict)?;
                return Ok((Some(r.result), BTreeSet::new()));
            }
            c.catalog()?.live_folder(folder)?;
            let p = check(c, id)?;
            ensure(!p.busy, Error::Pending)?;
            let mut required = extra.clone();
            for i in &picks {
                let e = p
                    .body
                    .entries
                    .get(*i)
                    .ok_or_else(|| dmsg_types::invalid("item"))?;
                ensure(p.decisions[*i].status != "same", Error::VersionConflict)?;
                for index in &e.attachment_refs {
                    required.insert(index.ok_or(Error::NotFound)?);
                }
            }
            ensure(
                required.iter().all(|i| *i < p.versions.len()),
                Error::NotFound,
            )?;
            p.busy = true;
            Ok((None, required))
        })
        .await?;
    if let Some(result) = already {
        return Ok(result);
    }
    let result = async {
        let mut staged: BTreeMap<usize, StagedFile> = BTreeMap::new();
        for i in required {
            let (blobs, manifest, mut file) = h
                .with(|c| {
                    let p = check(c, id)?;
                    let (f, v) = &p.versions[i];
                    let (_, _, manifest) = p.source.library_manifest(f, Some(v))?;
                    let blobs = p.source.blobs()?.clone();
                    let meta = &p.body.files[i];
                    ensure(
                        manifest.sha256 == hex::encode(meta.sha256.as_slice())
                            && manifest.size == meta.size,
                        Error::VersionConflict,
                    )?;
                    let file =
                        c.stage_library_file(&manifest.name, &manifest.mime, manifest.size)?;
                    Ok((blobs, manifest, file))
                })
                .await?;
            for index in 0..chunk_count(manifest.size) {
                let plain = read_chunk(&blobs, &manifest, index)?;
                h.with(|c| {
                    check(c, id)?;
                    c.secret_stage_part(&mut file, &plain)?;
                    c.local_import.as_mut().unwrap().done_bytes += plain.len() as u64;
                    Ok(())
                })
                .await?;
            }
            h.with(|c| {
                check(c, id)?;
                c.secret_stage_finish(&mut file, Some(&manifest.sha256))
            })
            .await?;
            staged.insert(i, file);
        }
        h.with(|c| {
            let key = restore::receipt_key(c, id)?;
            check(c, id)?;
            let p = c.local_import.take().ok_or(Error::Expired)?;
            for i in &picks {
                let (item, rev) = &p.heads[*i];
                ensure(
                    p.source.workspace.store.head(item)?.as_ref() == Some(rev),
                    Error::VersionConflict,
                )?;
            }
            restore::publish_selection(
                c,
                &p.body,
                &p.decisions,
                &picks,
                &extra,
                &staged,
                folder,
                &key,
                &inputs,
            )
        })
        .await
    }
    .await;
    if result.is_err() {
        let _ = h
            .with(|c| {
                cancel(c, id);
                Ok(())
            })
            .await;
    }
    result
}
