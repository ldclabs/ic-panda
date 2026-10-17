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
//! share neither a key nor a byte with it.

use super::Client;
use crate::codec::{canonical, unb64};
use crate::content::empty_file;
use crate::crypto::{Key, open, seal, seal_bytes};
use crate::files::{BlobStore, FileManifest, MAX_FILE, Source};
use crate::upload::{ChunkRef, UploadPlan, hex_id, random_hex};
use dmsg_types::{DAY, Error, Hash, MINUTE, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zeroize::Zeroizing;

/// The sealed records of the links this device made.
pub(crate) const LINK: &str = "lib-link:";
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
/// An unfinished link upload lapses after this.
const PLAN_MS: u64 = DAY;

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
    let origin = login_url
        .find("://")
        .and_then(|scheme| {
            login_url[scheme + 3..]
                .find('/')
                .map(|end| &login_url[..scheme + 3 + end])
        })
        .unwrap_or(login_url)
        .trim_end_matches('/');
    match key {
        Some(key) => format!("{origin}/s/{share_id}#{key}"),
        None => format!("{origin}/s/{share_id}"),
    }
}

/// A link this device made.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct LinkRecord {
    pub(crate) file: String,
    pub(crate) version: String,
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) share_id: String,
    pub(crate) upload_id: String,
    /// The edition's key, base64url.
    pub(crate) key: String,
    pub(crate) expires_at: u64,
    pub(crate) max_downloads: u64,
    pub(crate) password: bool,
    pub(crate) created_at: u64,
    pub(crate) revoked_at: Option<u64>,
    /// As the relay last reported.
    pub(crate) downloads: u64,
}

/// A link as it leaves the daemon; `url`, `file`, `version`, `name` and
/// `size` are known only on the device that made it.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct LinkView {
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
    /// Unix milliseconds.
    pub created_at: u64,
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
    fn link_records(&self) -> Result<Vec<LinkRecord>> {
        let mut records = Vec::new();
        for key in self.workspace.store.sealed_keys(LINK)? {
            if let Some(record) = self.workspace.private::<LinkRecord>(self.keys()?, &key)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    fn save_link(&self, record: &LinkRecord) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{LINK}{}", record.share_id), record)
    }

    fn link_view(&self, record: &LinkRecord, now: u64) -> LinkView {
        LinkView {
            share_id: record.share_id.clone(),
            file: Some(record.file.clone()),
            version: Some(record.version.clone()),
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
            created_at: record.created_at,
            revoked_at: record.revoked_at,
            state: state(record.expires_at, record.revoked_at, now),
            downloads: record.downloads,
        }
    }

    /// Share a version by link: upload a fresh edition of it as a `share`
    /// object and have the relay publish the link, good for `expires_in_ms`
    /// (the default week, at most 30 days) and `max_downloads` starts (0 for
    /// no limit). With a password the link carries no key; the key travels
    /// wrapped under the password, and the person needs both.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file or version,
    /// `Error::QuotaExceeded` for a version over 100 MiB or a cloud without
    /// room, `Error::RekeyRequired` while the account waits for a new root
    /// and `Error::InvalidInput` for a bad expiry or an empty password.
    pub async fn files_share_link(
        &mut self,
        id: &str,
        version: Option<&str>,
        expires_in_ms: Option<u64>,
        max_downloads: u64,
        password: Option<&str>,
        progress: &mut (dyn FnMut(u64, u64) + Send),
    ) -> Result<LinkView> {
        let expires_in = expires_in_ms.unwrap_or(LINK_DEFAULT_MS);
        ensure_valid((1..=LINK_MAX_MS).contains(&expires_in), "expires_in_ms")?;
        ensure_valid(max_downloads <= 1_000_000, "max_downloads")?;
        let (_, record, manifest) = self.library_manifest(id, version)?;
        ensure(record.local, Error::RecoveryIncomplete)?;
        ensure(manifest.size <= MAX_FILE, Error::QuotaExceeded)?;
        let security = self.cloud_state().await?;
        ensure(
            security.snapshot.vault_write_state == dmsg_types::user::VaultWriteState::Ready,
            Error::RekeyRequired,
        )?;
        let edition = self.share_edition(&manifest)?;
        let key = Zeroizing::new(
            <[u8; 32]>::try_from(Zeroizing::new(unb64(&edition.key)?).as_slice())
                .map_err(|_| Error::IntegrityFailed)?,
        );
        let share_id = edition.id.clone();
        let key_wrap = password
            .map(|password| wrap_key(&key, password, &share_id))
            .transpose()?;
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
        let upload_id = random_hex();
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
            upload_id: upload_id.clone(),
            object_id: edition.id.clone(),
            version_id: edition.version.clone(),
            root_generation: 0,
            epoch: 0,
            control_head: None,
            kind: SHARE.into(),
            chunks,
            manifest_digest: hex::encode(*crate::codec::sha256(&manifest_bytes)),
            manifest_size: manifest_bytes.len() as u64,
            expires_at: now + PLAN_MS,
        };
        let total = plan.chunks.len() as u64;
        let base = self.base()?;
        let uploaded = self
            .upload(
                &base,
                &plan,
                &manifest_bytes,
                0,
                Source::Library,
                |_, sent| {
                    progress(sent as u64, total);
                    Ok(())
                },
            )
            .await;
        if let Err(error) = uploaded {
            // The edition's chunks are nobody's now; the collector takes them.
            let _ = self.cancel_upload(&upload_id).await;
            return Err(error);
        }
        // The relay bounds the expiry by its own clock; a minute's margin
        // keeps the longest link within it.
        let now = crate::now_ms();
        let expires_at = now + expires_in.min(LINK_MAX_MS - MINUTE);
        let published = match self
            .relay_post(
                &format!("{base}/shares"),
                "dmsg/share/create/v1",
                &json!({
                    "share_id": share_id,
                    "upload_id": upload_id,
                    "expires_at": expires_at,
                    "max_downloads": max_downloads,
                    "key_wrap": key_wrap,
                }),
                &Hash::new(crate::crypto::random_id()),
            )
            .await
        {
            Ok(published) => published,
            Err(error) => {
                // Without its link nothing would retire the committed edition.
                let _ = self
                    .relay_post(
                        &format!("{base}/objects/collect"),
                        "dmsg/object/collect/v1",
                        &json!({"upload_id": upload_id, "generation": 0}),
                        &Hash::new(crate::crypto::random_id()),
                    )
                    .await;
                return Err(error);
            }
        };
        ensure(
            published["share_id"] == share_id.as_str(),
            Error::IntegrityFailed,
        )?;
        let link = LinkRecord {
            file: id.into(),
            version: manifest.version.clone(),
            name: manifest.name.clone(),
            size: manifest.size,
            share_id,
            upload_id,
            key: edition.key,
            expires_at,
            max_downloads,
            password: password.is_some(),
            created_at: published["created_at"].as_u64().unwrap_or(now),
            revoked_at: None,
            downloads: 0,
        };
        self.save_link(&link)?;
        Ok(self.link_view(&link, crate::now_ms()))
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

    /// Read the account's links from the relay: this device's records take
    /// the download counts and revocations; links other devices made are
    /// listed without a URL, since their keys never left those devices.
    ///
    /// # Errors
    /// Relay errors as returned.
    pub async fn files_share_links_refresh(&mut self) -> Result<Vec<LinkView>> {
        let base = self.base()?;
        let mut remote = Vec::new();
        let mut after = 0;
        loop {
            let page: RemotePage = serde_json::from_value(
                self.relay_get(&format!("{base}/shares?after={after}&limit=100"))
                    .await?,
            )
            .map_err(|_| Error::IntegrityFailed)?;
            remote.extend(page.entries);
            if page.end {
                break;
            }
            after = page.next_cursor;
        }
        let now = crate::now_ms();
        let mut views = Vec::with_capacity(remote.len());
        let mut records = self.link_records()?;
        for entry in remote {
            match records.iter_mut().find(|r| r.share_id == entry.share_id) {
                Some(record) => {
                    record.downloads = entry.downloads;
                    record.revoked_at = entry.revoked_at;
                    self.save_link(record)?;
                    views.push(self.link_view(record, now));
                }
                None => views.push(LinkView {
                    share_id: entry.share_id,
                    file: None,
                    version: None,
                    name: None,
                    size: None,
                    url: None,
                    expires_at: entry.expires_at,
                    max_downloads: entry.max_downloads,
                    password: entry.password,
                    created_at: entry.created_at,
                    revoked_at: entry.revoked_at,
                    state: state(entry.expires_at, entry.revoked_at, now),
                    downloads: entry.downloads,
                }),
            }
        }
        views.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        Ok(views)
    }

    /// Revoke a link: the relay retires its edition at once, and the link
    /// answers nothing from then on. The library's own copy stays.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a link the relay does not know.
    pub async fn files_share_revoke(&mut self, share_id: &str) -> Result<LinkView> {
        hex_id(share_id).map_err(|_| dmsg_types::invalid("share_id"))?;
        let base = self.base()?;
        let revoked = self
            .relay_post(
                &format!("{base}/shares/revoke"),
                "dmsg/share/revoke/v1",
                &json!({ "share_id": share_id }),
                &Hash::new(crate::crypto::random_id()),
            )
            .await?;
        let now = crate::now_ms();
        let revoked_at = revoked["revoked_at"].as_u64().or(Some(now));
        if let Some(mut record) = self
            .link_records()?
            .into_iter()
            .find(|r| r.share_id == share_id)
        {
            // The relay's answer counts no downloads; the record keeps the
            // last count it reported.
            record.revoked_at = revoked_at;
            self.save_link(&record)?;
            return Ok(self.link_view(&record, now));
        }
        Ok(LinkView {
            share_id: share_id.into(),
            file: None,
            version: None,
            name: None,
            size: None,
            url: None,
            expires_at: revoked["expires_at"].as_u64().unwrap_or(0),
            max_downloads: revoked["max_downloads"].as_u64().unwrap_or(0),
            password: revoked["password"].as_bool().unwrap_or(false),
            created_at: revoked["created_at"].as_u64().unwrap_or(0),
            revoked_at,
            state: "revoked",
            downloads: revoked["downloads"].as_u64().unwrap_or(0),
        })
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
