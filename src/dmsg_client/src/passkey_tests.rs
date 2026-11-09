use super::*;
use crate::{
    secure::SecureStorage,
    testing::{self, MemoryStorage, TempDir},
};
use p256::ecdsa::{SigningKey, signature::Signer};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};

struct Hardware;

impl HardwareAnchor for Hardware {
    fn create(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        Ok((
            vec![7],
            key.verifying_key().to_sec1_point(false).as_bytes().to_vec(),
        ))
    }

    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]> {
        self.sign_message(handle, message.as_slice())
    }

    fn sign_message(&self, _handle: &[u8], message: &[u8]) -> Result<[u8; 64]> {
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        let signature: p256::ecdsa::Signature = key.sign(message);
        Ok(signature.to_bytes().into())
    }
}

struct Shared(Mutex<Client>);

impl Hold for Shared {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        step(&mut *self.0.lock().await)
    }
}

fn client(dir: &TempDir, secure: Arc<MemoryStorage>) -> Client {
    let mut c = testing::unlocked(&dir.0, false, Some(secure));
    let auth = Principal::from_slice(&[1, 1]);
    c.config.auth = Some(auth);
    c.anchor = Some(Arc::new(Hardware));
    let mut key = DeviceLogin::new(auth, &Hardware).unwrap();
    let user_key = ic_auth_verifier::CanisterSigPublicKey::new(auth, vec![9; 32]).to_der();
    key.parent = Some(AuthDelegation {
        user_key: user_key.into(),
        public_key: key.public.clone(),
        expiration: 1,
        signature: vec![].into(),
    });
    key.targets = vec![auth];
    c.passkey = Some(key);
    c
}

async fn reject(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = vec![0; 8192];
    let size = stream.read(&mut bytes).await.unwrap();
    // A pooled connection can close without a request; it counts as none.
    let path = String::from_utf8_lossy(&bytes[..size])
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    stream
        .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    path
}

#[tokio::test]
async fn registration_preflight_failure_never_starts_a_creation_challenge() {
    let dir = TempDir::new();
    let mut c = client(&dir, Arc::new(MemoryStorage::default()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    c.config.environment = Environment::Production;
    c.config.ic_host = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        reject(&mut stream).await
    });
    assert!(matches!(
        PasskeySession::start(
            c.config,
            Arc::new(Hardware),
            AuthAction::Register,
            dmsg_protocol::login::IOS_CALLBACK.into(),
            None
        )
        .await,
        Err(Error::Unavailable(_))
    ));
    assert!(
        server.await.unwrap().contains("/query "),
        "readiness is checked before prepare_challenge"
    );
}

#[tokio::test]
async fn concurrent_failure_is_shared_and_retry_keeps_local_content() {
    let dir = TempDir::new();
    let mut c = client(&dir, Arc::new(MemoryStorage::default()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    c.config.ic_host = format!("http://{}", listener.local_addr().unwrap());
    let h = Shared(Mutex::new(c));
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let server = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let path = reject(&mut stream).await;
            // Query signature verification can also read the subnet certificate.
            if path.contains("/call ") || path.contains("/query ") {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    let outcomes =
        futures_util::future::join_all((0..12).map(|_| ensure_login(&h, MIN_VALIDITY))).await;
    assert!(outcomes.iter().all(|v| v.is_err()));
    assert!(outcomes.iter().all(|v| v == &outcomes[0]));
    assert_eq!(
        requests.load(Ordering::Relaxed),
        2,
        "one update and recovery query, shared by waiters"
    );
    h.with(|c| {
        assert!(c.keys.is_some() && c.root.is_some() && c.renewable());
        Ok(())
    })
    .await
    .unwrap();
    assert!(ensure_login(&h, MIN_VALIDITY).await.is_err());
    assert_eq!(
        requests.load(Ordering::Relaxed),
        4,
        "a later operation can retry"
    );
    server.abort();
}

#[tokio::test]
async fn a_current_login_does_not_wait_for_a_renewal_in_progress() {
    let dir = TempDir::new();
    let mut c = client(&dir, Arc::new(MemoryStorage::default()));
    let login = crate::login::kept_for_tests(3, now_ms() + DAY)
        .delegated()
        .unwrap();
    c.login = Some(crate::client::Login {
        ic: Ic::connect(&c.config, login.identity).await.unwrap(),
        principal: login.principal,
        expires_at: login.expires_at,
        kept: login.kept,
    });
    // Whole-client operations hold the client while asking; a renewal holding
    // the gate would wait for that client to finish.
    let gate = c.login_renewal.clone();
    let _renewing = gate.lock().await;
    let h = Shared(Mutex::new(c));
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        ensure_login(&h, MIN_VALIDITY),
    )
    .await
    .expect("no wait on the gate")
    .unwrap();
}

#[tokio::test]
async fn lock_during_network_rejects_the_late_result() {
    let dir = TempDir::new();
    let mut c = client(&dir, Arc::new(MemoryStorage::default()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    c.config.ic_host = format!("http://{}", listener.local_addr().unwrap());
    let h = Arc::new(Shared(Mutex::new(c)));
    let task = {
        let h = h.clone();
        tokio::spawn(async move { ensure_login(h.as_ref(), MIN_VALIDITY).await })
    };
    let (mut request, _) = listener.accept().await.unwrap();
    // This lock is available while the request is pending.
    h.with(|c| {
        c.lock();
        Ok(())
    })
    .await
    .unwrap();
    reject(&mut request).await;
    let (mut query, _) = listener.accept().await.unwrap();
    reject(&mut query).await;
    assert_eq!(task.await.unwrap(), Err(Error::Locked));
    h.with(|c| {
        assert!(c.login.is_none() && c.keys.is_none());
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn completion_retry_retrieves_the_same_grant_without_registering_again() {
    let dir = TempDir::new();
    let mut c = client(&dir, Arc::new(MemoryStorage::default()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    c.config.environment = Environment::Production;
    c.config.ic_host = format!("http://{}", listener.local_addr().unwrap());
    let updates = Arc::new(AtomicUsize::new(0));
    let queries = Arc::new(AtomicUsize::new(0));
    let counts = (updates.clone(), queries.clone());
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let path = reject(&mut stream).await;
            if path.contains("/call ") {
                counts.0.fetch_add(1, Ordering::Relaxed);
            }
            if path.contains("/query ") {
                counts.1.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    let mut session = PasskeySession {
        key: DeviceLogin::new(c.config.auth.unwrap(), &Hardware).unwrap(),
        config: c.config,
        hardware: Arc::new(Hardware),
        request: PasskeyRequest {
            protocol: PROTOCOL.into(),
            state: Hash::new([1; 32]),
            challenge: AuthChallenge {
                challenge: Hash::new([2; 32]),
                expires_at: now_ms() + MINUTE,
                action: AuthAction::Register,
            },
            next_url: dmsg_protocol::login::IOS_CALLBACK.into(),
            credential_id: None,
        },
        expected_credential: None,
        admission: None,
        submitted: None,
        confirmed: false,
        deadline: now_ms() + MINUTE,
    };
    let response = PasskeyResponse {
        credential_id: vec![3; 32].into(),
        client_data_json: vec![4].into(),
        authenticator_data: vec![5].into(),
        signature: vec![].into(),
    };
    for _ in 0..2 {
        assert!(matches!(
            session.complete(response.clone()).await,
            Err(Error::Unavailable(_))
        ));
    }
    assert_eq!(
        updates.load(Ordering::Relaxed),
        1,
        "a failed grant lookup never replays registration"
    );
    assert_eq!(
        queries.load(Ordering::Relaxed),
        2,
        "retry checks the original hardware grant"
    );
    let mut foreign = response.clone();
    foreign.credential_id[0] ^= 1;
    assert!(matches!(
        session.complete(foreign).await,
        Err(Error::IntegrityFailed)
    ));
    session.deadline = 0;
    assert!(matches!(
        session.complete(response).await,
        Err(Error::Expired)
    ));
    assert_eq!(updates.load(Ordering::Relaxed), 1);
    server.abort();
}

struct Person;

impl crate::secure::PresenceKey for Person {
    fn presence(&self) -> Option<crate::secure::Presence> {
        Some(crate::secure::Presence::Biometrics)
    }

    fn create(&self, _: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<crate::crypto::Key>)> {
        Ok((vec![3; 10], vec![3; 65], Zeroizing::new([5; 32])))
    }

    fn open(&self, _: &[u8], _: &[u8], _: bool) -> Result<Zeroizing<crate::crypto::Key>> {
        Ok(Zeroizing::new([5; 32]))
    }

    fn biometry_state(&self) -> Option<Hash> {
        None
    }

    fn confirm(&self, _: &str) -> Result<()> {
        Ok(())
    }
}

#[test]
fn capacity_reset_pins_the_failed_credential_and_preserves_the_remembered_login() {
    let dir = TempDir::new();
    let mut c = client(&dir, Arc::new(MemoryStorage::default()));
    c.set_presence_key(Arc::new(Person));
    c.passkey.as_mut().unwrap().credential = vec![11; 32].into();
    let prepared = c
        .prepare_login_keys(Zeroizing::new(testing::SECRET))
        .unwrap()
        .unwrap();
    let key = prepared.open().unwrap();
    c.apply_login_keys(prepared, &key, now_ms()).unwrap();
    let target = vec![22; 32];
    let reset = c
        .passkey_start(AuthAction::LogoutAll, Some(target.clone()))
        .unwrap();
    assert_eq!(
        reset.credential.as_ref().map(|v| v.as_ref()),
        Some(target.as_slice())
    );
    let retry = c
        .passkey_start(AuthAction::Login, Some(target.clone()))
        .unwrap();
    assert_eq!(retry.credential, reset.credential);
    assert_eq!(
        c.remembered_passkey().unwrap().unwrap().credential.as_ref(),
        &[11; 32]
    );
    let ordinary = c.passkey_start(AuthAction::LogoutAll, None).unwrap();
    assert_eq!(
        ordinary.credential.as_ref().map(|v| v.as_ref()),
        Some([11; 32].as_slice())
    );
    assert!(matches!(
        c.passkey_start(AuthAction::Register, Some(target.clone())),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        c.passkey_start(AuthAction::LogoutAll, Some(vec![])),
        Err(Error::InvalidInput(_))
    ));

    let state = Hash::new([7; 32]);
    let request = PasskeyRequest {
        protocol: PROTOCOL.into(),
        state,
        challenge: AuthChallenge {
            challenge: Hash::new([8; 32]),
            expires_at: now_ms() + MINUTE,
            action: AuthAction::LogoutAll,
        },
        next_url: dmsg_protocol::login::IOS_CALLBACK.into(),
        credential_id: reset.credential,
    };
    let session = test_session(reset.config, reset.hardware, request, vec![], None).unwrap();
    let mut callback = PasskeyCallback {
        protocol: PROTOCOL.into(),
        state,
        error: None,
        response: Some(PasskeyResponse {
            credential_id: vec![11; 32].into(),
            client_data_json: vec![].into(),
            authenticator_data: vec![].into(),
            signature: vec![].into(),
        }),
    };
    assert_eq!(
        session.accept(&canonical(&callback), now_ms()),
        Err(Error::Forbidden)
    );
    callback.response.as_mut().unwrap().credential_id = target.clone().into();
    assert_eq!(
        session
            .accept(&canonical(&callback), now_ms())
            .unwrap()
            .credential_id
            .as_ref(),
        target.as_slice()
    );
}

#[test]
fn passkey_login_binds_hardware_again_after_an_explicit_unbind() {
    let dir = TempDir::new();
    let secure = Arc::new(MemoryStorage::default());
    let mut c = client(&dir, secure.clone());
    c.set_presence_key(Arc::new(Person));
    // Unbinding a Passkey login signs out and records the decline.
    c.workspace.meta.bound_declined = true;
    let prepared = c
        .prepare_login_keys(Zeroizing::new(testing::SECRET))
        .unwrap()
        .unwrap();
    let key = prepared.open().unwrap();
    c.apply_login_keys(prepared, &key, now_ms()).unwrap();
    assert!(c.workspace.bound().unwrap() && !c.workspace.meta.bound_declined);
    assert!(secure.get(&c.passkey_slot().unwrap()).unwrap().is_some());
}

#[test]
fn provisional_and_unbound_workspaces_do_not_persist_login() {
    let dir = TempDir::new();
    let secure = Arc::new(MemoryStorage::default());
    let mut c = client(&dir, secure.clone());
    assert_eq!(c.remember_passkey(), Err(Error::Forbidden));
    assert!(secure.get(&c.passkey_slot().unwrap()).unwrap().is_none());
    c.forget_passkey().unwrap();
    assert!(!c.renewable());
    assert!(c.keys.is_some() && c.root.is_some());
}
