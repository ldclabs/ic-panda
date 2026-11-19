//! One standalone workspace per installation and deployment environment.
use crate::{
    Client, Status,
    client::{CURRENT, LocalAccount, Slot, WORKSPACE},
    codec::{canonical, decode, unb64},
    config::environment_name,
    crypto::{Key, derive, open_authenticated, random_id, random_key, seal},
    now_ms,
    root::{History, RootKeys},
    secure::{HardwareAnchor, PresenceKey, SecureStorage},
    storage,
    store::Store,
    workspace::{Anchor, Keys, META, Meta, Workspace},
};
use dmsg_types::{AccountId, Environment, Error, Hash, LOCAL_ACCOUNT_ID, Result, ensure};
use icu_normalizer::ComposingNormalizer;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::{collections::BTreeMap, io::Write, path::Path, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;
use zeroize::{Zeroize, Zeroizing};

pub(crate) const DIRECTORY: &str = "local-account";
const STAGING: &str = "local-account.creating";
const LDK: &str = "local-account-ldk/1";
const ROOT: &str = "local-account-root/1";

/// This operation needs a service account.
pub fn only() -> Error {
    dmsg_types::invalid("LOCAL_ACCOUNT_ONLY")
}

fn unavailable() -> Error {
    dmsg_types::invalid("LOCAL_HARDWARE_UNAVAILABLE")
}

/// The wrapping selected at creation. Never infer the policy from hardware availability on reopen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protection {
    /// A secret sealed by the platform presence key.
    HardwareV1,
    /// Password wrapping with a random Argon2id salt.
    PasswordV1(ByteArray<16>),
}

impl Protection {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::HardwareV1 => "hardware",
            Self::PasswordV1(_) => "password",
        }
    }
}

impl Slot {
    pub(crate) fn for_account(id: AccountId) -> Self {
        if id.is_empty() {
            Self::Local
        } else {
            Self::Account(id)
        }
    }

    pub(crate) fn account_id(&self) -> Option<AccountId> {
        match self {
            Self::Local => Some(LOCAL_ACCOUNT_ID),
            Self::Account(id) => Some(*id),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct LocalRoot {
    current: u64,
    roots: BTreeMap<u64, ByteArray<32>>,
}

impl Drop for LocalRoot {
    fn drop(&mut self) {
        for k in self.roots.values_mut() {
            k.as_mut_slice().zeroize();
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Handle {
    handle: ByteBuf,
    context: ByteBuf,
}

fn slot_name(environment: &str, device: Hash) -> String {
    format!(
        "dmsg:{environment}:{LOCAL_ACCOUNT_ID}:{}:local-account-v1",
        hex::encode(*device)
    )
}

pub(crate) fn workspace(dir: &Path, env: &Environment, read_only: bool) -> Result<Workspace> {
    let path = dir.join(WORKSPACE);
    ensure(path.is_file(), Error::NotFound)?;
    let store = if read_only {
        Store::read_only(&path)?
    } else {
        Store::open(&path)?
    };
    let meta: Meta = store.meta(META)?.ok_or(Error::IntegrityFailed)?;
    ensure(
        meta.environment == environment_name(env) && meta.account.is_none() && meta.local.is_some(),
        Error::IntegrityFailed,
    )?;
    ensure(
        !matches!(meta.local, Some(Protection::PasswordV1(_)))
            || (meta.anchor.is_none() && meta.platform.is_none()),
        Error::IntegrityFailed,
    )?;
    store.checkpoint()?;
    Ok(Workspace { store, meta })
}

fn context<'a>(
    environment: &'a str,
    device: Hash,
    protection: &'a Protection,
) -> (&'static str, &'a str, AccountId, Hash, &'a Protection) {
    (
        "dmsg/local-account/ldk/1",
        environment,
        LOCAL_ACCOUNT_ID,
        device,
        protection,
    )
}

fn password_key(password: &str, salt: &ByteArray<16>, creating: bool) -> Result<Zeroizing<Key>> {
    ensure(
        password.len() <= 1024,
        dmsg_types::invalid("LOCAL_PASSWORD_LENGTH"),
    )?;
    let normalized = Zeroizing::new(
        ComposingNormalizer::new_nfc()
            .normalize(password)
            .into_owned(),
    );
    ensure(
        !normalized.is_empty()
            && normalized.len() <= 256
            && (!creating || normalized.graphemes(true).count() >= 12),
        dmsg_types::invalid("LOCAL_PASSWORD_LENGTH"),
    )?;
    crate::kdf::derive(normalized.as_bytes(), salt.as_slice(), 65536, 3, 4)
}

fn new_password() -> Protection {
    Protection::PasswordV1(ByteArray::new(
        random_key()[..16].try_into().expect("salt length"),
    ))
}

/// Immutable work for the blocking credential/KDF worker. It owns no Client or workspace lock.
pub struct Challenge {
    generation: u64,
    environment: String,
    device: Hash,
    protection: Protection,
    create: bool,
    hardware: Option<Arc<dyn PresenceKey>>,
    anchor: Option<Arc<dyn HardwareAnchor>>,
    secure: Option<Arc<dyn SecureStorage>>,
}

/// Short-lived result bound to the request, session and immutable wrapping.
pub struct Access {
    request: Challenge,
    wrapping: Zeroizing<Key>,
    created_handle: Option<Handle>,
    created_anchor: Option<Anchor>,
}

impl Drop for Access {
    fn drop(&mut self) {
        if let (Some(key), Some(adapter)) = (&self.created_handle, &self.request.hardware) {
            let _ = adapter.discard_local_key(&key.handle);
        }
        if let (Some(anchor), Some(adapter)) = (&self.created_anchor, &self.request.anchor) {
            let _ = adapter.discard_local_key(&anchor.handle);
        }
    }
}

impl Challenge {
    /// Identifies an authentication attempt if the platform reports permanent key loss.
    pub fn instance(&self) -> (Hash, u64) {
        (self.device, self.generation)
    }

    /// Whether this request belongs to an unpublished workspace.
    pub fn opening_new(&self) -> bool {
        self.create
    }
    /// Perform the system prompt or password derivation outside the shared client.
    pub fn authenticate(self, password: Option<&str>) -> Result<Access> {
        let mut created_handle = None;
        let mut created_anchor = None;
        let secret = match &self.protection {
            Protection::PasswordV1(salt) => {
                password_key(password.unwrap_or_default(), salt, self.create)?
            }
            Protection::HardwareV1 => {
                ensure(password.is_none(), Error::Forbidden)?;
                let hw = self.hardware.as_ref().ok_or_else(unavailable)?;
                let secure = self.secure.as_ref().ok_or_else(unavailable)?;
                let name = slot_name(&self.environment, self.device);
                if self.create {
                    let anchor = self.anchor.as_ref().ok_or_else(unavailable)?;
                    let (handle, context, expected) = hw.create(false)?;
                    let (secret, anchor_handle, public) = (|| {
                        let secret = hw.open(&handle, &context, false)?;
                        ensure(*secret == *expected, Error::IntegrityFailed)?;
                        let (anchor_handle, public) = anchor.create()?;
                        Ok((secret, anchor_handle, public))
                    })()
                    .inspect_err(|_: &Error| {
                        let _ = hw.discard_local_key(&handle);
                    })?;
                    created_anchor = Some(Anchor {
                        handle: anchor_handle.into(),
                        public: public.into(),
                        created_at: now_ms(),
                        late: false,
                        foreign: false,
                    });
                    created_handle = Some(Handle {
                        handle: handle.into(),
                        context: context.into(),
                    });
                    secret
                } else {
                    let bytes = secure
                        .get(&name)?
                        .ok_or_else(|| dmsg_types::invalid("LOCAL_HARDWARE_LOST"))?;
                    let h: Handle = decode(&bytes, crate::codec::MAX_RECORD)?;
                    hw.open(&h.handle, &h.context, false).map_err(|e| {
                        if e == Error::IntegrityFailed {
                            dmsg_types::invalid("LOCAL_HARDWARE_LOST")
                        } else {
                            e
                        }
                    })?
                }
            }
        };
        let wrapping = derive(
            secret.as_slice(),
            &context(&self.environment, self.device, &self.protection),
        );
        Ok(Access {
            request: self,
            wrapping,
            created_handle,
            created_anchor,
        })
    }
}

impl Access {
    fn keys(&self, ws: &Workspace) -> Result<Keys> {
        ensure(
            ws.meta.device_id == self.request.device
                && ws.meta.local.as_ref() == Some(&self.request.protection)
                && ws.meta.environment == self.request.environment,
            Error::Locked,
        )?;
        let sealed = ws.store.sealed(LDK)?.ok_or(Error::IntegrityFailed)?;
        let bytes = unb64(&sealed).map_err(|_| Error::IntegrityFailed)?;
        let cause = match self.request.protection {
            Protection::PasswordV1(_) => dmsg_types::invalid("LOCAL_UNLOCK_FAILED"),
            Protection::HardwareV1 => Error::IntegrityFailed,
        };
        let key = open_authenticated(
            &self.wrapping,
            &bytes,
            &context(
                &ws.meta.environment,
                ws.meta.device_id,
                &self.request.protection,
            ),
            cause,
        )?;
        ws.device_keys(Zeroizing::new(
            key.as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        ))
    }
}

/// A replacement password wrap, prepared outside the client lock.
pub struct PasswordChange {
    prior: Access,
    protection: Protection,
    wrapping: Zeroizing<Key>,
}

impl Access {
    /// Derive the replacement password independently of the previous wrapping.
    pub fn change_password(self, password: &str) -> Result<PasswordChange> {
        ensure(
            matches!(self.request.protection, Protection::PasswordV1(_)),
            Error::Forbidden,
        )?;
        let protection = new_password();
        let Protection::PasswordV1(salt) = &protection else {
            unreachable!()
        };
        let secret = password_key(password, salt, true)?;
        let wrapping = derive(
            secret.as_slice(),
            &context(&self.request.environment, self.request.device, &protection),
        );
        Ok(PasswordChange {
            prior: self,
            protection,
            wrapping,
        })
    }
}

fn load_root(ws: &Workspace, keys: &Keys) -> Result<RootKeys> {
    let record = ws
        .private::<LocalRoot>(keys, ROOT)?
        .ok_or(Error::IntegrityFailed)?;
    let roots: History = record
        .roots
        .iter()
        .map(|(g, k)| (*g, Zeroizing::new(**k)))
        .collect();
    RootKeys::local(record.current, roots)
}

impl Client {
    /// Whether the selected slot belongs only to this installation.
    pub fn is_local(&self) -> bool {
        self.slot == Slot::Local
    }

    pub(crate) fn require_online(&self) -> Result<()> {
        ensure(!self.offline_mode(), only())
    }

    /// Includes the system prompt before a new standalone workspace is published.
    pub fn offline_mode(&self) -> bool {
        self.is_local() || !self.cloud.network_enabled()
    }

    /// ID for content encryption, including the local workspace.
    pub(crate) fn content_id(&self) -> Result<AccountId> {
        if self.is_local() {
            Ok(LOCAL_ACCOUNT_ID)
        } else {
            Ok(self.account()?.id)
        }
    }

    pub(crate) fn account_kind(&self) -> &'static str {
        if self.is_local() {
            "local"
        } else if self.workspace.meta.account.is_some() {
            "cloud"
        } else {
            "none"
        }
    }

    pub(crate) fn local_unlock_method(&self) -> &'static str {
        if let Some(p) = &self.workspace.meta.local {
            return p.name();
        }
        if self.presence.as_ref().and_then(|p| p.presence()).is_some() && self.secure().is_some() {
            "hardware"
        } else if self.require_fast_presence {
            "unavailable"
        } else {
            "password"
        }
    }

    pub(crate) fn local_account(&self) -> Result<Option<LocalAccount>> {
        let dir = self.slot_dir(&Slot::Local);
        if !dir.exists() {
            return Ok(None);
        }
        let ws = workspace(&dir, &self.config.environment, true)?;
        Ok(Some(LocalAccount {
            account_kind: "local",
            local_unlock_method: ws.meta.local.as_ref().map(Protection::name),
            account: LOCAL_ACCOUNT_ID.to_string(),
            device_id: hex::encode(*ws.meta.device_id),
            bound: true,
            fast_unlock: matches!(ws.meta.local, Some(Protection::HardwareV1)),
            login_unlocked_at: None,
            current: self.is_local(),
            moved: self.moved.contains(&LOCAL_ACCOUNT_ID),
        }))
    }

    pub(crate) fn write_current(&self, id: AccountId) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.dir).map_err(storage)?;
        file.write_all(id.to_string().as_bytes()).map_err(storage)?;
        file.as_file().sync_all().map_err(storage)?;
        file.persist(self.dir.join(CURRENT))
            .map_err(|e| storage(e.error))?;
        Ok(())
    }

    fn local_challenge(&self, device: Hash, protection: Protection, create: bool) -> Challenge {
        Challenge {
            generation: self.session_generation,
            environment: environment_name(&self.config.environment).into(),
            device,
            protection,
            create,
            hardware: self.presence.clone(),
            secure: self.secure(),
            anchor: self.anchor.clone(),
        }
    }

    /// Start creating a local workspace. Existing workspaces are never replaced.
    pub fn prepare_local_create(&mut self) -> Result<Challenge> {
        self.finish_local_deletion()?;
        ensure(
            !self.slot_dir(&Slot::Local).exists(),
            Error::IdempotencyConflict,
        )?;
        let protection = match self.local_unlock_method() {
            "hardware" => Protection::HardwareV1,
            "password" => new_password(),
            _ => return Err(unavailable()),
        };
        self.shutdown();
        self.cloud.network(false);
        Ok(self.local_challenge(Hash::new(random_id()), protection, true))
    }

    /// Release the temporary network gate when an unpublished creation was cancelled.
    pub fn local_creation_failed(&mut self, generation: u64) {
        if !self.is_local() && self.session_generation == generation {
            self.cloud.network(true);
        }
    }

    /// Remember confirmed permanent loss on the instance that asked. Cancellations remain retryable.
    pub fn local_key_failure(&mut self, instance: (Hash, u64), error: &Error) -> Result<()> {
        if *error == dmsg_types::invalid("LOCAL_HARDWARE_LOST")
            && self.is_local()
            && instance == (self.workspace.meta.device_id, self.session_generation)
            && let Some(anchor) = self.workspace.meta.anchor.as_mut()
        {
            anchor.foreign = true;
            self.workspace.save_meta()?;
            self.shutdown();
            self.check_hardware()?;
        }
        Ok(())
    }

    /// Start an unlock or a fresh owner confirmation on the selected workspace.
    pub fn prepare_local_unlock(&mut self) -> Result<Challenge> {
        ensure(self.is_local(), Error::Forbidden)?;
        self.hardware_ready()?;
        Ok(self.local_challenge(
            self.workspace.meta.device_id,
            self.workspace
                .meta
                .local
                .clone()
                .ok_or(Error::IntegrityFailed)?,
            false,
        ))
    }

    /// Authenticate the read-only local source while a different, cloud workspace remains selected.
    pub fn prepare_local_import(&self) -> Result<Challenge> {
        self.account()?;
        crate::vault::backup::access(self, true)?;
        let mut ws = workspace(&self.slot_dir(&Slot::Local), &self.config.environment, true)?;
        let protection = ws.meta.local.clone().ok_or(Error::IntegrityFailed)?;
        if protection == Protection::HardwareV1 {
            ensure(ws.meta.anchor.is_some(), Error::IntegrityFailed)?;
            let anchor = self.anchor.as_ref().ok_or_else(unavailable)?;
            ensure(
                crate::hardware::verdict(anchor.as_ref(), &ws.store, &mut ws.meta)?
                    == crate::hardware::Verdict::Native,
                Error::Forbidden,
            )?;
        }
        Ok(self.local_challenge(ws.meta.device_id, protection, false))
    }

    pub(crate) fn local_source(&self, access: Access) -> Result<Self> {
        ensure(
            !access.request.create && access.request.generation == self.session_generation,
            Error::Locked,
        )?;
        let dir = self.slot_dir(&Slot::Local);
        let ws = workspace(&dir, &self.config.environment, true)?;
        let keys = access.keys(&ws)?;
        let root = load_root(&ws, &keys)?;
        let mut source = Self::empty(self.config.clone(), &self.dir, None)?;
        source.slot = Slot::Local;
        source.workspace = ws;
        source.blobs = Some(crate::library::blobs::Blobs::new(dir.join("blobs")));
        source.keys = Some(keys);
        source.root = Some(root);
        source.cloud.network(false);
        Ok(source)
    }

    /// Publish a verified local workspace. Only ciphertext reaches its SQLite database.
    pub fn complete_local_create(&mut self, mut access: Access) -> Result<Status> {
        let r = &access.request;
        ensure(
            r.create && r.generation == self.session_generation,
            Error::Locked,
        )?;
        let dir = self.slot_dir(&Slot::Local);
        ensure(!dir.exists(), Error::IdempotencyConflict)?;
        let stage = self.dir.join(STAGING);
        if stage.exists() {
            if let Ok(old) = workspace(&stage, &self.config.environment, true)
                && let Some(secure) = self.secure()
            {
                secure.delete(&slot_name(&old.meta.environment, old.meta.device_id))?;
            }
            std::fs::remove_dir_all(&stage).map_err(storage)?;
        }
        crate::library::blobs::create_private_dir(&stage)?;
        let (mut ws, keys) = Workspace::fresh(
            Store::open(&stage.join(WORKSPACE))?,
            &self.config.environment,
            now_ms(),
            r.device,
            Some(r.protection.clone()),
        );
        ws.meta.anchor = access.created_anchor.clone();
        let root = LocalRoot {
            current: 1,
            roots: BTreeMap::from([(1, ByteArray::new(random_key()))]),
        };
        let ctx = context(&r.environment, r.device, &r.protection);
        ws.store.atomic(|store| {
            store.set_sealed(LDK, &seal(&access.wrapping, keys.ldk.as_slice(), &ctx))?;
            ws.seal_device(&keys)?;
            ws.put_private(&keys, ROOT, &root)?;
            store.set_meta(META, &ws.meta)
        })?;
        ws.store.checkpoint()?;
        let verified = access.keys(&ws)?;
        let roots = load_root(&ws, &verified)?;
        if let Some(handle) = &access.created_handle {
            r.secure
                .as_ref()
                .ok_or_else(unavailable)?
                .set(&slot_name(&r.environment, r.device), &canonical(handle))?;
        }
        drop(ws);
        std::fs::rename(&stage, &dir).map_err(storage)?;
        // The published instance now owns its platform keys, even if an ensuing selection fails.
        access.created_handle = None;
        access.created_anchor = None;
        self.select(Slot::Local)?;
        self.hardware_ready()?;
        self.keys = Some(verified);
        self.root = Some(roots);
        self.status()
    }

    /// Finish only the session in which this unlock was requested.
    pub fn complete_local_unlock(&mut self, access: Access) -> Result<Status> {
        let keys = self.verified_keys(&access)?;
        let root = load_root(&self.workspace, &keys)?;
        self.shutdown();
        self.keys = Some(keys);
        self.root = Some(root);
        self.status()
    }

    /// Verify a fresh owner confirmation without restarting this session.
    pub fn verify_local(&mut self, access: &Access) -> Result<()> {
        self.verified_keys(access).map(drop)
    }

    fn verified_keys(&mut self, access: &Access) -> Result<Keys> {
        ensure(
            self.is_local()
                && !access.request.create
                && access.request.generation == self.session_generation,
            Error::Locked,
        )?;
        self.hardware_ready()?;
        access.keys(&self.workspace)
    }

    /// Check the old password again against the live header before committing the replacement.
    pub fn complete_password_change(&mut self, change: PasswordChange) -> Result<Status> {
        self.verify_local(&change.prior)?;
        let ws = &self.workspace;
        let ctx = context(&ws.meta.environment, ws.meta.device_id, &change.protection);
        let sealed = seal(&change.wrapping, self.keys()?.ldk.as_slice(), &ctx);
        let mut meta = ws.meta.clone();
        meta.local = Some(change.protection);
        ws.store.atomic(|s| {
            s.set_sealed(LDK, &sealed)?;
            s.set_meta(META, &meta)
        })?;
        self.workspace.meta = meta;
        self.workspace.store.checkpoint()?;
        self.status()
    }

    /// Leave the local workspace before explicitly starting online sign-in.
    pub fn leave_local(&mut self) -> Result<Status> {
        ensure(self.is_local(), Error::Forbidden)?;
        self.shutdown();
        self.workspace = crate::client::idle(&self.config.environment)?;
        self.slot = Slot::Idle;
        self.hardware = crate::hardware::Verdict::Native;
        self.cloud.network(true);
        self.blobs = None;
        match std::fs::remove_file(self.dir.join(CURRENT)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(storage(e)),
            _ => {}
        }
        self.status()
    }

    /// Number of ordinary vault entries and independent files affected by removal.
    pub fn local_counts(&mut self) -> Result<serde_json::Value> {
        ensure(self.is_local(), Error::Forbidden)?;
        self.keys()?;
        let vault = self
            .workspace
            .store
            .heads(crate::content::VAULT)?
            .into_iter()
            .map(|(id, rev)| {
                self.vault_version(&id, &rev)
                    .map(|v| usize::from(!v.item.reserved()))
            })
            .sum::<Result<usize>>()?;
        Ok(serde_json::json!({"vault": vault, "files": self.catalog()?.files.len()}))
    }

    fn finish_local_deletion(&self) -> Result<()> {
        let deleting = self.dir.join("local-account.deleting");
        if !deleting.exists() {
            return Ok(());
        }
        let saved = workspace(&deleting, &self.config.environment, true)?;
        let name = slot_name(&saved.meta.environment, saved.meta.device_id);
        if let Some(key) = &saved.meta.anchor {
            let secure = self.secure().ok_or_else(unavailable)?;
            if let Some(bytes) = secure.get(&name)? {
                let h: Handle = decode(&bytes, crate::codec::MAX_RECORD)?;
                self.presence
                    .as_ref()
                    .ok_or_else(unavailable)?
                    .discard_local_key(&h.handle)?;
            }
            self.anchor
                .as_ref()
                .ok_or_else(unavailable)?
                .discard_local_key(&key.handle)?;
            secure.delete(&name)?;
        }
        drop(saved);
        std::fs::remove_dir_all(deleting).map_err(storage)
    }

    /// Remove the selected standalone instance after fresh owner authentication.
    pub fn delete_local(&mut self, access: Access) -> Result<Status> {
        self.keys()?;
        self.verify_local(&access)?;
        let dir = self.slot_dir(&Slot::Local);
        self.finish_local_deletion()?;
        let deleting = self.dir.join("local-account.deleting");
        let status = self.leave_local()?;
        std::fs::rename(dir, &deleting).map_err(storage)?;
        self.finish_local_deletion()?;
        Ok(status)
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
