use super::wire::{self, MAX_RECORD, Reader, Writer};
use crate::{
    codec::canonical,
    files::{CHUNK, MAX_FILE},
    library::catalog,
    upload::hex_id,
    vault::{Field, Item, VaultEntry},
};
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
};
use zeroize::{Zeroize, Zeroizing};

pub(super) const ITEMS: usize = 10_000;
pub(super) const FILES: usize = 4096;
pub(super) const FILE_BYTES: u64 = 256 << 20;
const META_BYTES: usize = 16 << 20;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Meta {
    pub archive_id: ByteArray<16>,
    pub created_at: u64,
    pub environment: String,
    pub account: String,
    pub with_attachments: bool,
    pub item_count: usize,
    pub file_count: usize,
    pub omitted_refs: usize,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FileMeta {
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub sha256: ByteArray<32>,
}

// This is a positive list of portable fields. In particular no FileManifest,
// source file key, resolved-conflict graph or workspace record can enter it.
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Entry {
    pub id: String,
    pub revision: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub tags: Vec<String>,
    pub notes: String,
    pub fields: Vec<Field>,
    pub favorite: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub attachment_refs: Vec<Option<usize>>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.title.zeroize();
        self.tags.zeroize();
        self.notes.zeroize();
        for field in &mut self.fields {
            field.label.zeroize();
            field.section.zeroize();
            field.value.zeroize();
        }
    }
}

impl Entry {
    pub fn new(e: VaultEntry, refs: Vec<Option<usize>>) -> Self {
        let i = e.item;
        Self {
            id: e.id,
            revision: e.revision,
            kind: i.kind,
            title: i.title,
            tags: i.tags,
            notes: i.notes,
            fields: i.fields,
            favorite: i.favorite,
            created_at: i.created_at,
            updated_at: i.updated_at,
            attachment_refs: refs,
        }
    }

    pub fn item(&self, files: Vec<String>) -> Item {
        Item {
            kind: self.kind.clone(),
            title: self.title.clone(),
            tags: self.tags.clone(),
            notes: self.notes.clone(),
            fields: self.fields.clone(),
            files,
            favorite: self.favorite,
            created_at: self.created_at,
            updated_at: self.updated_at,
            resolved_conflicts: None,
        }
    }

    pub fn fingerprint(&self, files: &[Option<&FileMeta>]) -> [u8; 32] {
        Sha256::digest(&*Zeroizing::new(canonical(&(
            &self.kind,
            &self.title,
            &self.tags,
            &self.notes,
            &self.fields,
            self.favorite,
            files,
        ))))
        .into()
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct Body {
    pub meta: Meta,
    pub entries: Vec<Entry>,
    pub files: Vec<FileMeta>,
}

impl Body {
    pub fn write<W: Write>(
        &self,
        dest: W,
        password: &str,
        mut get: impl FnMut(usize, u64) -> Result<Zeroizing<Vec<u8>>>,
        mut check: impl FnMut() -> Result<()>,
    ) -> Result<W> {
        let mut records = Vec::new();
        let mut metadata = 0;
        for entry in &self.entries {
            check()?;
            let bytes = Zeroizing::new(canonical(entry));
            metadata += bytes.len();
            ensure(
                bytes.len() <= MAX_RECORD && metadata <= META_BYTES,
                Error::QuotaExceeded,
            )?;
            records.push(bytes);
        }
        let mut start = vec![0x83];
        start.extend(canonical(&self.meta));
        start.extend(wire::prefix(4, records.len() as u64));
        let file_count = wire::prefix(4, self.files.len() as u64);
        let mut headers = Vec::new();
        let mut size = metadata + start.len() + file_count.len();
        for f in &self.files {
            let mut header = vec![0x82];
            header.extend(canonical(f));
            header.extend(wire::prefix(2, f.size));
            metadata += header.len();
            size += header.len() + f.size as usize;
            headers.push(header);
        }
        ensure(metadata + start.len() <= META_BYTES, Error::QuotaExceeded)?;
        let mut writer = Writer::new(dest, size, password)?;
        check()?;
        writer.push(&start)?;
        for record in records {
            check()?;
            writer.push(&record)?;
        }
        writer.push(&file_count)?;
        for (i, (f, header)) in self.files.iter().zip(headers).enumerate() {
            writer.push(&header)?;
            let mut digest = Sha256::new();
            for chunk in 0..f.size.div_ceil(CHUNK as u64) {
                check()?;
                let bytes = get(i, chunk)?;
                ensure(
                    bytes.len() as u64 == (f.size - chunk * CHUNK as u64).min(CHUNK as u64),
                    Error::IntegrityFailed,
                )?;
                digest.update(&bytes);
                writer.push(&bytes)?;
            }
            ensure(
                digest.finalize().as_slice() == f.sha256.as_slice(),
                Error::IntegrityFailed,
            )?;
        }
        writer.finish()
    }
}

pub(super) enum FileEvent {
    Begin(FileMeta),
    Chunk(Zeroizing<Vec<u8>>),
    End,
}

pub(super) fn read(
    input: impl Read,
    password: &str,
    mut emit: impl FnMut(FileEvent) -> Result<()>,
    mut check: impl FnMut() -> Result<()>,
) -> Result<(Body, String)> {
    let mut reader = Reader::new(input, password)?;
    check()?;
    ensure(
        wire::length(&mut reader, 4, 3)? == 3,
        Error::IntegrityFailed,
    )?;
    let (meta, mut used): (Meta, _) = wire::record(&mut reader, MAX_RECORD)?;
    meta.account
        .parse::<dmsg_types::AccountId>()
        .map_err(|_| Error::IntegrityFailed)?;
    ensure(
        meta.environment.len() <= 128 && !meta.environment.is_empty(),
        Error::IntegrityFailed,
    )?;
    let count = wire::length(&mut reader, 4, ITEMS)?;
    ensure(
        count > 0 && count == meta.item_count && meta.file_count <= FILES,
        Error::IntegrityFailed,
    )?;
    let mut entries = Vec::with_capacity(count);
    let mut ids = BTreeSet::new();
    let mut referenced = BTreeSet::new();
    let mut omitted = 0;
    for _ in 0..count {
        check()?;
        let (e, size): (Entry, _) = wire::record(&mut reader, MAX_RECORD.min(META_BYTES - used))?;
        used += size;
        hex_id(&e.id)?;
        hex_id(&e.revision)?;
        ensure(
            ids.insert(e.id.clone()) && e.attachment_refs.len() <= super::super::MAX_FILES,
            Error::IntegrityFailed,
        )?;
        let i = e.item(Vec::new());
        ensure(
            !i.tags
                .iter()
                .any(|t| t.trim().starts_with(super::super::RESERVED_TAG)),
            Error::Forbidden,
        )?;
        i.validate()?;
        let mut unique = BTreeSet::new();
        for reference in &e.attachment_refs {
            match reference {
                Some(n) => {
                    ensure(
                        meta.with_attachments && *n < meta.file_count && unique.insert(*n),
                        Error::IntegrityFailed,
                    )?;
                    referenced.insert(*n);
                }
                None => {
                    ensure(!meta.with_attachments, Error::IntegrityFailed)?;
                    omitted += 1;
                }
            }
        }
        entries.push(e);
    }
    let count = wire::length(&mut reader, 4, FILES)?;
    ensure(
        count == meta.file_count && referenced.len() == count && omitted == meta.omitted_refs,
        Error::IntegrityFailed,
    )?;
    let mut files = Vec::with_capacity(count);
    let mut total = 0;
    for _ in 0..count {
        check()?;
        ensure(
            wire::length(&mut reader, 4, 2)? == 2,
            Error::IntegrityFailed,
        )?;
        let (f, size): (FileMeta, _) =
            wire::record(&mut reader, MAX_RECORD.min(META_BYTES - used))?;
        used += size;
        catalog::validate_name(&f.name)?;
        ensure(
            f.mime.len() <= 1024 && f.size <= MAX_FILE && f.size <= FILE_BYTES - total,
            Error::QuotaExceeded,
        )?;
        total += f.size;
        ensure(
            wire::length(&mut reader, 2, MAX_FILE as usize)? as u64 == f.size,
            Error::IntegrityFailed,
        )?;
        emit(FileEvent::Begin(f.clone()))?;
        let mut remaining = f.size;
        let mut digest = Sha256::new();
        while remaining > 0 {
            check()?;
            let mut buf = Zeroizing::new(vec![0; remaining.min(CHUNK as u64) as usize]);
            reader.read_exact(&mut buf).map_err(wire::file_io)?;
            digest.update(&buf);
            remaining -= buf.len() as u64;
            emit(FileEvent::Chunk(buf))?;
        }
        ensure(
            digest.finalize().as_slice() == f.sha256.as_slice(),
            Error::IntegrityFailed,
        )?;
        emit(FileEvent::End)?;
        files.push(f);
    }
    check()?;
    Ok((
        Body {
            meta,
            entries,
            files,
        },
        reader.finish()?,
    ))
}
