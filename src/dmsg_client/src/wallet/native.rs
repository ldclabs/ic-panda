//! This device's native Taproot cosigner keys: the BIP-340 keys whose
//! signature, beside the wallet's chain key, the first leaf (L1) of a
//! native Taproot address requires. One random root is sealed for each
//! protector, a dedicated hardware key that recovers it only with the person
//! present, or kept in software under this device's local key when the
//! person chose that. A wallet's key derives from the root, so registering
//! it or signing an L1 spend asks for the person once; its public key is
//! kept, so showing it asks nothing. The person is asked without the client.
use super::*;
use crate::crypto::{Key, derive, open_bytes, random_key, seal_bytes};
use crate::secure::PresenceKey;
use dmsg_types::AccountId;
use k256::schnorr::{SigningKey, signature::hazmat::RandomizedPrehashSigner};
use serde_bytes::ByteBuf;
use std::sync::Arc;
use zeroize::Zeroizing;

const ROOT: &str = "wallet-native-root";
const KEY: &str = "wallet-native:";

/// The root and what recovers it, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone)]
struct NativeRoot {
    root_id: Hash,
    /// The root itself, when the person chose to keep it in software.
    software: Option<ByteBuf>,
    protectors: Vec<Protector>,
    created_at: u64,
}

/// A dedicated hardware key that recovers the root with the person present.
#[derive(Serialize, Deserialize, Clone)]
struct Protector {
    index: u32,
    handle: ByteBuf,
    peer: ByteBuf,
    /// For a key that takes only the fingerprints enrolled when it was
    /// made: their digest then.
    biometry: Option<Hash>,
    /// The root, sealed under the key's secret.
    sealed: ByteBuf,
    created_at: u64,
}

/// A wallet's key, without its secret.
#[derive(Serialize, Deserialize, Clone)]
struct NativeKey {
    root_id: Hash,
    index: u32,
    /// x-only public key.
    public_key: ByteBuf,
    created_at: u64,
}

impl NativeKey {
    fn view(&self, wallet: Principal) -> Json {
        json!({
            "wallet": wallet.to_text(),
            "public_key": hex::encode(&self.public_key),
            "created_at": self.created_at,
        })
    }
}

fn wrapping(secret: &Key, account: &AccountId, device: Hash, index: u32) -> Zeroizing<Key> {
    derive(
        secret,
        &("dmsg-wallet/native-root-wrap/v1", account, device, index),
    )
}

fn aad(account: &AccountId, device: Hash, root_id: Hash, index: u32) -> impl Serialize + '_ {
    (
        "dmsg-wallet/native-root/v1",
        account,
        device,
        root_id,
        index,
    )
}

fn root_key(bytes: &[u8]) -> Result<Zeroizing<Key>> {
    Ok(Zeroizing::new(
        bytes.try_into().map_err(|_| Error::IntegrityFailed)?,
    ))
}

/// Seal `root` for a new hardware key; asks nothing.
fn protect(
    hardware: &dyn PresenceKey,
    account: &AccountId,
    device: Hash,
    root_id: Hash,
    index: u32,
    biometry_only: bool,
    root: &Key,
) -> Result<Protector> {
    let biometry = match biometry_only {
        true => Some(
            hardware
                .biometry_state()
                .ok_or_else(|| Error::Unavailable("hardware: biometry unavailable".into()))?,
        ),
        false => None,
    };
    let (handle, peer, secret) = hardware.create(biometry_only)?;
    let sealed = seal_bytes(
        &wrapping(&secret, account, device, index),
        root,
        &aad(account, device, root_id, index),
    );
    Ok(Protector {
        index,
        handle: handle.into(),
        peer: peer.into(),
        biometry,
        sealed: sealed.into(),
        created_at: now_ms(),
    })
}

/// The wallet's key number `index`; a derivation outside the curve's
/// order tries the next attempt.
fn signing_key(root: &Key, wallet: Principal, index: u32) -> SigningKey {
    (0u32..)
        .find_map(|attempt| {
            let secret = derive(
                root,
                &("dmsg-wallet/native-cosigner/v1", wallet, index, attempt),
            );
            SigningKey::from_slice(&secret[..]).ok()
        })
        .expect("a scalar in range")
}

/// What recovers the root, without the client.
struct Opener {
    account: AccountId,
    device: Hash,
    root: NativeRoot,
    hardware: Option<Arc<dyn PresenceKey>>,
}

impl Opener {
    /// The root; asks for the person unless it is kept in software. A
    /// declined prompt stops; a protector that no longer opens gives way to
    /// the next.
    fn open(&self) -> Result<Zeroizing<Key>> {
        if let Some(root) = &self.root.software {
            return root_key(root);
        }
        let hardware = self
            .hardware
            .as_deref()
            .ok_or_else(|| Error::Unavailable("no hardware key on this device".into()))?;
        let mut failure = Error::NotFound;
        for p in &self.root.protectors {
            match self.unwrap(hardware, p) {
                Err(Error::Forbidden) => return Err(Error::Forbidden),
                Err(e) => failure = e,
                opened => return opened,
            }
        }
        Err(failure)
    }

    fn unwrap(&self, hardware: &dyn PresenceKey, p: &Protector) -> Result<Zeroizing<Key>> {
        let secret = match hardware.open(&p.handle, &p.peer, p.biometry.is_some()) {
            Err(Error::Forbidden) => return Err(Error::Forbidden),
            // Such a key is lost once the enrolled fingerprints change.
            Err(_)
                if p.biometry.is_some_and(|state| {
                    hardware
                        .biometry_state()
                        .is_some_and(|current| current != state)
                }) =>
            {
                return Err(Error::IntegrityFailed);
            }
            result => result?,
        };
        let root = open_bytes(
            &wrapping(&secret, &self.account, self.device, p.index),
            &p.sealed,
            &aad(&self.account, self.device, self.root.root_id, p.index),
        )?;
        root_key(&root)
    }
}

/// Run what asks for the person off the client's executor.
async fn asking<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Error::Unavailable("hardware: the request stopped".into()))?
}

impl Client {
    fn native_root(&self) -> Result<Option<NativeRoot>> {
        self.workspace.private(self.keys()?, ROOT)
    }

    /// This device's key for `wallet`, if the current root made one.
    fn native_key(&self, wallet: Principal) -> Result<Option<NativeKey>> {
        let Some(root) = self.native_root()? else {
            return Ok(None);
        };
        Ok(self
            .workspace
            .private::<NativeKey>(self.keys()?, &format!("{KEY}{wallet}"))?
            .filter(|k| k.root_id == root.root_id))
    }

    /// Whether this device has a root, and its key for `wallet`.
    pub(super) fn native_public_key(&self, wallet: Principal) -> Result<(bool, Option<ByteBuf>)> {
        Ok((
            self.native_root()?.is_some(),
            self.native_key(wallet)?.map(|k| k.public_key),
        ))
    }

    /// Whether the cosigner key `public_key` of `wallet` is this device's.
    pub(super) fn native_holds(&self, wallet: Principal, public_key: &[u8]) -> Result<bool> {
        Ok(self
            .native_key(wallet)?
            .is_some_and(|k| k.public_key.as_slice() == public_key))
    }

    fn native_opener(&self) -> Result<Opener> {
        Ok(Opener {
            account: self.account()?.id,
            device: self.workspace.meta.device_id,
            root: self.native_root()?.ok_or(Error::NotFound)?,
            hardware: self.presence.clone(),
        })
    }

    /// The native cosigner root of this device without secrets: whether it
    /// exists, how it is protected, the hardware this device has for it,
    /// and this device's key for `wallet`.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn wallet_native_status(&self, wallet: Option<Principal>) -> Result<Json> {
        let root = self.native_root()?;
        let key = match wallet {
            Some(wallet) => self.native_key(wallet)?.map(|k| k.view(wallet)),
            None => None,
        };
        Ok(json!({
            "created": root.is_some(),
            "protection": root.as_ref().map(|r| match r.software {
                Some(_) => "software",
                None => "hardware",
            }),
            "protectors": root.as_ref().map_or(vec![], |r| r.protectors.iter().map(|p| json!({
                "index": p.index,
                "biometry_only": p.biometry.is_some(),
                "created_at": p.created_at,
            })).collect()),
            "created_at": root.as_ref().map(|r| r.created_at),
            "hardware": self.presence.as_ref().and_then(|p| p.presence()),
            "key": key,
        }))
    }

    /// Make the root, sealed for a new hardware key that asks for the
    /// person, or kept in software when `software`: only the caller chooses
    /// software, never a missing keystore.
    ///
    /// # Errors
    /// Returns `Error::IdempotencyConflict` when the root exists and
    /// `Error::Unavailable` when this device has no such hardware.
    pub fn wallet_native_create(&mut self, software: bool, biometry_only: bool) -> Result<Json> {
        ensure(self.native_root()?.is_none(), Error::IdempotencyConflict)?;
        let account = self.account()?.id;
        let device = self.workspace.meta.device_id;
        let root = Zeroizing::new(random_key());
        let root_id = Hash::new(random_id());
        let (software, protectors) = if software {
            (Some(ByteBuf::from(root.to_vec())), vec![])
        } else {
            let hardware = self
                .presence
                .clone()
                .filter(|p| p.presence().is_some())
                .ok_or_else(|| Error::Unavailable("no hardware key on this device".into()))?;
            let protector = protect(
                hardware.as_ref(),
                &account,
                device,
                root_id,
                0,
                biometry_only,
                &root,
            )?;
            (None, vec![protector])
        };
        let record = NativeRoot {
            root_id,
            software,
            protectors,
            created_at: now_ms(),
        };
        self.workspace.put_private(self.keys()?, ROOT, &record)?;
        self.wallet_native_status(None)
    }

    /// Remove protector `index`; at least one stays.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown protector and
    /// `Error::Forbidden` for the last one.
    pub fn wallet_native_remove_protector(&mut self, index: u32) -> Result<Json> {
        let mut root = self.native_root()?.ok_or(Error::NotFound)?;
        let before = root.protectors.len();
        root.protectors.retain(|p| p.index != index);
        ensure(root.protectors.len() < before, Error::NotFound)?;
        ensure(!root.protectors.is_empty(), Error::Forbidden)?;
        self.workspace.put_private(self.keys()?, ROOT, &root)?;
        self.wallet_native_status(None)
    }
}

/// Add a protector: the person opens the root once, then a new hardware
/// key is sealed for it. A key for today's fingerprints only replaces an
/// older one by adding it, then removing the old.
///
/// # Errors
/// Returns `Error::Forbidden` for a root kept in software or a declined
/// prompt, and `Error::Unavailable` without such hardware.
pub async fn add_protector<H: Hold>(h: &H, biometry_only: bool) -> Result<Json> {
    let bound = Unlocked::new(h).await?;
    let h = &bound;
    let opener = h.with(|c| c.native_opener()).await?;
    ensure(opener.root.software.is_none(), Error::Forbidden)?;
    let hardware = opener
        .hardware
        .clone()
        .filter(|p| p.presence().is_some())
        .ok_or_else(|| Error::Unavailable("no hardware key on this device".into()))?;
    let root_id = opener.root.root_id;
    let protector = asking(move || {
        let root = opener.open()?;
        let index = opener
            .root
            .protectors
            .iter()
            .map(|p| p.index + 1)
            .max()
            .unwrap_or(0);
        protect(
            hardware.as_ref(),
            &opener.account,
            opener.device,
            root_id,
            index,
            biometry_only,
            &root,
        )
    })
    .await?;
    h.with(|c| {
        let mut root = c.native_root()?.ok_or(Error::NotFound)?;
        ensure(
            root.root_id == root_id && root.protectors.iter().all(|p| p.index < protector.index),
            Error::VersionConflict,
        )?;
        root.protectors.push(protector);
        c.workspace.put_private(c.keys()?, ROOT, &root)?;
        c.wallet_native_status(None)
    })
    .await
}

/// This device's key for `wallet` and the `AddCosigner` intent that
/// registers it for this account. Deriving it the first time asks for the
/// person. A wallet that requires hardware cosigners refuses the proposal.
///
/// # Errors
/// Returns `Error::NotFound` without a root and `Error::Forbidden` when the
/// person declines.
pub async fn key<H: Hold>(h: &H, wallet: Principal, label: &str) -> Result<Json> {
    let bound = Unlocked::new(h).await?;
    let h = &bound;
    ensure(
        !label.is_empty() && label.len() <= 64,
        dmsg_types::invalid("label"),
    )?;
    let (opener, member, kept) = h
        .with(|c| {
            Ok((
                c.native_opener()?,
                c.wallet_member()?,
                c.native_key(wallet)?,
            ))
        })
        .await?;
    let root_id = opener.root.root_id;
    let key = match kept {
        Some(key) => key,
        None => {
            let public_key = asking(move || {
                let root = opener.open()?;
                Ok(signing_key(&root, wallet, 0)
                    .verifying_key()
                    .to_bytes()
                    .to_vec())
            })
            .await?;
            let key = NativeKey {
                root_id,
                index: 0,
                public_key: public_key.into(),
                created_at: now_ms(),
            };
            h.with(|c| {
                ensure(
                    c.native_root()?.is_some_and(|r| r.root_id == root_id),
                    Error::VersionConflict,
                )?;
                c.workspace
                    .put_private(c.keys()?, &format!("{KEY}{wallet}"), &key)
            })
            .await?;
            key
        }
    };
    let input = CosignerInput {
        member,
        kind: CosignerKind::Bip340,
        public_key: key.public_key.clone(),
        envelope: CosignerEnvelope::Raw,
        hardware: false,
        label: label.into(),
    };
    Ok(json!({
        "key": key.view(wallet),
        "intent": serde_json::to_value(Intent::AddCosigner(input))
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
    }))
}

/// Sign each of `messages` with this device's key for `wallet` once the
/// person opens the root, with fresh auxiliary randomness each time.
/// Refuses when the key derived now is not the registered `public_key`.
pub(super) async fn sign<H: Hold>(
    h: &H,
    wallet: Principal,
    public_key: &[u8],
    messages: Vec<Hash>,
) -> Result<Vec<Vec<u8>>> {
    let bound = Unlocked::new(h).await?;
    let h = &bound;
    let (opener, key) = h
        .with(|c| {
            let key = c
                .native_key(wallet)?
                .filter(|k| k.public_key.as_slice() == public_key)
                .ok_or(Error::NotFound)?;
            Ok((c.native_opener()?, key))
        })
        .await?;
    let signatures = asking(move || {
        let root = opener.open()?;
        let signer = signing_key(&root, wallet, key.index);
        ensure(
            signer.verifying_key().to_bytes()[..] == key.public_key[..],
            Error::IntegrityFailed,
        )?;
        messages
            .iter()
            .map(|m| {
                signer
                    .sign_prehash_with_rng(&mut getrandom::SysRng, &m[..])
                    .map(|s| s.to_bytes().to_vec())
                    .map_err(|_| Error::IntegrityFailed)
            })
            .collect()
    })
    .await?;
    h.with(|_| Ok(())).await?;
    Ok(signatures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secure::Presence;
    use crate::testing::{TempDir, unlocked};
    use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering::SeqCst};
    use tokio::sync::Mutex;

    /// Hardware that answers as the person tells it to; each key it makes
    /// has its own secret, from the first byte of its handle.
    #[derive(Default)]
    struct Hardware {
        declines: AtomicBool,
        made: AtomicU8,
        opened: AtomicU32,
        fingerprints: AtomicU8,
        /// Keys that stopped opening.
        lost: std::sync::Mutex<Vec<u8>>,
        /// Holds every prompt until released.
        held: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    }

    impl PresenceKey for Hardware {
        fn presence(&self) -> Option<Presence> {
            Some(Presence::Biometrics)
        }

        fn create(&self, _: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)> {
            let n = self.made.fetch_add(1, SeqCst) + 1;
            Ok((vec![n; 8], vec![n; 65], Zeroizing::new([n + 100; 32])))
        }

        fn open(&self, handle: &[u8], _: &[u8], _: bool) -> Result<Zeroizing<Key>> {
            let held = self.held.lock().unwrap().take();
            if let Some(release) = held {
                release.recv().unwrap();
            }
            self.opened.fetch_add(1, SeqCst);
            if self.declines.load(SeqCst) {
                return Err(Error::Forbidden);
            }
            if self.lost.lock().unwrap().contains(&handle[0]) {
                return Err(Error::Unavailable("key".into()));
            }
            Ok(Zeroizing::new([handle[0] + 100; 32]))
        }

        fn biometry_state(&self) -> Option<Hash> {
            Some(Hash::new([self.fingerprints.load(SeqCst); 32]))
        }

        fn confirm(&self, _: &str) -> Result<()> {
            Ok(())
        }
    }

    fn wallet() -> Principal {
        Principal::from_slice(&[9; 10])
    }

    fn with_hardware(dir: &TempDir) -> (Client, Arc<Hardware>) {
        let mut client = unlocked(&dir.0, false, None);
        let hardware = Arc::new(Hardware::default());
        client.set_presence_key(hardware.clone());
        (client, hardware)
    }

    #[tokio::test]
    async fn creating_asks_nothing_and_each_wallet_key_asks_once() {
        let dir = TempDir::new();
        let (mut client, hardware) = with_hardware(&dir);
        let status = client.wallet_native_create(false, false).unwrap();
        assert_eq!(
            (
                status["protection"].as_str(),
                status["protectors"][0]["index"].as_u64()
            ),
            (Some("hardware"), Some(0))
        );
        assert_eq!(hardware.opened.load(SeqCst), 0);
        assert_eq!(
            client.wallet_native_create(false, false),
            Err(Error::IdempotencyConflict)
        );
        let h = client.hold();
        let first = key(&h, wallet(), "phone").await.unwrap();
        assert_eq!(hardware.opened.load(SeqCst), 1);
        let again = key(&h, wallet(), "phone").await.unwrap();
        assert_eq!(hardware.opened.load(SeqCst), 1);
        assert_eq!(first["key"], again["key"]);
        let intent = &first["intent"]["AddCosigner"];
        assert_eq!(
            (intent["kind"].as_str(), intent["hardware"].as_bool()),
            (Some("Bip340"), Some(false))
        );
        let other = key(&h, Principal::from_slice(&[8; 10]), "phone")
            .await
            .unwrap();
        assert_ne!(other["key"]["public_key"], first["key"]["public_key"]);
        drop(h);
        assert_eq!(
            client.wallet_native_status(Some(wallet())).unwrap()["key"],
            first["key"]
        );
    }

    #[tokio::test]
    async fn signatures_verify_under_the_registered_key_and_a_mismatch_refuses() {
        let dir = TempDir::new();
        let (mut client, _) = with_hardware(&dir);
        client.wallet_native_create(false, false).unwrap();
        let h = client.hold();
        let made = key(&h, wallet(), "phone").await.unwrap();
        let public_key = hex::decode(made["key"]["public_key"].as_str().unwrap()).unwrap();
        let cosigner = Cosigner {
            id: 3,
            input: CosignerInput {
                member: MemberId::Principal(wallet()),
                kind: CosignerKind::Bip340,
                public_key: public_key.clone().into(),
                envelope: CosignerEnvelope::Raw,
                hardware: false,
                label: "phone".into(),
            },
            counter: 0,
            added_at_ms: 0,
        };
        let messages = vec![Hash::new([1; 32]), Hash::new([2; 32])];
        let signatures = sign(&h, wallet(), &public_key, messages.clone())
            .await
            .unwrap();
        for (i, (message, signature)) in messages.iter().zip(&signatures).enumerate() {
            let sig = CosignerSig {
                cosigner: 3,
                digest_index: i as u32,
                signature: signature.clone().into(),
                authenticator_data: None,
                client_data_json: None,
            };
            dmsg_protocol::wallet::verify_cosign(&cosigner, message, &sig).unwrap();
        }
        // Fresh randomness each time: the same message signs differently.
        let twice = sign(&h, wallet(), &public_key, messages[..1].to_vec())
            .await
            .unwrap();
        assert_ne!(twice[0], signatures[0]);
        assert_eq!(
            sign(&h, wallet(), &[7; 32], messages.clone()).await,
            Err(Error::NotFound)
        );
        // A kept public key the root no longer derives is refused.
        h.with(|c| {
            let mut kept = c.native_key(wallet())?.unwrap();
            kept.index = 1;
            c.workspace
                .put_private(c.keys()?, &format!("{KEY}{}", wallet()), &kept)
        })
        .await
        .unwrap();
        assert_eq!(
            sign(&h, wallet(), &public_key, messages).await,
            Err(Error::IntegrityFailed)
        );
    }

    #[tokio::test]
    async fn protectors_open_in_turn_stop_when_declined_and_keep_one() {
        let dir = TempDir::new();
        let (mut client, hardware) = with_hardware(&dir);
        client.wallet_native_create(false, false).unwrap();
        let h = client.hold();
        let status = add_protector(&h, true).await.unwrap();
        assert_eq!(status["protectors"][1]["biometry_only"], true);
        let made = key(&h, wallet(), "phone").await.unwrap();
        let public_key = hex::decode(made["key"]["public_key"].as_str().unwrap()).unwrap();
        // The first protector stopped opening; the second recovers the root.
        hardware.lost.lock().unwrap().push(1);
        let opened = hardware.opened.load(SeqCst);
        sign(&h, wallet(), &public_key, vec![Hash::new([1; 32])])
            .await
            .unwrap();
        assert_eq!(hardware.opened.load(SeqCst), opened + 2);
        hardware.declines.store(true, SeqCst);
        assert_eq!(
            sign(&h, wallet(), &public_key, vec![Hash::new([1; 32])]).await,
            Err(Error::Forbidden)
        );
        hardware.declines.store(false, SeqCst);
        // New fingerprints lose the key that takes only the old ones.
        hardware.fingerprints.store(5, SeqCst);
        hardware.lost.lock().unwrap().push(2);
        assert_eq!(
            sign(&h, wallet(), &public_key, vec![Hash::new([1; 32])]).await,
            Err(Error::IntegrityFailed)
        );
        drop(h);
        client.wallet_native_remove_protector(0).unwrap();
        assert_eq!(
            client.wallet_native_remove_protector(1),
            Err(Error::Forbidden)
        );
        assert_eq!(
            client.wallet_native_remove_protector(7),
            Err(Error::NotFound)
        );
    }

    #[tokio::test]
    async fn a_root_opens_only_on_its_own_account_and_device() {
        let dir = TempDir::new();
        let (mut client, _) = with_hardware(&dir);
        client.wallet_native_create(false, false).unwrap();
        let mut opener = client.native_opener().unwrap();
        assert!(opener.open().is_ok());
        // A record copied into another device's workspace does not open.
        opener.device = Hash::new([4; 32]);
        assert_eq!(opener.open().err(), Some(Error::IntegrityFailed));
        let mut opener = client.native_opener().unwrap();
        opener.root.root_id = Hash::new([4; 32]);
        assert_eq!(opener.open().err(), Some(Error::IntegrityFailed));
        let mut opener = client.native_opener().unwrap();
        opener.root.protectors[0].index = 1;
        assert_eq!(opener.open().err(), Some(Error::IntegrityFailed));
    }

    #[tokio::test]
    async fn only_the_caller_chooses_software_and_it_asks_nothing() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        assert!(matches!(
            client.wallet_native_create(false, false),
            Err(Error::Unavailable(_))
        ));
        let status = client.wallet_native_create(true, false).unwrap();
        assert_eq!(status["protection"], "software");
        let h = client.hold();
        let made = key(&h, wallet(), "desktop").await.unwrap();
        let public_key = hex::decode(made["key"]["public_key"].as_str().unwrap()).unwrap();
        assert_eq!(
            sign(&h, wallet(), &public_key, vec![Hash::new([1; 32])])
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(add_protector(&h, false).await, Err(Error::Forbidden));
    }

    #[tokio::test]
    async fn waiting_for_the_person_leaves_the_client_free() {
        let dir = TempDir::new();
        let (mut client, hardware) = with_hardware(&dir);
        client.wallet_native_create(false, false).unwrap();
        let (release, held) = std::sync::mpsc::channel();
        *hardware.held.lock().unwrap() = Some(held);
        let shared = Arc::new(Shared(Mutex::new(client)));
        let prompt = tokio::spawn({
            let shared = shared.clone();
            async move { key(&*shared, wallet(), "phone").await }
        });
        while hardware.held.lock().unwrap().is_some() {
            tokio::task::yield_now().await;
        }
        // The prompt is open; other calls still reach the client.
        let status = shared.with(|c| c.wallet_native_status(None)).await.unwrap();
        assert_eq!(status["created"], true);
        release.send(()).unwrap();
        prompt.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_prompt_cannot_finish_after_locking_and_unlocking_the_same_workspace() {
        let dir = TempDir::new();
        let (mut client, hardware) = with_hardware(&dir);
        client.wallet_native_create(false, false).unwrap();
        let (release, held) = std::sync::mpsc::channel();
        *hardware.held.lock().unwrap() = Some(held);
        let shared = Arc::new(Shared(Mutex::new(client)));
        let prompt = tokio::spawn({
            let shared = shared.clone();
            async move { key(&*shared, wallet(), "phone").await }
        });
        while hardware.held.lock().unwrap().is_some() {
            tokio::task::yield_now().await;
        }
        shared
            .with(|c| {
                c.lock();
                c.keys = Some(c.workspace.unlock(&crate::testing::SECRET)?);
                Ok(())
            })
            .await
            .unwrap();
        release.send(()).unwrap();
        assert_eq!(prompt.await.unwrap(), Err(Error::Locked));
        assert!(
            shared
                .with(|c| c.native_key(wallet()))
                .await
                .unwrap()
                .is_none()
        );
    }

    struct Shared(Mutex<Client>);

    impl Hold for Shared {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut *self.0.lock().await)
        }
    }
}
