//! Share links: a version handed to someone outside dMsg. A shared edition
//! ([`Client::share_edition`]: the same bytes under a fresh key and a fresh
//! identity) goes to the relay as a `share` upload whose manifest is sealed
//! under the edition's key, never a root; the relay then publishes the link
//! on a signed command with its expiry and download limit. The link
//! `<site>/s/<share_id>#<key>` carries the key in its fragment, which never
//! reaches a server; with a password the fragment is empty and the key
//! travels wrapped under an Argon2id key of the password, salted with the
//! share ID. The browser page decrypts by the chunk rules of
//! `content-v1.json`. Revoking retires the edition on the relay; nothing of
//! it is kept here but the record, so the library's own copy and any backup
//! share neither a key nor a byte with it. That holds for a version only in
//! the cloud too: its backup comes down just long enough to be re-encrypted,
//! and the library gains nothing.

use super::Client;
use crate::codec::canonical;
use crate::content::empty_file;
use crate::crypto::{Key, open, seal, seal_bytes};
use crate::files::{BlobStore, FileManifest, MAX_FILE, Source};
use crate::hold::Hold;
use crate::upload::{ChunkRef, UploadPlan, hex_id, random_hex};
use dmsg_types::{DAY, Error, Hash, MINUTE, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeroize::Zeroizing;

/// The sealed records of the links this device made.
pub(crate) const LINK: &str = "lib-link:";
/// Sealed publication journals of feedback screenshots, by operation.
pub(crate) const FEEDBACK_SHARE: &str = "feedback-share:";
/// A link lasts this long unless told otherwise.
pub const LINK_DEFAULT_MS: u64 = 7 * DAY;
/// A link lasts at most this long; the relay enforces the same.
pub const LINK_MAX_MS: u64 = 30 * DAY;
/// The upload kind of a shared edition.
pub const SHARE: &str = "share";
/// The plaintext format of a share manifest.
pub const SHARE_FORMAT: &str = "dmsg-share-file/1";
/// Argon2id of the password key, OWASP's first recommendation: 19 MiB,
/// two passes, one lane; the browser page derives the same.
pub const PASSWORD_M_KIB: u32 = 19_456;
/// Argon2id passes.
pub const PASSWORD_T: u32 = 2;
/// Argon2id lanes.
pub const PASSWORD_P: u32 = 1;

/// What a new link allows.
#[derive(Clone, Copy, Debug, Default)]
pub struct LinkTerms<'a> {
    /// How long it lasts: a week by default, at most 30 days.
    pub expires_in_ms: Option<u64>,
    /// How many downloads may start; 0 for no limit.
    pub max_downloads: u64,
    /// With a password the link carries no key; the key travels wrapped
    /// under the password, and the person needs both.
    pub password: Option<&'a str>,
    /// Explicitly disclose the creating account through its public profile.
    pub show_profile: bool,
}

impl LinkTerms<'_> {
    /// How long the link lasts, once the terms are known to be good.
    fn lifetime(&self) -> Result<u64> {
        let lifetime = self.expires_in_ms.unwrap_or(LINK_DEFAULT_MS);
        ensure_valid((1..=LINK_MAX_MS).contains(&lifetime), "expires_in_ms")?;
        ensure_valid(self.max_downloads <= 1_000_000, "max_downloads")?;
        Ok(lifetime)
    }
}

/// The stage of [`share_link`]'s progress while a backup comes down.
pub const LINK_DOWNLOADING: &str = "downloading";
/// The stage of [`share_link`]'s progress while the edition goes up.
pub const LINK_UPLOADING: &str = "uploading";

/// What a share manifest carries, sealed under the edition's key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShareFile {
    /// [`SHARE_FORMAT`].
    pub format: String,
    /// The share ID: the edition's file ID.
    pub share_id: String,
    /// The edition's manifest without its key: name, type, size, SHA-256
    /// and chunks.
    pub file: FileManifest,
}

/// The AAD of a share manifest.
pub fn share_manifest_aad<'a>(
    share_id: &'a str,
    upload_id: &'a str,
    object_id: &'a str,
    version_id: &'a str,
) -> (&'static str, &'a str, &'a str, &'a str, &'a str) {
    (
        "dmsg/share-manifest/1",
        share_id,
        upload_id,
        object_id,
        version_id,
    )
}

/// The AAD of a password-wrapped share key.
pub fn share_key_aad(share_id: &str) -> (&'static str, &str) {
    ("dmsg/share-key/1", share_id)
}

/// The key a password derives for a share: Argon2id over the password with
/// the share ID's bytes as the salt.
///
/// # Errors
/// Returns `Error::InvalidInput` for an empty password and
/// `Error::IntegrityFailed` for a share ID that is not a 32-byte hex ID.
pub fn password_key(password: &str, share_id: &str) -> Result<Zeroizing<Key>> {
    ensure_valid(!password.is_empty(), "password")?;
    let salt = hex_id(share_id)?;
    let params = argon2::Params::new(PASSWORD_M_KIB, PASSWORD_T, PASSWORD_P, Some(32))
        .map_err(|e| Error::Unavailable(format!("argon2: {e}")))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), &salt[..], &mut key[..])
        .map_err(|e| Error::Unavailable(format!("argon2: {e}")))?;
    Ok(key)
}

/// The share key sealed under the password key, base64url.
///
/// # Errors
/// As [`password_key`].
pub fn wrap_key(key: &Key, password: &str, share_id: &str) -> Result<String> {
    let derived = password_key(password, share_id)?;
    Ok(seal(&derived, &key[..], &share_key_aad(share_id)))
}

/// The share key out of its password wrap.
///
/// # Errors
/// A wrong password returns `Error::IntegrityFailed`.
pub fn unwrap_key(wrap: &str, password: &str, share_id: &str) -> Result<Zeroizing<Key>> {
    let derived = password_key(password, share_id)?;
    let plain = open(&derived, wrap, &share_key_aad(share_id))?;
    Ok(Zeroizing::new(
        <[u8; 32]>::try_from(plain.as_slice()).map_err(|_| Error::IntegrityFailed)?,
    ))
}

/// The link of a share: the site's `/s/<share_id>`, with the key in the
/// fragment unless a password carries it.
pub fn share_url(login_url: &str, share_id: &str, key: Option<&str>) -> String {
    let origin = site(login_url);
    match key {
        Some(key) => format!("{origin}/s/{share_id}#{key}"),
        None => format!("{origin}/s/{share_id}"),
    }
}

/// The site's origin, out of the login URL.
pub(super) fn site(login_url: &str) -> &str {
    login_url
        .find("://")
        .and_then(|scheme| {
            login_url[scheme + 3..]
                .find('/')
                .map(|end| &login_url[..scheme + 3 + end])
        })
        .unwrap_or(login_url)
        .trim_end_matches('/')
}

/// Encrypted management relationship; a snapshot never pretends to be a library file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LinkSource {
    /// An explicitly selected library version.
    FileVersion {
        /// Original library file.
        file: String,
        /// Exact immutable version.
        version: String,
    },
    /// A separately encrypted, immutable Vault snapshot.
    VaultSnapshot {
        /// Original Vault item, retained only in sealed owner management.
        item_id: String,
        /// Source revision fixed by the share approval.
        item_revision: String,
        /// Random independent snapshot identity.
        snapshot_id: String,
    },
}

/// A link this device made.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct LinkRecord {
    pub(crate) source: LinkSource,
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) share_id: String,
    pub(crate) upload_id: String,
    /// The edition's key, base64url.
    pub(crate) key: String,
    pub(crate) expires_at: u64,
    pub(crate) max_downloads: u64,
    pub(crate) password: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) show_profile: bool,
    pub(crate) created_at: u64,
    pub(crate) last_checked_at: Option<u64>,
    pub(crate) revoked_at: Option<u64>,
    /// As the relay last reported.
    pub(crate) downloads: u64,
}

impl LinkRecord {
    /// Anonymous commands retain their original shape; opt-in is signed explicitly.
    pub(super) fn create_command(&self, key_wrap: Option<&str>) -> Value {
        let mut command = json!({
            "share_id": self.share_id,
            "upload_id": self.upload_id,
            "expires_at": self.expires_at,
            "max_downloads": self.max_downloads,
            "key_wrap": key_wrap,
        });
        if self.show_profile {
            command["show_profile"] = true.into();
        }
        command
    }

    /// The sealed management intent that other devices restore and recover
    /// the link from. Released clients parse its record strictly, so the
    /// record keeps its anonymous shape and the profile choice travels beside it.
    pub(super) fn intent(&self, key_wrap: Option<&str>) -> Value {
        let record = LinkRecord {
            show_profile: false,
            ..self.clone()
        };
        let mut intent = json!({"kind": "link", "record": record, "key_wrap": key_wrap});
        if self.show_profile {
            intent["show_profile"] = true.into();
        }
        intent
    }

    /// The record of a sealed `link` or `secret_link` intent.
    pub(super) fn from_intent(intent: &Value) -> Result<Self> {
        let mut record = Self::from_value(intent["record"].clone())?;
        record.show_profile = intent["show_profile"] == true;
        Ok(record)
    }

    /// Read local records and decrypted cloud management with the same source
    /// rules. File links written before snapshots stored `file`/`version` at
    /// the top level; their keys, limits and observation history stay intact.
    pub(super) fn from_value(mut value: Value) -> Result<Self> {
        let fields = value.as_object_mut().ok_or(Error::IntegrityFailed)?;
        if !fields.contains_key("source") {
            let file = fields.remove("file").ok_or(Error::IntegrityFailed)?;
            let version = fields.remove("version").ok_or(Error::IntegrityFailed)?;
            fields.insert(
                "source".into(),
                json!({"type":"file_version", "file":file, "version":version}),
            );
        }
        serde_json::from_value(value).map_err(|_| Error::IntegrityFailed)
    }
}

/// A link as it leaves the daemon; `url`, `file`, `version`, `name` and
/// `size` are known only on the device that made it.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct LinkView {
    /// Source relationship when this device can decrypt its management record.
    pub source: Option<LinkSource>,
    /// Share ID.
    pub share_id: String,
    /// The library file.
    pub file: Option<String>,
    /// Its version.
    pub version: Option<String>,
    /// The file's name.
    pub name: Option<String>,
    /// The version's length.
    pub size: Option<u64>,
    /// The link, key included unless a password carries it.
    pub url: Option<String>,
    /// Unix milliseconds.
    pub expires_at: u64,
    /// 0 is unlimited.
    pub max_downloads: u64,
    /// Opens only with its password.
    pub password: bool,
    /// The creator explicitly chose to show their public profile.
    pub show_profile: bool,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Time this device last received cloud state; absent means never observed.
    pub last_checked_at: Option<u64>,
    /// When it was revoked.
    pub revoked_at: Option<u64>,
    /// `active`, `revoked` or `expired`.
    pub state: &'static str,
    /// Downloads started, as the relay last reported.
    pub downloads: u64,
}

fn state(expires_at: u64, revoked_at: Option<u64>, now: u64) -> &'static str {
    match revoked_at {
        Some(_) => "revoked",
        None if expires_at <= now => "expired",
        None => "active",
    }
}

/// A relay entry of `GET /shares`.
#[derive(Deserialize)]
struct RemoteLink {
    share_id: String,
    expires_at: u64,
    max_downloads: u64,
    password: bool,
    #[serde(default)]
    show_profile: bool,
    created_at: u64,
    revoked_at: Option<u64>,
    #[serde(default)]
    downloads: u64,
}

#[derive(Deserialize)]
struct RemotePage {
    entries: Vec<RemoteLink>,
    next_cursor: u64,
    end: bool,
}

impl Client {
    pub(crate) fn link_records(&self) -> Result<Vec<LinkRecord>> {
        let mut records = Vec::new();
        for key in self.workspace.store.sealed_keys(LINK)? {
            if let Some(value) = self.workspace.private::<Value>(self.keys()?, &key)? {
                records.push(LinkRecord::from_value(value)?);
            }
        }
        Ok(records)
    }

    pub(crate) fn save_link(&self, record: &LinkRecord) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{LINK}{}", record.share_id), record)
    }

    pub(crate) fn link_view(&self, record: &LinkRecord, now: u64) -> LinkView {
        LinkView {
            share_id: record.share_id.clone(),
            source: Some(record.source.clone()),
            file: match &record.source {
                LinkSource::FileVersion { file, .. } => Some(file.clone()),
                _ => None,
            },
            version: match &record.source {
                LinkSource::FileVersion { version, .. } => Some(version.clone()),
                _ => None,
            },
            name: Some(record.name.clone()),
            size: Some(record.size),
            url: Some(share_url(
                &self.config.login_url,
                &record.share_id,
                (!record.password).then_some(record.key.as_str()),
            )),
            expires_at: record.expires_at,
            max_downloads: record.max_downloads,
            password: record.password,
            show_profile: record.show_profile,
            created_at: record.created_at,
            last_checked_at: record.last_checked_at,
            revoked_at: record.revoked_at,
            state: if record.revoked_at.is_none()
                && record.expires_at > now
                && record.max_downloads > 0
                && record.downloads >= record.max_downloads
            {
                "exhausted"
            } else {
                state(record.expires_at, record.revoked_at, now)
            },
            downloads: record.downloads,
        }
    }

    /// The shared edition a link hands out, under a new upload of the
    /// account; nothing goes to the relay yet.
    pub(crate) fn share_edition_upload(
        &self,
        manifest: FileManifest,
        edition: FileManifest,
        key_wrap: Option<String>,
        upload_id: Option<String>,
    ) -> Result<Sharing> {
        let key = edition.file_key()?;
        let share_id = edition.id.clone();
        let blobs = self.blobs()?;
        let mut chunks = Vec::with_capacity(edition.chunks.len());
        for chunk in &edition.chunks {
            let size = blobs
                .size(&chunk.digest)?
                .ok_or(Error::RecoveryIncomplete)?;
            chunks.push(ChunkRef {
                digest: chunk.digest.clone(),
                size,
            });
        }
        if chunks.is_empty() {
            let empty = empty_file();
            let digest = hex::encode(*crate::codec::sha256(&empty));
            blobs.put_chunk(&digest, &empty)?;
            chunks.push(ChunkRef {
                digest,
                size: empty.len() as u64,
            });
        }
        let now = crate::now_ms();
        let upload_id = upload_id.unwrap_or_else(random_hex);
        let manifest_bytes = seal_bytes(
            &key,
            &canonical(&ShareFile {
                format: SHARE_FORMAT.into(),
                share_id: share_id.clone(),
                file: edition.public(),
            }),
            &share_manifest_aad(&share_id, &upload_id, &edition.id, &edition.version),
        );
        let plan = UploadPlan {
            management: None,
            upload_id,
            object_id: edition.id.clone(),
            version_id: edition.version.clone(),
            root_generation: 0,
            epoch: 0,
            control_head: None,
            kind: SHARE.into(),
            chunks,
            manifest_digest: hex::encode(*crate::codec::sha256(&manifest_bytes)),
            manifest_size: manifest_bytes.len() as u64,
            expires_at: crate::upload::expires_at(now),
        };
        Ok(Sharing {
            base: self.base()?,
            manifest,
            share_id,
            key_wrap,
            key: edition.key,
            plan,
            manifest_bytes,
        })
    }

    /// The links this device made, newest first, as last known.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_share_links(&mut self) -> Result<Vec<LinkView>> {
        let now = crate::now_ms();
        let mut views: Vec<LinkView> = self
            .link_records()?
            .iter()
            .map(|r| self.link_view(r, now))
            .collect();
        views.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        Ok(views)
    }
}

/// A shared edition on its way to a link.
pub(crate) struct Sharing {
    pub(crate) base: String,
    /// The library version shared.
    pub(crate) manifest: FileManifest,
    pub(crate) share_id: String,
    pub(crate) key_wrap: Option<String>,
    /// The edition's key, base64url.
    pub(crate) key: String,
    pub(crate) plan: UploadPlan,
    pub(crate) manifest_bytes: Vec<u8>,
}

/// Share a version by link: upload a fresh edition of it as a `share`
/// object and have the relay publish the link on `terms`. A version freed
/// from this device comes down from its backup first.
///
/// # Errors
/// Returns `Error::NotFound` for an unknown file or version,
/// `Error::RecoveryIncomplete` for a version with neither its chunks nor a
/// backup here, `Error::QuotaExceeded` for a version over 100 MiB or a cloud
/// without room, `Error::RekeyRequired` while the account waits for a new
/// root and `Error::InvalidInput` for a bad expiry or an empty password.
pub async fn share_link<H: Hold>(
    h: &H,
    id: &str,
    version: Option<&str>,
    terms: LinkTerms<'_>,
    progress: &mut (dyn FnMut(&'static str, u64, u64) + Send),
) -> Result<LinkView> {
    terms.lifetime()?;
    let (manifest, backup) = h
        .with(|c| {
            let (_, record, manifest) = c.library_manifest(id, version)?;
            ensure(manifest.size <= MAX_FILE, Error::QuotaExceeded)?;
            let backup = match record.local {
                true => None,
                false => Some(record.backup.ok_or(Error::RecoveryIncomplete)?.upload_id),
            };
            Ok((manifest, backup))
        })
        .await?;
    publish_link(h, manifest, backup.as_deref(), terms, progress, None).await
}

/// Share a backup by link, whether or not this device's library has it. The
/// library does not change.
///
/// # Errors
/// Returns `Error::NotFound` for a backup not in the inventory,
/// `Error::RecoveryIncomplete` for one whose root this device lacks, and
/// otherwise as [`share_link`].
pub async fn share_backup_link<H: Hold>(
    h: &H,
    upload_id: &str,
    terms: LinkTerms<'_>,
    progress: &mut (dyn FnMut(&'static str, u64, u64) + Send),
) -> Result<LinkView> {
    terms.lifetime()?;
    let manifest = h
        .with(|c| {
            let entry = c.cloud_entry(upload_id)?.ok_or(Error::NotFound)?;
            let manifest = entry.manifest.ok_or(Error::RecoveryIncomplete)?;
            ensure(manifest.file.size <= MAX_FILE, Error::QuotaExceeded)?;
            Ok(manifest.file)
        })
        .await?;
    publish_link(h, manifest, Some(upload_id), terms, progress, None).await
}

/// A feedback screenshot keeps its original upload and link across retries.
pub(crate) async fn feedback_link<H: Hold>(
    h: &H,
    file: &str,
    version: &str,
    operation: &str,
) -> Result<LinkView> {
    hex_id(operation)?;
    let key = format!("{FEEDBACK_SHARE}{operation}");
    if let Some(attempt) = h
        .with(|c| c.workspace.private::<LinkAttempt>(c.keys()?, &key))
        .await?
    {
        ensure(
            attempt.link.source
                == LinkSource::FileVersion {
                    file: file.into(),
                    version: version.into(),
                },
            Error::IdempotencyConflict,
        )?;
        return submit_link(h, attempt, Some(&key), &mut |_, _, _| {}).await;
    }
    let manifest = h
        .with(|c| Ok(c.library_manifest(file, Some(version))?.2))
        .await?;
    publish_link(
        h,
        manifest,
        None,
        LinkTerms {
            expires_in_ms: Some(LINK_MAX_MS),
            ..Default::default()
        },
        &mut |_, _, _| {},
        Some(&key),
    )
    .await
}

#[derive(Clone, Serialize, Deserialize)]
struct LinkAttempt {
    base: String,
    link: LinkRecord,
    key_wrap: Option<String>,
    plan: UploadPlan,
    manifest: Vec<u8>,
    publishing: bool,
    published: bool,
}

/// A fresh edition of `source`, read from its backup when this device does
/// not hold its chunks. What came down for this is collected at once, while
/// the edition's own chunks stay held.
async fn prepare_edition<H: Hold>(
    h: &H,
    source: &FileManifest,
    backup: Option<&str>,
    progress: &mut (dyn FnMut(&'static str, u64, u64) + Send),
) -> Result<FileManifest> {
    let pulled = match backup {
        Some(upload) => Some(
            super::cloud::pull_chunks(h, upload, &mut |done, total| {
                progress(LINK_DOWNLOADING, done, total)
            })
            .await?,
        ),
        None => None,
    };
    let edition = super::share::edition(h, source).await;
    if let Some(pulled) = pulled {
        h.with(|c| {
            c.release_chunks(&pulled)?;
            c.collect_blobs().map(|_| ())
        })
        .await?;
    }
    edition
}

async fn publish_link<H: Hold>(
    h: &H,
    manifest: FileManifest,
    backup: Option<&str>,
    terms: LinkTerms<'_>,
    progress: &mut (dyn FnMut(&'static str, u64, u64) + Send),
    operation: Option<&str>,
) -> Result<LinkView> {
    let expires_in = terms.lifetime()?;
    let (max_downloads, password) = (terms.max_downloads, terms.password);
    let (_, security) = h.signed(Client::fresh).await?;
    ensure(
        security.snapshot.vault_write_state == dmsg_types::user::VaultWriteState::Ready,
        Error::RekeyRequired,
    )?;
    let edition = prepare_edition(h, &manifest, backup, progress).await?;
    // Password derivation and full-file re-encryption do not hold the client.
    let key_wrap = password
        .map(|password| wrap_key(&*edition.file_key()?, password, &edition.id))
        .transpose()?;
    h.signed(Client::root_now).await?;
    let Sharing {
        base,
        manifest,
        share_id,
        key_wrap,
        key,
        mut plan,
        manifest_bytes,
    } = h
        .with(|c| c.share_edition_upload(manifest, edition, key_wrap, None))
        .await?;
    let upload_id = plan.upload_id.clone();
    let now = crate::now_ms();
    let expires_at = now + expires_in.min(LINK_MAX_MS - MINUTE);
    let link = LinkRecord {
        source: LinkSource::FileVersion {
            file: manifest.id.clone(),
            version: manifest.version.clone(),
        },
        name: manifest.name.clone(),
        size: manifest.size,
        share_id: share_id.clone(),
        upload_id: upload_id.clone(),
        key: key.clone(),
        expires_at,
        max_downloads,
        password: password.is_some(),
        show_profile: terms.show_profile,
        created_at: now,
        last_checked_at: None,
        revoked_at: None,
        downloads: 0,
    };
    plan.management = Some(h.with(|c| c.seal_management(&plan, &json!({
        "name":manifest.name,"source_file":manifest.id,"source_version":manifest.version,"plain_bytes":manifest.size,
        "intent":link.intent(key_wrap.as_deref())
    }))).await?);
    let attempt = LinkAttempt {
        base,
        link,
        key_wrap,
        plan,
        manifest: manifest_bytes,
        publishing: false,
        published: false,
    };
    h.with(|c| {
        c.workspace.store.atomic(|_| {
            c.workspace.put_private(
                c.keys()?,
                &format!("lib-share-job:{upload_id}"),
                &json!({"plan":attempt.plan,"manifest":crate::codec::b64(&attempt.manifest)}),
            )?;
            if let Some(key) = operation {
                c.workspace.put_private(c.keys()?, key, &attempt)?;
            }
            c.release_chunks(&share_id)
        })
    })
    .await?;
    submit_link(h, attempt, operation, progress).await
}

async fn submit_link<H: Hold>(
    h: &H,
    mut attempt: LinkAttempt,
    operation: Option<&str>,
    progress: &mut (dyn FnMut(&'static str, u64, u64) + Send),
) -> Result<LinkView> {
    if attempt.publishing
        && !attempt.published
        && let Some(remote) = share_links_remote(h)
            .await?
            .into_iter()
            .find(|l| l.share_id == attempt.link.share_id)
    {
        ensure(
            remote.expires_at == attempt.link.expires_at
                && remote.password == attempt.link.password
                && remote.max_downloads == attempt.link.max_downloads
                && remote.show_profile == attempt.link.show_profile,
            Error::IntegrityFailed,
        )?;
        attempt.link.created_at = remote.created_at;
        attempt.link.revoked_at = remote.revoked_at;
        attempt.link.downloads = remote.downloads;
        attempt.published = true;
        return finish_link(h, &attempt, operation).await;
    }
    if attempt.published {
        return h
            .with(|c| {
                let now = crate::now_ms();
                ensure(attempt.link.expires_at > now, Error::ResultExpired)?;
                let record = c
                    .link_records()?
                    .into_iter()
                    .find(|l| l.share_id == attempt.link.share_id)
                    .unwrap_or(attempt.link.clone());
                Ok(c.link_view(&record, now))
            })
            .await;
    }
    let total = attempt.plan.chunks.len() as u64;
    let uploaded = crate::upload::upload(
        h,
        &attempt.base,
        &attempt.plan,
        &attempt.manifest,
        0,
        Source::Library,
        |_, sent| {
            progress(LINK_UPLOADING, sent as u64, total);
            Ok(())
        },
    )
    .await;
    uploaded?;
    ensure(
        attempt.link.expires_at > crate::now_ms(),
        Error::ResultExpired,
    )?;
    if let Some(key) = operation {
        attempt.publishing = true;
        h.with(|c| c.workspace.put_private(c.keys()?, key, &attempt))
            .await?;
    }
    let published = match h
        .post(
            &format!("{}/shares", attempt.base),
            "dmsg/share/create/v1",
            &attempt.link.create_command(attempt.key_wrap.as_deref()),
            &Hash::new(crate::crypto::random_id()),
        )
        .await
    {
        Ok(published) => published,
        Err(error) => return Err(error),
    };
    ensure(
        published["share_id"] == attempt.link.share_id.as_str()
            && published["show_profile"].as_bool().unwrap_or(false) == attempt.link.show_profile,
        Error::IntegrityFailed,
    )?;
    attempt.link.created_at = published["created_at"]
        .as_u64()
        .unwrap_or(attempt.link.created_at);
    attempt.published = true;
    finish_link(h, &attempt, operation).await
}

async fn finish_link<H: Hold>(
    h: &H,
    attempt: &LinkAttempt,
    operation: Option<&str>,
) -> Result<LinkView> {
    h.with(|c| {
        c.workspace.store.atomic(|_| {
            c.save_link(&attempt.link)?;
            if let Some(key) = operation {
                c.workspace.put_private(c.keys()?, key, &attempt)?;
            }
            c.workspace
                .delete_private(&format!("lib-share-job:{}", attempt.plan.upload_id))
        })?;
        Ok(c.link_view(&attempt.link, crate::now_ms()))
    })
    .await
}

/// Read the account's links after opening their encrypted source records.
/// Links from other devices retain their URL when the required account root
/// is available, even if the local source was deleted. Undecryptable records
/// still expose remote status without inventing a URL.
///
/// # Errors
/// Relay errors as returned.
pub async fn share_links_refresh<H: Hold>(h: &H) -> Result<Vec<LinkView>> {
    super::storage::sync_metadata(h).await?;
    share_links_remote(h).await
}

pub(crate) async fn share_links_remote<H: Hold>(h: &H) -> Result<Vec<LinkView>> {
    let base = h.with(|c| c.base()).await?;
    let mut remote = Vec::new();
    let mut after = 0;
    loop {
        let page: RemotePage = serde_json::from_value(
            h.get(&format!("{base}/shares?after={after}&limit=100"))
                .await?,
        )
        .map_err(|_| Error::IntegrityFailed)?;
        remote.extend(page.entries);
        if page.end {
            break;
        }
        ensure(page.next_cursor > after, Error::IntegrityFailed)?;
        after = page.next_cursor;
    }
    h.with(|c| {
        let now = crate::now_ms();
        let mut views = Vec::with_capacity(remote.len());
        let mut records = c.link_records()?;
        for entry in remote {
            match records.iter_mut().find(|r| r.share_id == entry.share_id) {
                Some(record) => {
                    record.downloads = entry.downloads;
                    record.show_profile = entry.show_profile;
                    record.last_checked_at = Some(now);
                    record.revoked_at = entry.revoked_at;
                    c.save_link(record)?;
                    views.push(c.link_view(record, now));
                }
                None => views.push(LinkView {
                    share_id: entry.share_id,
                    source: None,
                    file: None,
                    version: None,
                    name: None,
                    size: None,
                    url: None,
                    expires_at: entry.expires_at,
                    max_downloads: entry.max_downloads,
                    password: entry.password,
                    show_profile: entry.show_profile,
                    created_at: entry.created_at,
                    last_checked_at: Some(now),
                    revoked_at: entry.revoked_at,
                    state: if entry.revoked_at.is_none()
                        && entry.expires_at > now
                        && entry.max_downloads > 0
                        && entry.downloads >= entry.max_downloads
                    {
                        "exhausted"
                    } else {
                        state(entry.expires_at, entry.revoked_at, now)
                    },
                    downloads: entry.downloads,
                }),
            }
        }
        views.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        Ok(views)
    })
    .await
}

/// Revoke a link: the relay retires its edition at once, and the link
/// answers nothing from then on. The library's own copy stays.
///
/// # Errors
/// Returns `Error::NotFound` for a link the relay does not know.
pub async fn share_revoke<H: Hold>(h: &H, share_id: &str) -> Result<LinkView> {
    hex_id(share_id).map_err(|_| dmsg_types::invalid("share_id"))?;
    let base = h.with(|c| c.base()).await?;
    let revoked = h
        .post(
            &format!("{base}/shares/revoke"),
            "dmsg/share/revoke/v1",
            &json!({ "share_id": share_id }),
            &Hash::new(crate::crypto::random_id()),
        )
        .await?;
    h.with(|c| {
        let now = crate::now_ms();
        let revoked_at = revoked["revoked_at"].as_u64().or(Some(now));
        if let Some(mut record) = c
            .link_records()?
            .into_iter()
            .find(|r| r.share_id == share_id)
        {
            // The relay's answer counts no downloads; the record keeps the
            // last count it reported.
            record.revoked_at = revoked_at;
            record.last_checked_at = Some(now);
            c.save_link(&record)?;
            return Ok(c.link_view(&record, now));
        }
        Ok(LinkView {
            share_id: share_id.into(),
            source: None,
            file: None,
            version: None,
            name: None,
            size: None,
            url: None,
            expires_at: revoked["expires_at"].as_u64().unwrap_or(0),
            max_downloads: revoked["max_downloads"].as_u64().unwrap_or(0),
            password: revoked["password"].as_bool().unwrap_or(false),
            show_profile: revoked["show_profile"].as_bool().unwrap_or(false),
            created_at: revoked["created_at"].as_u64().unwrap_or(0),
            last_checked_at: Some(now),
            revoked_at,
            state: "revoked",
            downloads: revoked["downloads"].as_u64().unwrap_or(0),
        })
    })
    .await
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
