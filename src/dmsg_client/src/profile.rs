//! The public profile (`dmsg/profile/v1`, `docs/protocol/cloud_zh.md`):
//! only the fields the owner explicitly publishes, signed by one of the
//! account's devices. Another account checks the signature against the
//! account's certified devices; the owner checks it against the certified
//! evidence the relay kept when it accepted the profile, so its own profile
//! still verifies after recovery or the device bound replaced the signer.
//! What the profile says stays self-asserted, and the relay may serve an
//! older version than the latest.
//!
//! Publishing is a presigned command kept as a job until the relay answers,
//! so a lost reply is reconciled by its request ID, never signed twice.

use crate::account::Security;
use crate::client::Client;
use crate::cloud::{Evidence, Presigned, verify_signed_command};
use crate::codec::{canonical, decode, sha256, unb64};
use crate::evidence::under;
use crate::now_ms;
use crate::upload::{ChunkRef, UploadPlan, random_hex};
use dmsg_types::{AccountId, Error, Hash, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// The pending publish, in the metadata: a presigned command holds no secret.
const JOB: &str = "profile-job";
const HEAD: &str = "profile-head";
const ACTION: &str = "dmsg/profile/v1";

/// A published profile as the device signs it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// 1 for the first version, then one more each time.
    pub version: u64,
    /// Hash of the version this one replaces.
    pub prev_hash: Option<String>,
    /// Display name, at most 80 characters.
    pub display_name: String,
    /// Bio, at most 2000 characters.
    pub bio: String,
    /// A public avatar upload.
    pub avatar_upload: Option<String>,
    /// Up to ten HTTPS links.
    pub links: Vec<String>,
}

impl Profile {
    /// Check the relay's limits.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` naming the field.
    pub fn validate(&self) -> Result<()> {
        let control =
            |s: &str, newline: bool| s.chars().any(|c| c.is_control() && !(newline && c == '\n'));
        ensure_valid(
            self.display_name.chars().count() <= 80 && !control(&self.display_name, false),
            "display_name",
        )?;
        ensure_valid(
            self.bio.chars().count() <= 2000 && !control(&self.bio, true),
            "bio",
        )?;
        ensure_valid(
            self.links.len() <= 10
                && self.links.iter().all(|l| {
                    l.len() <= 2048 && url::Url::parse(l).is_ok_and(|u| u.scheme() == "https")
                }),
            "links",
        )
    }
}

/// What to publish; anything left out is not published.
#[derive(Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileFields {
    /// Display name.
    #[serde(default)]
    pub display_name: String,
    /// Bio.
    #[serde(default)]
    pub bio: String,
    /// HTTPS links.
    #[serde(default)]
    pub links: Vec<String>,
}

/// A profile and the checks it passed.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct PublishedProfile {
    /// The account.
    pub account: String,
    /// The signed profile.
    pub profile: Profile,
    /// SHA-256 of the signed command, hex.
    pub hash: String,
    /// The certified device that signed it.
    pub device_id: String,
    /// Always true: the account says this about itself.
    pub self_asserted: bool,
}

/// Largest public avatar: the relay's limit.
pub const MAX_AVATAR: usize = 512 * 1024;

/// The media type of avatar bytes: PNG, JPEG or WebP of 12 bytes to
/// 512 KiB, which the relay checks the same way.
///
/// # Errors
/// Returns `Error::InvalidInput` naming `avatar` for anything else.
pub fn avatar_type(bytes: &[u8]) -> Result<&'static str> {
    ensure_valid((12..=MAX_AVATAR).contains(&bytes.len()), "avatar")?;
    if bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        Ok("image/png")
    } else if bytes.starts_with(&[255, 216, 255]) {
        Ok("image/jpeg")
    } else if bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Ok("image/webp")
    } else {
        Err(dmsg_types::invalid("avatar"))
    }
}

/// The manifest of a public avatar upload; the relay stores it unread.
#[derive(Serialize)]
struct AvatarManifest<'a> {
    format: &'static str,
    account_id: &'a str,
    upload_id: &'a str,
    mime: &'a str,
}

/// The body of a presigned command.
#[derive(Serialize, Deserialize)]
struct Body {
    cose_sign1: String,
}

#[derive(Serialize, Deserialize)]
struct ProfileJob {
    profile: Profile,
    request_id: Hash,
    dispatch: Option<Presigned>,
}

/// The signed command of a relay profile record.
fn signed(value: &Json) -> Result<&str> {
    value["signed"]["cose_sign1"]
        .as_str()
        .ok_or(Error::IntegrityFailed)
}

/// This account's profile record, checked against `security`, its certified
/// security when the relay accepted the record at `stored_at`: the signer
/// was an unrevoked content signer of the account then, whatever replaced
/// it since.
fn verify_own(
    account: &str,
    value: &Json,
    security: &Security,
    stored_at: u64,
) -> Result<PublishedProfile> {
    let cose = signed(value)?;
    let (device, body) =
        under::<Profile>(security, cose, Some(ACTION)).map_err(|_| Error::IntegrityFailed)?;
    ensure(body.deadline > stored_at, Error::IntegrityFailed)?;
    published(account, value, cose, &device, body.payload)
}

/// Another account's profile record, checked against its certified devices.
fn verify_public(account: &str, value: &Json, security: &Security) -> Result<PublishedProfile> {
    let cose = signed(value)?;
    let (device, profile) =
        verify_signed_command(cose, &security.snapshot.issuer, &security.devices, ACTION)?;
    published(account, value, cose, &device, profile)
}

/// A relay profile record whose command `cose`, signed by `device`, says
/// `profile`: its projection must be of that command.
fn published(
    account: &str,
    value: &Json,
    cose: &str,
    device: &Hash,
    profile: Profile,
) -> Result<PublishedProfile> {
    let hash = hex::encode(*sha256(&unb64(cose)?));
    let device_id = hex::encode(**device);
    ensure(
        serde_json::to_value(&profile).ok().as_ref() == Some(&value["payload"])
            && value["version"].as_u64() == Some(profile.version)
            && value["hash"] == hash.as_str()
            && value["signer"]["device_id"] == device_id.as_str(),
        Error::IntegrityFailed,
    )?;
    Ok(PublishedProfile {
        account: account.into(),
        profile,
        hash,
        device_id,
        self_asserted: true,
    })
}

impl Client {
    /// This account's published profile, checked against the account's
    /// certified devices when the relay accepted it and against the newest
    /// version this device has seen.
    ///
    /// # Errors
    /// A relay that loses or rolls back a profile this device saw returns
    /// `Error::IntegrityFailed`.
    pub async fn profile(&mut self) -> Result<Option<PublishedProfile>> {
        let account = self.account()?.id;
        let value = self
            .relay_find(&format!("{}/profile", self.base()?))
            .await?;
        let known: Option<(u64, String)> = self.workspace.store.meta(HEAD)?;
        let Some(value) = value else {
            ensure(known.is_none(), Error::IntegrityFailed)?;
            return Ok(None);
        };
        let evidence: Evidence = serde_json::from_value(value["evidence"].clone())
            .map_err(|_| Error::IntegrityFailed)?;
        let stored_at = value["stored_at"].as_u64().ok_or(Error::IntegrityFailed)?;
        let security = self
            .trust()
            .await?
            .account(&evidence, &account, stored_at, now_ms())
            .map_err(|_| Error::IntegrityFailed)?;
        let published = verify_own(&account.to_string(), &value, &security, stored_at)?;
        let version = published.profile.version;
        if let Some((seen, hash)) = &known {
            ensure(
                version > *seen || (version == *seen && published.hash == *hash),
                Error::IntegrityFailed,
            )?;
        }
        self.workspace
            .store
            .set_meta(HEAD, &(version, published.hash.clone()))?;
        Ok(Some(published))
    }

    /// Publish exactly `fields` as the next profile version; the avatar, if
    /// any, stays. An earlier publish still pending is settled first.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::VersionConflict`
    /// when another device published in between.
    pub async fn publish_profile(&mut self, fields: &ProfileFields) -> Result<PublishedProfile> {
        self.publish_next(|current| Profile {
            version: 0,
            prev_hash: None,
            display_name: fields.display_name.trim().into(),
            bio: fields.bio.clone(),
            avatar_upload: current.and_then(|c| c.avatar_upload.clone()),
            links: fields.links.clone(),
        })
        .await
    }

    /// Upload `image` (PNG, JPEG or WebP, at most 512 KiB) as a public
    /// avatar and publish the next profile version with it; `None` removes
    /// the avatar. The other fields stay. The relay serves the image to
    /// anyone, unencrypted; earlier versions keep naming their avatars.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for another format or size, and as
    /// [`Self::publish_profile`].
    pub async fn publish_avatar(&mut self, image: Option<&[u8]>) -> Result<PublishedProfile> {
        self.keys()?;
        let upload = match image {
            Some(bytes) => Some(self.upload_avatar(bytes).await?),
            None => None,
        };
        self.publish_next(|current| Profile {
            version: 0,
            prev_hash: None,
            display_name: current.map(|c| c.display_name.clone()).unwrap_or_default(),
            bio: current.map(|c| c.bio.clone()).unwrap_or_default(),
            avatar_upload: upload,
            links: current.map(|c| c.links.clone()).unwrap_or_default(),
        })
        .await
    }

    /// The avatar an account's profile names, as the relay serves it to
    /// anyone: its media type and bytes, `None` when gone.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for bytes that are not an avatar.
    pub async fn avatar(
        &self,
        account: &AccountId,
        upload: &str,
    ) -> Result<Option<(&'static str, Vec<u8>)>> {
        crate::upload::hex_id(upload)?;
        let bytes = self
            .cloud
            .public_bytes(&format!("/public/accounts/{account}/avatar/{upload}"))
            .await?;
        bytes
            .map(|b| {
                let mime = avatar_type(&b).map_err(|_| Error::IntegrityFailed)?;
                Ok((mime, b))
            })
            .transpose()
    }

    /// Upload an avatar; returns its upload ID.
    async fn upload_avatar(&mut self, bytes: &[u8]) -> Result<String> {
        let mime = avatar_type(bytes)?;
        let account = self.account()?.id.to_string();
        let upload_id = random_hex();
        let manifest = canonical(&AvatarManifest {
            format: "dmsg-cloud-avatar/1",
            account_id: &account,
            upload_id: &upload_id,
            mime,
        });
        let digest = hex::encode(*sha256(bytes));
        let plan = UploadPlan {
            upload_id: upload_id.clone(),
            object_id: random_hex(),
            version_id: random_hex(),
            root_generation: 0,
            epoch: 0,
            control_head: None,
            kind: "avatar".into(),
            chunks: vec![ChunkRef {
                digest: digest.clone(),
                size: bytes.len() as u64,
            }],
            manifest_digest: hex::encode(*sha256(&manifest)),
            manifest_size: manifest.len() as u64,
            expires_at: now_ms() + dmsg_types::DAY,
        };
        // Read from the store by the upload, then collected after it.
        self.workspace.store.put_chunk(&digest, bytes)?;
        self.upload(
            &self.base()?,
            &plan,
            &manifest,
            0,
            crate::files::Source::Store,
            |_, _| Ok(()),
        )
        .await?;
        Ok(upload_id)
    }

    /// Publish the next profile version, which `build` makes from the
    /// current one; an earlier publish still pending is settled first.
    async fn publish_next(
        &mut self,
        build: impl FnOnce(Option<&Profile>) -> Profile,
    ) -> Result<PublishedProfile> {
        self.keys()?;
        if let Some(job) = self.workspace.store.meta::<ProfileJob>(JOB)? {
            // Whatever became of it, the next version builds on the relay's.
            let _ = self.send_profile(job).await;
            self.workspace.store.delete_meta(JOB)?;
        }
        let current = self.profile().await?;
        let profile = Profile {
            version: current.as_ref().map_or(0, |c| c.profile.version) + 1,
            prev_hash: current.as_ref().map(|c| c.hash.clone()),
            ..build(current.as_ref().map(|c| &c.profile))
        };
        profile.validate()?;
        let job = ProfileJob {
            profile,
            request_id: Hash::new(crate::crypto::random_id()),
            dispatch: None,
        };
        self.workspace.store.set_meta(JOB, &job)?;
        let version = job.profile.version;
        let sent = self.send_profile(job).await;
        if !matches!(sent, Err(Error::ExecutionUnknown)) {
            self.workspace.store.delete_meta(JOB)?;
        }
        sent?;
        let published = self.profile().await?.ok_or(Error::IntegrityFailed)?;
        ensure(published.profile.version >= version, Error::IntegrityFailed)?;
        Ok(published)
    }

    /// Submit a profile job: its stored outcome, its presigned command
    /// before the deadline, or, once that lapsed unanswered, a new one.
    async fn send_profile(&mut self, mut job: ProfileJob) -> Result<()> {
        let base = self.base()?;
        let request = hex::encode(*job.request_id);
        let status = format!("{base}/operations/{request}");
        let fresh = job
            .dispatch
            .as_ref()
            .filter(|d| d.deadline > now_ms() + 5_000);
        let dispatch = match fresh {
            Some(dispatch) => dispatch.clone(),
            None => {
                if let Some(found) = self
                    .relay_find(&status)
                    .await?
                    .filter(|f| f["found"] == true)
                {
                    return ensure(
                        found["result"]["version"].as_u64() == Some(job.profile.version),
                        Error::VersionConflict,
                    );
                }
                let security = self.cloud_state().await?;
                let dispatch = self.cloud.presign(
                    &self.signer(&security)?,
                    &format!("{base}/profile"),
                    "dmsg/profile/v1",
                    &job.profile,
                    &job.request_id,
                    &status,
                    now_ms(),
                )?;
                job.dispatch = Some(dispatch.clone());
                self.workspace.store.set_meta(JOB, &job)?;
                dispatch
            }
        };
        let receipt = self.cloud.submit(&dispatch, now_ms()).await?;
        let body: Body = decode(&dispatch.body, 262_144)?;
        let hash = hex::encode(*sha256(&unb64(&body.cose_sign1)?));
        ensure(
            receipt["version"].as_u64() == Some(job.profile.version)
                && receipt["hash"] == hash.as_str(),
            Error::IntegrityFailed,
        )
    }

    /// Another account's published profile, checked against that account's
    /// certified devices; no login needed.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when no user home knows the account.
    pub async fn profile_resolve(
        &mut self,
        account: &AccountId,
    ) -> Result<Option<PublishedProfile>> {
        let (_, security) = self
            .read_peers(std::slice::from_ref(account))
            .await?
            .pop()
            .ok_or(Error::NotFound)?;
        let value = self
            .cloud
            .public(&format!("/public/accounts/{account}/profile"))
            .await?;
        value
            .map(|value| verify_public(&account.to_string(), &value, &security))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn own_profiles_verify_against_the_devices_that_signed_them() {
        use crate::testing::{account, command, device, security};
        use serde_json::json;

        let (a, signer) = device(1, None);
        let (b, other) = device(2, None);
        let profile = Profile {
            version: 1,
            prev_hash: None,
            display_name: "name".into(),
            bio: String::new(),
            avatar_upload: None,
            links: vec![],
        };
        let cose = command(1, 3, ACTION, &profile);
        let value = json!({
            "signed": { "cose_sign1": cose },
            "payload": profile,
            "version": 1,
            "hash": hex::encode(*sha256(&unb64(&cose).unwrap())),
            "signer": { "device_id": hex::encode(*a) },
        });
        let id = account().id.to_string();
        // Device A signed it under epoch 3; recovery later replaced A with B.
        let then = security(3, BTreeMap::from([(a, signer)]));
        let today = security(4, BTreeMap::from([(b, other)]));
        let published = verify_own(&id, &value, &then, 5).unwrap();
        assert_eq!(published.profile, profile);
        assert_eq!(published.device_id, hex::encode(*a));
        // Against today's devices, as another account checks it, it fails.
        assert_eq!(
            verify_public(&id, &value, &today).unwrap_err(),
            Error::IntegrityFailed
        );
        assert_eq!(
            verify_own(&id, &value, &today, 5).unwrap_err(),
            Error::IntegrityFailed
        );
        assert_eq!(verify_public(&id, &value, &then).unwrap(), published);
        // Stored past its deadline, or by a device revoked then, it fails.
        assert_eq!(
            verify_own(&id, &value, &then, 10).unwrap_err(),
            Error::IntegrityFailed
        );
        let revoked = security(3, BTreeMap::from([device(1, Some(2))]));
        assert_eq!(
            verify_own(&id, &value, &revoked, 5).unwrap_err(),
            Error::IntegrityFailed
        );
        // The projection must be of the signed command.
        for (field, wrong) in [
            ("/signer/device_id", json!(hex::encode(*b))),
            ("/payload/display_name", json!("other")),
            ("/version", json!(2)),
        ] {
            let mut edited = value.clone();
            *edited.pointer_mut(field).unwrap() = wrong;
            assert_eq!(
                verify_own(&id, &edited, &then, 5).unwrap_err(),
                Error::IntegrityFailed
            );
        }
    }

    #[test]
    fn avatars_are_png_jpeg_or_webp_up_to_512_kib() {
        let png = [&[137, 80, 78, 71, 13, 10, 26, 10][..], &[0; 8]].concat();
        assert_eq!(avatar_type(&png), Ok("image/png"));
        assert_eq!(
            avatar_type(&[255, 216, 255, 224, 0, 0, 0, 0, 0, 0, 0, 0]),
            Ok("image/jpeg")
        );
        assert_eq!(avatar_type(b"RIFF\0\0\0\0WEBPVP8 "), Ok("image/webp"));
        assert!(avatar_type(b"GIF89a\0\0\0\0\0\0").is_err());
        assert!(avatar_type(&png[..11]).is_err());
        let large = [&png[..], &vec![0; MAX_AVATAR - png.len() + 1]].concat();
        assert!(avatar_type(&large).is_err());
        assert!(avatar_type(&large[..MAX_AVATAR]).is_ok());
    }
}
