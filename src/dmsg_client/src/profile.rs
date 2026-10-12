//! The public profile (`dmsg/profile/v1`, `docs/protocol/cloud_zh.md`):
//! only the fields the owner explicitly publishes, signed by one of the
//! account's devices. A reader checks the signature against the account's
//! certified devices; what the profile says stays self-asserted, and the
//! relay may serve an older version than the latest.
//!
//! Publishing is a presigned command kept as a job until the relay answers,
//! so a lost reply is reconciled by its request ID, never signed twice.

use crate::account::{user_homes, verify_security, Security};
use crate::client::Client;
use crate::cloud::{verify_command, Presigned};
use crate::codec::{decode, sha256, unb64};
use crate::now_ms;
use dmsg_types::{
    ensure, ensure_valid,
    user::{Device, SecuritySnapshot},
    AccountId, CertifiedBatch, Error, Hash, Result,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::collections::BTreeMap;

const JOB: &str = "profile";
const HEAD: &str = "profile-head";

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

/// Check a relay profile record against the account's certified devices.
fn verify_profile(account: &str, value: &Json, security: &Security) -> Result<PublishedProfile> {
    let cose = value["signed"]["cose_sign1"]
        .as_str()
        .ok_or(Error::IntegrityFailed)?;
    let profile: Profile = verify_command(
        cose,
        &security.snapshot.issuer,
        &security.devices,
        "dmsg/profile/v1",
    )?;
    let hash = hex::encode(*sha256(&unb64(cose)?));
    ensure(
        serde_json::to_value(&profile).ok().as_ref() == Some(&value["payload"])
            && value["version"].as_u64() == Some(profile.version)
            && value["hash"] == hash.as_str(),
        Error::IntegrityFailed,
    )?;
    let device_id = value["signer"]["device_id"]
        .as_str()
        .ok_or(Error::IntegrityFailed)?;
    let device = crate::upload::hex_id(device_id)?;
    ensure(
        security.devices.contains_key(&device),
        Error::IntegrityFailed,
    )?;
    Ok(PublishedProfile {
        account: account.into(),
        profile,
        hash,
        device_id: device_id.into(),
        self_asserted: true,
    })
}

impl Client {
    /// This account's published profile, checked against its devices and
    /// against the newest version this device has seen.
    ///
    /// # Errors
    /// A relay that loses or rolls back a profile this device saw returns
    /// `Error::IntegrityFailed`.
    pub async fn profile(&mut self) -> Result<Option<PublishedProfile>> {
        let account = self.account()?.id.to_string();
        let value = self
            .relay_find(&format!("{}/profile", self.base()?))
            .await?;
        let known: Option<(u64, String)> = self.workspace.store.meta(HEAD)?;
        let Some(value) = value else {
            ensure(known.is_none(), Error::IntegrityFailed)?;
            return Ok(None);
        };
        let security = self.cloud_state().await?;
        let published = verify_profile(&account, &value, &security)?;
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
        self.keys()?;
        if let Some(job) = self.workspace.store.job::<ProfileJob>(JOB)? {
            // Whatever became of it, the next version builds on the relay's.
            let _ = self.send_profile(job).await;
            self.workspace.store.delete_job(JOB)?;
        }
        let current = self.profile().await?;
        let profile = Profile {
            version: current.as_ref().map_or(0, |c| c.profile.version) + 1,
            prev_hash: current.as_ref().map(|c| c.hash.clone()),
            display_name: fields.display_name.trim().into(),
            bio: fields.bio.clone(),
            avatar_upload: current.and_then(|c| c.profile.avatar_upload),
            links: fields.links.clone(),
        };
        profile.validate()?;
        let job = ProfileJob {
            profile,
            request_id: Hash::new(crate::crypto::random_id()),
            dispatch: None,
        };
        self.workspace.store.set_job(JOB, &job)?;
        let version = job.profile.version;
        let sent = self.send_profile(job).await;
        if !matches!(sent, Err(Error::ExecutionUnknown)) {
            self.workspace.store.delete_job(JOB)?;
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
                self.workspace.store.set_job(JOB, &job)?;
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
        let ic = self.reader().await?;
        let homes = user_homes(&ic, self.config.handle, &self.config.user_homes).await?;
        let mut security = None;
        for home in homes.user_homes {
            let bundle: Result<(SecuritySnapshot, BTreeMap<Hash, Device>)> =
                ic.query(home, "get_device_bundle", (account,)).await?;
            let Ok((_, devices)) = bundle else {
                continue;
            };
            let batch: Result<CertifiedBatch> = ic
                .query(home, "security_snapshot_batch", (vec![*account],))
                .await?;
            security = Some(verify_security(
                home,
                account,
                &batch?,
                devices,
                &ic.root_key(),
                now_ms(),
            )?);
            break;
        }
        let security = security.ok_or(Error::NotFound)?;
        let value = self
            .cloud
            .public(&format!("/public/accounts/{account}/profile"))
            .await?;
        value
            .map(|value| verify_profile(&account.to_string(), &value, &security))
            .transpose()
    }
}
