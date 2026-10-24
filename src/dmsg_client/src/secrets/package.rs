//! Length-delimited secret snapshots inside the existing encrypted share-file format.
use super::*;
use crate::vault::{Field, Item};
use std::collections::BTreeSet;

/// Package media type; recipients must not offer a whole plaintext-package download.
pub const MIME: &str = "application/vnd.dmsg.secret-package";
/// Exact wire format identifier.
pub const FORMAT: &str = "dmsg-secret-package/1";
/// Maximum JSON manifest length.
pub const MAX_MANIFEST: usize = 256 * 1024;

/// Selected item content only; no source account, source IDs, keys, tags or history.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// Item type.
    #[serde(rename = "type")]
    pub kind: String,
    /// Untrusted display text.
    pub title: String,
    /// Only explicitly selected fields, with package-local IDs.
    pub fields: Vec<Field>,
    /// Explicitly selected notes, absent by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Ordered attachment description. Names are display suggestions, never paths.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    /// Original name, untrusted plain text.
    pub name: String,
    /// Media type, not executable markup.
    pub mime: String,
    /// Exact number of following bytes.
    pub size: u64,
    /// Full plaintext SHA-256.
    pub sha256: String,
}

/// Encrypted by the outer file container; never printed to logs or preview APIs.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// dmsg-secret-package/1.
    pub format: String,
    /// Random snapshot identity, independent of source objects.
    pub snapshot_id: String,
    /// Snapshot time, no attested wall-clock claim.
    pub created_at: u64,
    /// Explicitly selected content.
    pub item: Snapshot,
    /// In physical byte order, no offsets or filesystem paths.
    pub attachments: Vec<Attachment>,
}

impl Drop for Manifest {
    fn drop(&mut self) {
        for field in &mut self.item.fields {
            field.value.zeroize();
        }
        if let Some(notes) = &mut self.item.notes {
            notes.zeroize();
        }
    }
}

impl Manifest {
    /// Validate every bound before constructing a preview or importing anything.
    pub fn validate(&self) -> Result<()> {
        ensure(self.format == FORMAT, Error::UnsupportedProtocol)?;
        ensure_valid(
            hex_id(&self.snapshot_id, 64)
                && self.created_at <= 9_007_199_254_740_991
                && self.attachments.len() <= 20
                && self.item.fields.len() <= 64,
            "package manifest",
        )?;
        let mut ids = BTreeSet::new();
        for field in &self.item.fields {
            ensure_valid(ids.insert(&field.id), "duplicate field")?;
        }
        let mut item = Item::new(&self.item.kind, &self.item.title, self.created_at);
        item.fields = self.item.fields.clone();
        item.notes = self.item.notes.clone().unwrap_or_default();
        let checked = item.validate();
        for f in &mut item.fields {
            f.value.zeroize();
        }
        item.notes.zeroize();
        checked?;
        for attachment in &self.attachments {
            ensure_valid(
                !attachment.name.is_empty()
                    && attachment.name.len() <= 1024
                    && !attachment.name.contains('\0')
                    && attachment.mime.len() <= 256
                    && !attachment.mime.chars().any(char::is_control)
                    && hex_id(&attachment.sha256, 64)
                    && attachment.size <= crate::files::MAX_FILE,
                "package attachment",
            )?;
        }
        Ok(())
    }

    /// Header bytes for streaming directly into a fresh encrypted file.
    pub fn header(&self) -> Result<Zeroizing<Vec<u8>>> {
        self.validate()?;
        let json = Zeroizing::new(serde_json::to_vec(self).map_err(|_| Error::IntegrityFailed)?);
        ensure(json.len() <= MAX_MANIFEST, Error::QuotaExceeded)?;
        let total = self
            .attachments
            .iter()
            .try_fold(4 + json.len() as u64, |n, a| {
                n.checked_add(a.size).ok_or(Error::QuotaExceeded)
            })?;
        ensure(total <= crate::files::MAX_FILE, Error::QuotaExceeded)?;
        let mut bytes = Zeroizing::new((json.len() as u32).to_be_bytes().to_vec());
        bytes.extend_from_slice(&json);
        Ok(bytes)
    }

    /// Metadata only, before any field reveal or save action.
    pub fn preview(&self) -> Value {
        json!({"snapshot_id":self.snapshot_id,"created_at":self.created_at,"type":self.item.kind,"title":self.item.title,"fields":self.item.fields.iter().map(|f|json!({"id":f.id,"type":f.kind,"label":f.label,"section":f.section})).collect::<Vec<_>>(),"notes":self.item.notes.is_some(),"attachments":self.attachments})
    }

    /// A new local item; remote IDs never overwrite local identities or grant access.
    pub fn new_item(&self, fields: &[String], include_notes: bool) -> Result<Item> {
        self.validate()?;
        let selected: BTreeSet<_> = fields.iter().collect();
        ensure_valid(
            selected.len() == fields.len()
                && selected
                    .iter()
                    .all(|id| self.item.fields.iter().any(|f| &f.id == *id)),
            "package selection",
        )?;
        let mut item = Item::new(&self.item.kind, &self.item.title, now_ms());
        let mut roles = BTreeSet::new();
        item.fields = self
            .item
            .fields
            .iter()
            .filter(|f| selected.contains(&f.id))
            .enumerate()
            .map(|(i, f)| {
                let mut field = f.clone();
                let role =
                    f.id.split_once('_')
                        .filter(|(prefix, _)| {
                            prefix.strip_prefix('f').is_some_and(|n| {
                                !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
                            })
                        })
                        .map(|(_, role)| role);
                field.id = role
                    .filter(|role| {
                        crate::vault::template(&self.item.kind)
                            .iter()
                            .any(|(id, kind)| id == role && *kind == f.kind)
                            && roles.insert((*role).to_owned())
                    })
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("f{i}"));
                field
            })
            .collect();
        item.notes = if include_notes {
            self.item.notes.clone().unwrap_or_default()
        } else {
            String::new()
        };
        item.validate()?;
        Ok(item)
    }
}

/// Fully validated container; no field or attachment is exposed before all hashes pass.
pub struct Package<'a> {
    /// Manifest, containing sensitive field values.
    pub manifest: Manifest,
    /// Borrowed original byte slices; callers only save explicitly chosen entries.
    pub attachments: Vec<&'a [u8]>,
}

/// Reject unknown formats, malformed lengths, trailing bytes and any bad attachment.
pub fn parse(bytes: &[u8]) -> Result<Package<'_>> {
    ensure(
        bytes.len() as u64 <= crate::files::MAX_FILE && bytes.len() >= 4,
        Error::IntegrityFailed,
    )?;
    let length =
        u32::from_be_bytes(bytes[..4].try_into().map_err(|_| Error::IntegrityFailed)?) as usize;
    ensure(
        length <= MAX_MANIFEST && 4 + length <= bytes.len(),
        Error::IntegrityFailed,
    )?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes[4..4 + length]).map_err(|_| Error::IntegrityFailed)?;
    manifest.validate()?;
    let mut offset = 4 + length;
    let mut attachments = Vec::new();
    for item in &manifest.attachments {
        let size = usize::try_from(item.size).map_err(|_| Error::IntegrityFailed)?;
        let end = offset.checked_add(size).ok_or(Error::IntegrityFailed)?;
        let data = bytes.get(offset..end).ok_or(Error::IntegrityFailed)?;
        ensure(
            hex::encode(Sha256::digest(data)) == item.sha256,
            Error::IntegrityFailed,
        )?;
        attachments.push(data);
        offset = end;
    }
    ensure(offset == bytes.len(), Error::IntegrityFailed)?;
    Ok(Package {
        manifest,
        attachments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_cross_language_vector() {
        let vector: Value = serde_json::from_str(include_str!(
            "../../../../tests/vectors/client/secret-package-v1.json"
        ))
        .unwrap();
        let bytes = crate::codec::unb64(vector["bytes_base64url"].as_str().unwrap()).unwrap();
        let package = parse(&bytes).unwrap();
        assert_eq!(
            serde_json::to_value(&package.manifest).unwrap(),
            vector["manifest"]
        );
        assert_eq!(hex::encode(Sha256::digest(&bytes)), vector["sha256"]);
        let mut encoded = package.manifest.header().unwrap();
        for attachment in package.attachments {
            encoded.extend_from_slice(attachment);
        }
        assert_eq!(&*encoded, &bytes);
    }

    #[test]
    fn whole_package_is_checked_before_preview_or_attachment_exposure() {
        let m = Manifest {
            format: FORMAT.into(),
            snapshot_id: "a7".repeat(32),
            created_at: 1,
            item: Snapshot {
                kind: "note".into(),
                title: "<script>untrusted</script>".into(),
                fields: vec![],
                notes: Some("synthetic private note".into()),
            },
            attachments: vec![Attachment {
                name: "../../untrusted.txt".into(),
                mime: "text/plain".into(),
                size: 4,
                sha256: hex::encode(Sha256::digest(b"test")),
            }],
        };
        let mut bytes = m.header().unwrap();
        bytes.extend_from_slice(b"test");
        let p = parse(&bytes).unwrap();
        assert_eq!(p.attachments[0], b"test");
        assert!(
            !p.manifest
                .preview()
                .to_string()
                .contains("synthetic private note")
        );
        for n in 0..bytes.len() {
            assert!(parse(&bytes[..n]).is_err());
        }
        let mut extra = bytes.to_vec();
        extra.push(0);
        assert!(parse(&extra).is_err());
        let mut corrupt = bytes.to_vec();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(parse(&corrupt).is_err());
        let mut bad = m.clone();
        bad.format = "unknown".into();
        assert!(bad.header().is_err());
        bad = m.clone();
        bad.attachments[0].size = u64::MAX;
        assert!(bad.header().is_err());
        assert_eq!(m.new_item(&[], false).unwrap().notes, "");
    }
}
