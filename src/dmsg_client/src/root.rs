//! Content roots (`docs/protocol/account_root_zh.md`): the RootBundle v2
//! bytes, the reserve → wrap → upload → commit sequence, and opening the
//! committed root with this device's own envelope.
//!
//! A bundle wraps a random root key (VRK) to every active device holding
//! `VaultUnlock` (HPKE), to the account's vetKD recovery identity (IBE) and,
//! from the second generation, carries the previous root sealed under the new
//! one. Readers authenticate the current bundle by the certified
//! `bundle_digest` and earlier ones by that chain.

use crate::client::Client;
use crate::codec::{b64, base64url, canonical, decode, digest, sha256, unb64};
use crate::crypto::{
    derive, hpke_open, hpke_seal, open, random_key, seal, sign, signing_public, Key,
};
use crate::now_ms;
use crate::upload::{hex_id, random_hex, ChunkRef, UploadPlan};
use dmsg_protocol::{root_bundle_digest, root_recipients_digest, verify};
use dmsg_types::{
    cose::KeyDescriptor,
    ensure,
    user::{AccountCommand, Capability, ContentRootRef},
    AccountId, Environment, Error, Hash, Result,
};
use ic_vetkeys::{DerivedPublicKey, IbeCiphertext, IbeIdentity, IbeSeed, VetKey};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::collections::BTreeMap;
use std::str::FromStr;
use zeroize::Zeroizing;

/// Suite of every committed root.
pub const ROOT_SUITE: &str = "dmsg-root-v2";
/// `body.format` of a bundle.
pub const ROOT_FORMAT: &str = "dmsg-root-bundle/2";
/// Largest bundle.
pub const MAX_BUNDLE: usize = 64_000;
/// Most bundles a previous-root chain may hold.
const MAX_CHAIN: usize = 256;
/// Most devices one bundle wraps to.
const MAX_RECIPIENTS: usize = 16;
const ROOT: &str = "root";
pub(crate) const ROOT_JOB: &str = "root-job";

/// A signed RootBundle v2.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RootBundle {
    /// The signed body.
    pub body: RootBody,
    /// Device Ed25519 signature over `digest("dmsg/root-bundle/2", body)`.
    pub signature: ByteArray<64>,
}

/// The body of a bundle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootBody {
    /// [`ROOT_FORMAT`].
    pub format: String,
    /// Account, environment, generation and reservation.
    pub context: RootContext,
    /// `hex(SHA-256(D(VRK, ["dmsg/root-commitment/1"])))`.
    pub commitment: String,
    /// One HPKE envelope per device, ascending by device ID.
    pub envelopes: Vec<DeviceEnvelope>,
    /// The COSE content-root key the recovery envelope is encrypted to.
    pub recovery_key: RecoveryKey,
    /// IBE ciphertext of the VRK to `CBOR([account, generation])`, base64url.
    pub recovery: String,
    /// The previous root, sealed under this one.
    pub previous: Option<PreviousRoot>,
    /// The signing device.
    pub device: String,
    /// The signing device's Ed25519 public key, base64url.
    pub signing_public: String,
}

/// The reservation a bundle belongs to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootContext {
    /// Account ID text.
    pub account: String,
    /// Lowercase environment name.
    pub environment: String,
    /// Root generation.
    pub generation: u64,
    /// Reservation operation ID, hex.
    pub op_id: String,
    /// Security epoch of the reservation.
    pub security_epoch: u64,
}

/// The VRK sealed to one device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeviceEnvelope {
    /// Device ID, hex.
    pub device: String,
    /// HPKE encapsulated key.
    #[serde(with = "base64url")]
    pub enc: Vec<u8>,
    /// HPKE ciphertext.
    #[serde(with = "base64url")]
    pub ciphertext: Vec<u8>,
}

/// A COSE executor's content-root vetKD public key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryKey {
    /// The executor, Principal text.
    pub home_cose: String,
    /// Master key name, such as `key_1`.
    pub key_name: String,
    /// The 96-byte derived public key, base64url.
    pub public_key: String,
}

/// Link to the previous generation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviousRoot {
    /// SHA-256 of the previous bundle bytes, hex.
    pub digest: String,
    /// Upload holding the previous bundle.
    pub upload_id: String,
    /// The previous generation.
    pub generation: u64,
    /// The previous VRK sealed under this one.
    pub envelope: String,
}

/// What `CommitRoot` binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BundleDigests {
    /// `digest("dmsg/root-recipients/1", [sorted devices, generation])`.
    pub recipients: Hash,
    /// SHA-256 of the canonical body.
    pub body: Hash,
    /// `digest("dmsg/root-bundle-digest/2", [recipients, body])`.
    pub bundle: Hash,
}

/// A device that receives the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recipient {
    /// Device ID.
    pub device: Hash,
    /// The device's X25519 public key.
    pub hpke_pub: Hash,
}

/// The previous generation a new bundle links to.
pub struct PreviousLink<'a> {
    /// SHA-256 of its bundle bytes, hex.
    pub digest: &'a str,
    /// Its upload.
    pub upload_id: &'a str,
    /// Its generation.
    pub generation: u64,
    /// Its VRK.
    pub root: &'a Key,
}

/// The device-root HPKE context.
fn device_context<'a>(
    context: &'a RootContext,
    device: &'a str,
) -> (&'static str, &'a str, &'a str, u64, &'a str) {
    (
        "dmsg/device-root/1",
        &context.environment,
        &context.account,
        context.generation,
        device,
    )
}

fn commitment(root: &Key) -> String {
    hex::encode(*sha256(
        derive(root, &("dmsg/root-commitment/1",)).as_slice(),
    ))
}

/// The vetKD IBE identity of one account generation.
pub fn recovery_identity(account: &AccountId, generation: u64) -> Vec<u8> {
    canonical(&(account, generation))
}

fn previous_context<'a>(
    account: &'a str,
    generation: u64,
    link: &'a PreviousRoot,
) -> (&'static str, &'a str, u64, u64, &'a str) {
    (
        "dmsg/previous-root/1",
        account,
        generation,
        link.generation,
        &link.digest,
    )
}

/// Build and sign a bundle. Envelopes are sorted by device ID.
///
/// # Errors
/// Returns `Error::InvalidInput` for an empty, oversized or duplicated
/// recipient set or one without the signer, and `Error::QuotaExceeded` for
/// a bundle above [`MAX_BUNDLE`].
pub fn wrap_root(
    context: &RootContext,
    root: &Key,
    recipients: &[Recipient],
    recovery_key: &RecoveryKey,
    signer: (&Hash, &Key),
    previous: Option<PreviousLink<'_>>,
) -> Result<Vec<u8>> {
    let mut recipients = recipients.to_vec();
    recipients.sort_by_key(|r| r.device);
    recipients.dedup_by_key(|r| r.device);
    ensure(
        !recipients.is_empty()
            && recipients.len() <= MAX_RECIPIENTS
            && recipients.iter().any(|r| r.device == *signer.0),
        dmsg_types::invalid("recipients"),
    )?;
    let account =
        AccountId::from_str(&context.account).map_err(|_| dmsg_types::invalid("account"))?;
    let public = DerivedPublicKey::deserialize(&unb64(&recovery_key.public_key)?)
        .map_err(|_| Error::IntegrityFailed)?;
    let mut envelopes = Vec::with_capacity(recipients.len());
    for recipient in &recipients {
        let device = hex::encode(*recipient.device);
        let sealed = hpke_seal(&recipient.hpke_pub, root, &device_context(context, &device))?;
        envelopes.push(DeviceEnvelope {
            device,
            enc: sealed.enc,
            ciphertext: sealed.ciphertext,
        });
    }
    let seed = IbeSeed::from_bytes(&random_key()).map_err(|_| Error::IntegrityFailed)?;
    let recovery = IbeCiphertext::encrypt(
        &public,
        &IbeIdentity::from_bytes(&recovery_identity(&account, context.generation)),
        root,
        &seed,
    );
    let previous = previous.map(|link| {
        let mut entry = PreviousRoot {
            digest: link.digest.into(),
            upload_id: link.upload_id.into(),
            generation: link.generation,
            envelope: String::new(),
        };
        entry.envelope = seal(
            root,
            link.root,
            &previous_context(&context.account, context.generation, &entry),
        );
        entry
    });
    let body = RootBody {
        format: ROOT_FORMAT.into(),
        context: context.clone(),
        commitment: commitment(root),
        envelopes,
        recovery_key: recovery_key.clone(),
        recovery: b64(&recovery.serialize()),
        previous,
        device: hex::encode(**signer.0),
        signing_public: b64(&signing_public(signer.1)),
    };
    let signature = sign(signer.1, digest("dmsg/root-bundle/2", &body).as_slice());
    let bytes = canonical(&RootBundle {
        body,
        signature: ByteArray::new(signature),
    });
    ensure(bytes.len() <= MAX_BUNDLE, Error::QuotaExceeded)?;
    Ok(bytes)
}

fn check_body(body: &RootBody) -> Result<()> {
    let context = &body.context;
    AccountId::from_str(&context.account).map_err(|_| Error::IntegrityFailed)?;
    crate::config::parse_environment(&context.environment).map_err(|_| Error::IntegrityFailed)?;
    hex_id(&context.op_id)?;
    ensure(
        body.format == ROOT_FORMAT
            && body.commitment.len() == 64
            && body
                .commitment
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            && context.generation > 0
            && !body.envelopes.is_empty()
            && body.envelopes.len() <= MAX_RECIPIENTS
            && body.envelopes.windows(2).all(|w| w[0].device < w[1].device)
            && body.envelopes.iter().any(|e| e.device == body.device)
            && body.recovery_key.home_cose.len() <= 63
            && (1..=64).contains(&body.recovery_key.key_name.len())
            && unb64(&body.recovery_key.public_key)?.len() == 96
            && body.recovery.len() <= 512
            && unb64(&body.recovery).is_ok(),
        Error::IntegrityFailed,
    )?;
    for envelope in &body.envelopes {
        hex_id(&envelope.device)?;
    }
    if let Some(link) = &body.previous {
        hex_id(&link.digest)?;
        hex_id(&link.upload_id)?;
        ensure(
            link.generation > 0 && link.generation < context.generation,
            Error::IntegrityFailed,
        )?;
    }
    Ok(())
}

/// Decode a bundle and check its structure and device signature. This does
/// not prove the bundle is the committed one; compare [`bundle_digests`].
///
/// # Errors
/// Returns `Error::IntegrityFailed` for anything malformed or unsigned.
pub fn parse_bundle(bytes: &[u8]) -> Result<RootBundle> {
    let bundle: RootBundle = decode(bytes, MAX_BUNDLE)?;
    check_body(&bundle.body)?;
    let public: [u8; 32] = unb64(&bundle.body.signing_public)?
        .try_into()
        .map_err(|_| Error::IntegrityFailed)?;
    verify(
        &Hash::new(public),
        digest("dmsg/root-bundle/2", &bundle.body).as_slice(),
        bundle.signature.as_slice(),
    )?;
    Ok(bundle)
}

/// The digests `CommitRoot` binds for a bundle.
///
/// # Errors
/// A malformed device ID returns `Error::IntegrityFailed`.
pub fn bundle_digests(bundle: &RootBundle) -> Result<BundleDigests> {
    let devices = bundle
        .body
        .envelopes
        .iter()
        .map(|e| hex_id(&e.device))
        .collect::<Result<Vec<_>>>()?;
    let recipients = root_recipients_digest(&devices, bundle.body.context.generation);
    let body = sha256(&canonical(&bundle.body));
    Ok(BundleDigests {
        recipients,
        body,
        bundle: root_bundle_digest(recipients, body),
    })
}

/// Earlier roots by generation.
pub type History = BTreeMap<u64, Zeroizing<Key>>;

/// An opened root and every earlier generation its chain reaches.
pub struct RootKeys {
    /// Current generation.
    pub generation: u64,
    /// Committed `bundle_digest`.
    pub digest: Hash,
    /// SHA-256 of the bundle bytes, hex: the next bundle's `previous.digest`.
    pub sha256: String,
    /// The upload holding the bundle.
    pub upload_id: String,
    root: Zeroizing<Key>,
    history: History,
}

impl RootKeys {
    /// One generation, for tests.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn single(generation: u64, root: Key) -> Self {
        Self {
            generation,
            digest: Hash::new([0; 32]),
            sha256: String::new(),
            upload_id: String::new(),
            root: Zeroizing::new(root),
            history: BTreeMap::new(),
        }
    }

    /// The VRK of `generation`, current or earlier.
    pub fn key(&self, generation: u64) -> Option<&Key> {
        if generation == self.generation {
            Some(&self.root)
        } else {
            self.history.get(&generation).map(|k| &**k)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RootRecord {
    generation: u64,
    digest: Hash,
    sha256: String,
    upload_id: String,
    root: ByteArray<32>,
    history: BTreeMap<u64, ByteArray<32>>,
}

/// Open the current bundle with this device's envelope and walk its
/// previous-root chain. `bundles[0]` is the current bundle, each next one
/// the bundle its predecessor links to.
///
/// # Errors
/// Returns `Error::DeviceNotApproved` when the bundle has no envelope for
/// this device and `Error::IntegrityFailed` for any broken link.
pub fn open_root(
    bundles: &[Vec<u8>],
    expected: &Hash,
    context: (&str, &str, u64),
    device: &Hash,
    hpke_seed: &Key,
) -> Result<(Zeroizing<Key>, History)> {
    let current = current_bundle(bundles, expected, context)?;
    let c = &current.body.context;
    let device = hex::encode(**device);
    let envelope = current
        .body
        .envelopes
        .iter()
        .find(|e| e.device == device)
        .ok_or(Error::DeviceNotApproved)?;
    let opened = hpke_open(
        hpke_seed,
        &crate::crypto::HpkeEnvelope {
            enc: envelope.enc.clone(),
            ciphertext: envelope.ciphertext.clone(),
        },
        &device_context(c, &device),
    )?;
    walk_chain(bundles, current, &opened)
}

/// Open the current bundle's recovery envelope with the vetKD key of its
/// generation and walk its previous-root chain, as [`open_root`].
///
/// # Errors
/// A key of another identity or generation returns `Error::IntegrityFailed`.
pub fn open_recovered(
    bundles: &[Vec<u8>],
    expected: &Hash,
    context: (&str, &str, u64),
    vetkey: &VetKey,
) -> Result<(Zeroizing<Key>, History)> {
    let current = current_bundle(bundles, expected, context)?;
    let ciphertext = IbeCiphertext::deserialize(&unb64(&current.body.recovery)?)
        .map_err(|_| Error::IntegrityFailed)?;
    let opened = Zeroizing::new(
        ciphertext
            .decrypt(vetkey)
            .map_err(|_| Error::IntegrityFailed)?,
    );
    walk_chain(bundles, current, &opened)
}

/// The current bundle, checked against the committed digest and context.
fn current_bundle(
    bundles: &[Vec<u8>],
    expected: &Hash,
    context: (&str, &str, u64),
) -> Result<RootBundle> {
    ensure(
        !bundles.is_empty() && bundles.len() <= MAX_CHAIN,
        Error::QuotaExceeded,
    )?;
    let current = parse_bundle(&bundles[0])?;
    let c = &current.body.context;
    ensure(
        bundle_digests(&current)?.bundle == *expected
            && (c.account.as_str(), c.environment.as_str(), c.generation) == context,
        Error::IntegrityFailed,
    )?;
    Ok(current)
}

/// Check the opened root against the current bundle's commitment, then
/// follow its previous-root links through `bundles[1..]`.
fn walk_chain(
    bundles: &[Vec<u8>],
    current: RootBundle,
    opened: &[u8],
) -> Result<(Zeroizing<Key>, History)> {
    let root: Key = opened.try_into().map_err(|_| Error::IntegrityFailed)?;
    let root = Zeroizing::new(root);
    ensure(
        commitment(&root) == current.body.commitment,
        Error::IntegrityFailed,
    )?;
    let c = current.body.context.clone();
    let mut history = BTreeMap::new();
    let mut cursor = current.body;
    let mut key = root.clone();
    for bytes in &bundles[1..] {
        let link = cursor.previous.clone().ok_or(Error::IntegrityFailed)?;
        // Earlier bundles are authenticated by the chain: the link names
        // their bytes and the sealed root must match their commitment.
        ensure(
            hex::encode(*sha256(bytes)) == link.digest,
            Error::IntegrityFailed,
        )?;
        let previous: RootBundle = decode(bytes, MAX_BUNDLE)?;
        check_body(&previous.body)?;
        ensure(
            previous.body.context.generation == link.generation
                && previous.body.context.account == c.account
                && previous.body.context.environment == c.environment,
            Error::IntegrityFailed,
        )?;
        let prior = open(
            &key,
            &link.envelope,
            &previous_context(&c.account, cursor.context.generation, &link),
        )?;
        let prior: Key = prior
            .as_slice()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        let prior = Zeroizing::new(prior);
        ensure(
            commitment(&prior) == previous.body.commitment,
            Error::IntegrityFailed,
        )?;
        history.insert(link.generation, prior.clone());
        key = prior;
        cursor = previous.body;
    }
    ensure(cursor.previous.is_none(), Error::RecoveryIncomplete)?;
    Ok((root, history))
}

#[derive(Serialize, Deserialize)]
struct RootJob {
    op_id: Hash,
    expected_generation: u64,
    context: Option<RootContext>,
    candidate: Option<ByteArray<32>>,
    bytes: Option<ByteBuf>,
    plan: Option<UploadPlan>,
}

#[derive(Deserialize)]
struct RootPointer {
    upload_id: String,
    digest: String,
    root_generation: u64,
}

impl Client {
    /// The content-root key of `generation`, checked against the account's
    /// COSE home, the deployment and the master key.
    ///
    /// # Errors
    /// Any mismatch returns `Error::IntegrityFailed`.
    pub async fn recovery_key(
        &self,
        account: &AccountId,
        generation: u64,
        home_cose: candid::Principal,
    ) -> Result<RecoveryKey> {
        let key: Result<KeyDescriptor> = self
            .session()?
            .ic
            .query(home_cose, "root_public_key", (account, generation))
            .await?;
        let key = key?;
        let names: &[&str] = if self.config.environment == Environment::Local {
            &["key_1", "test_key_1", "dfx_test_key"]
        } else {
            &["key_1"]
        };
        ensure(
            key.home_cose == home_cose
                && key.key_generation == generation
                && key.derivation_version == 2
                && key.account_id == *account
                && key.environment == self.config.environment
                && key.public_key.len() == 96
                && key.public_key_fingerprint == sha256(&key.public_key)
                && names.contains(&key.master_key_name.as_str())
                && DerivedPublicKey::deserialize(&key.public_key).is_ok(),
            Error::IntegrityFailed,
        )?;
        Ok(RecoveryKey {
            home_cose: home_cose.to_text(),
            key_name: key.master_key_name,
            public_key: b64(&key.public_key),
        })
    }

    /// The upload plan of a root rotation in flight.
    pub(crate) fn root_plan(&self) -> Result<Option<UploadPlan>> {
        Ok(self
            .workspace
            .private::<RootJob>(self.keys()?, ROOT_JOB)?
            .and_then(|job| job.plan))
    }

    pub(crate) fn save_root(&self, keys: &RootKeys) -> Result<()> {
        let record = RootRecord {
            generation: keys.generation,
            digest: keys.digest,
            sha256: keys.sha256.clone(),
            upload_id: keys.upload_id.clone(),
            root: ByteArray::new(*keys.root),
            history: keys
                .history
                .iter()
                .map(|(g, k)| (*g, ByteArray::new(**k)))
                .collect(),
        };
        self.workspace.put_private(self.keys()?, ROOT, &record)
    }

    /// Load the root this device holds, and fetch the committed one when
    /// the account has moved past it.
    ///
    /// # Errors
    /// Returns `Error::DeviceNotApproved` when the committed bundle is not
    /// wrapped to this device.
    pub(crate) async fn load_root(&mut self) -> Result<()> {
        if let Some(record) = self.workspace.private::<RootRecord>(self.keys()?, ROOT)? {
            self.root = Some(RootKeys {
                generation: record.generation,
                digest: record.digest,
                sha256: record.sha256,
                upload_id: record.upload_id,
                root: Zeroizing::new(*record.root),
                history: record
                    .history
                    .into_iter()
                    .map(|(g, k)| (g, Zeroizing::new(*k)))
                    .collect(),
            });
        }
        let state = self.cloud_state().await?;
        let snapshot = &state.snapshot;
        match snapshot.content_root_digest {
            Some(digest)
                if self.root.as_ref().map(|r| (r.generation, r.digest))
                    != Some((snapshot.content_root_generation, digest)) =>
            {
                self.open_current().await
            }
            _ => Ok(()),
        }
    }

    /// The committed bundle of `generation` and every bundle its chain
    /// links to, current first, with the upload holding the current one.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` for a chain the relay cannot serve.
    pub(crate) async fn root_bundles(&mut self, generation: u64) -> Result<(Vec<Vec<u8>>, String)> {
        let value = self.relay_get(&format!("{}/root", self.base()?)).await?;
        let pointer: RootPointer =
            serde_json::from_value(value["root"].clone()).map_err(|_| Error::IntegrityFailed)?;
        hex_id(&pointer.upload_id)?;
        ensure(
            pointer.root_generation == generation,
            Error::IntegrityFailed,
        )?;
        let mut bundles = vec![
            self.download(&pointer.upload_id, "manifest", &pointer.digest)
                .await?,
        ];
        let mut link = parse_bundle(&bundles[0])?.body.previous;
        while let Some(previous) = link {
            ensure(bundles.len() < MAX_CHAIN, Error::RecoveryIncomplete)?;
            let bytes = self
                .download(&previous.upload_id, "manifest", &previous.digest)
                .await?;
            link = decode::<RootBundle>(&bytes, MAX_BUNDLE)?.body.previous;
            bundles.push(bytes);
        }
        Ok((bundles, pointer.upload_id))
    }

    /// Download the committed bundle chain and open it with this device's envelope.
    ///
    /// # Errors
    /// As [`open_root`].
    pub async fn open_current(&mut self) -> Result<()> {
        let account = self.account()?.clone();
        let state = self.cloud_state().await?;
        let generation = state.snapshot.content_root_generation;
        let digest = state
            .snapshot
            .content_root_digest
            .ok_or(Error::RecoveryIncomplete)?;
        state.active(&self.workspace.meta.device_id).and_then(|d| {
            ensure(
                d.input.capabilities.contains(&Capability::VaultUnlock),
                Error::Forbidden,
            )
        })?;
        let (bundles, upload_id) = self.root_bundles(generation).await?;
        let environment = crate::config::environment_name(&self.config.environment);
        let (root, history) = open_root(
            &bundles,
            &digest,
            (&account.id.to_string(), environment, generation),
            &self.workspace.meta.device_id,
            &self.keys()?.hpke,
        )?;
        self.keep_root(generation, digest, &bundles[0], upload_id, root, history)
    }

    /// Keep an opened committed root as this device's root.
    pub(crate) fn keep_root(
        &mut self,
        generation: u64,
        digest: Hash,
        bundle: &[u8],
        upload_id: String,
        root: Zeroizing<Key>,
        history: History,
    ) -> Result<()> {
        let keys = RootKeys {
            generation,
            digest,
            sha256: hex::encode(*sha256(bundle)),
            upload_id,
            root,
            history,
        };
        self.save_root(&keys)?;
        self.root = Some(keys);
        Ok(())
    }

    /// Commit a new root generation wrapped to every active device holding
    /// `VaultUnlock`. Interrupted runs resume with the same reservation,
    /// root key and bundle bytes.
    ///
    /// # Errors
    /// Returns `Error::VersionConflict` when another device committed first
    /// and `Error::RecoveryIncomplete` when this device has not opened the
    /// current root yet.
    pub async fn rotate_root(&mut self) -> Result<u64> {
        let account = self.account()?.clone();
        let mut state = self.refresh(account.home, &account.id).await?;
        let current = state.info.current_root.clone();
        let mut job = match self.workspace.private::<RootJob>(self.keys()?, ROOT_JOB)? {
            Some(job) => job,
            None => {
                ensure(
                    current.as_ref().map(|c| (c.generation, c.bundle_digest))
                        == self.root.as_ref().map(|r| (r.generation, r.digest)),
                    Error::RecoveryIncomplete,
                )?;
                RootJob {
                    op_id: Hash::new(crate::crypto::random_id()),
                    expected_generation: current.as_ref().map_or(0, |c| c.generation),
                    context: None,
                    candidate: None,
                    bytes: None,
                    plan: None,
                }
            }
        };
        let save = |client: &Self, job: &RootJob| {
            client.workspace.put_private(client.keys()?, ROOT_JOB, job)
        };
        save(self, &job)?;
        if let Some(bytes) = &job.bytes {
            let digests = bundle_digests(&parse_bundle(bytes)?)?;
            if current.as_ref().map(|c| c.bundle_digest) == Some(digests.bundle) {
                return self.activate(job).await;
            }
        }
        if current.as_ref().map_or(0, |c| c.generation) != job.expected_generation {
            // Another device won; this reservation can never commit.
            self.workspace.delete_private(ROOT_JOB)?;
            return Err(Error::VersionConflict);
        }
        if job.context.is_none() {
            let device = state.active(&self.workspace.meta.device_id)?;
            ensure(
                device.input.capabilities.contains(&Capability::VaultUnlock),
                Error::Forbidden,
            )?;
            let live = state
                .info
                .root_slot
                .clone()
                .filter(|s| s.expires_at > state.security.certified_at);
            let slot = match live {
                Some(slot) => slot,
                None => {
                    state = self
                        .mutate(
                            AccountCommand::ReserveRoot {
                                expected_generation: job.expected_generation,
                                op_id: job.op_id,
                            },
                            None,
                        )
                        .await?;
                    state.info.root_slot.clone().ok_or(Error::VersionConflict)?
                }
            };
            ensure(slot.op_id == job.op_id, Error::VersionConflict)?;
            job.context = Some(RootContext {
                account: account.id.to_string(),
                environment: crate::config::environment_name(&self.config.environment).into(),
                generation: slot.generation,
                op_id: hex::encode(*job.op_id),
                security_epoch: slot.security_epoch,
            });
            save(self, &job)?;
        }
        let context = job.context.clone().ok_or(Error::IntegrityFailed)?;
        if job.bytes.is_none() {
            let slot = state
                .info
                .root_slot
                .as_ref()
                .ok_or(Error::VersionConflict)?;
            ensure(
                slot.op_id == job.op_id
                    && slot.expires_at > now_ms()
                    && slot.security_epoch == state.info.security_epoch,
                Error::VersionConflict,
            )?;
            let recipients: Vec<Recipient> = state
                .info
                .devices
                .iter()
                .filter(|(_, d)| {
                    d.revoked_at.is_none()
                        && d.input.capabilities.contains(&Capability::VaultUnlock)
                })
                .map(|(id, d)| Recipient {
                    device: *id,
                    hpke_pub: d.input.hpke_pub,
                })
                .collect();
            let recovery = self
                .recovery_key(&account.id, context.generation, state.info.home_cose)
                .await?;
            let candidate = *job
                .candidate
                .get_or_insert_with(|| ByteArray::new(random_key()));
            save(self, &job)?;
            let previous = self.root.as_ref().map(|r| PreviousLink {
                digest: &r.sha256,
                upload_id: &r.upload_id,
                generation: r.generation,
                root: &r.root,
            });
            let bytes = wrap_root(
                &context,
                &candidate,
                &recipients,
                &recovery,
                (&self.workspace.meta.device_id, &self.keys()?.signing),
                previous,
            )?;
            job.bytes = Some(ByteBuf::from(bytes));
            save(self, &job)?;
        }
        let bytes = job.bytes.clone().ok_or(Error::IntegrityFailed)?.into_vec();
        let digests = bundle_digests(&parse_bundle(&bytes)?)?;
        let placeholder = canonical(&Vec::<u8>::new());
        let plan = job
            .plan
            .get_or_insert_with(|| UploadPlan {
                upload_id: random_hex(),
                // The relay serves the committed root by its certified digest.
                object_id: hex::encode(*digests.bundle),
                version_id: random_hex(),
                root_generation: context.generation,
                epoch: 0,
                control_head: None,
                kind: "root".into(),
                chunks: vec![ChunkRef {
                    digest: hex::encode(*sha256(&placeholder)),
                    size: placeholder.len() as u64,
                }],
                manifest_digest: hex::encode(*sha256(&bytes)),
                manifest_size: bytes.len() as u64,
                expires_at: now_ms() + dmsg_types::DAY,
            })
            .clone();
        save(self, &job)?;
        self.workspace
            .store
            .put_chunk(&plan.chunks[0].digest, &placeholder)?;
        self.upload(
            &self.base()?,
            &plan,
            &bytes,
            0,
            crate::files::Source::Store,
            |_, _| Ok(()),
        )
        .await?;
        self.download(&plan.upload_id, "manifest", &plan.manifest_digest)
            .await?;
        self.mutate(
            AccountCommand::CommitRoot {
                expected_generation: job.expected_generation,
                op_id: job.op_id,
                root: ContentRootRef {
                    generation: context.generation,
                    suite: ROOT_SUITE.into(),
                    bundle_digest: digests.bundle,
                    recipients_digest: digests.recipients,
                    body_digest: digests.body,
                },
            },
            None,
        )
        .await?;
        self.activate(job).await
    }

    /// Make a committed job's root this device's root.
    async fn activate(&mut self, job: RootJob) -> Result<u64> {
        let account = self.account()?.clone();
        let state = self.refresh(account.home, &account.id).await?;
        let bytes = job.bytes.ok_or(Error::IntegrityFailed)?.into_vec();
        let digests = bundle_digests(&parse_bundle(&bytes)?)?;
        let current = state.info.current_root.ok_or(Error::IntegrityFailed)?;
        let context = job.context.ok_or(Error::IntegrityFailed)?;
        ensure(
            current.bundle_digest == digests.bundle && current.generation == context.generation,
            Error::IntegrityFailed,
        )?;
        if self.root.as_ref().map(|r| r.generation) == Some(current.generation) {
            // Already opened from the committed bundle after a restart.
            self.workspace.delete_private(ROOT_JOB)?;
            return Ok(current.generation);
        }
        let mut history = BTreeMap::new();
        if let Some(previous) = self.root.take() {
            history = previous.history;
            history.insert(previous.generation, previous.root);
        }
        let keys = RootKeys {
            generation: current.generation,
            digest: current.bundle_digest,
            sha256: hex::encode(*sha256(&bytes)),
            upload_id: job.plan.ok_or(Error::IntegrityFailed)?.upload_id,
            root: Zeroizing::new(*job.candidate.ok_or(Error::IntegrityFailed)?),
            history,
        };
        self.save_root(&keys)?;
        self.root = Some(keys);
        self.workspace.delete_private(ROOT_JOB)?;
        Ok(current.generation)
    }
}

#[cfg(test)]
#[path = "root_tests.rs"]
mod tests;
