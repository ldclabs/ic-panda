//! ic_signin_with: wallet sign-in, certified delegations and ingress filtering.
use base64::{Engine, engine::general_purpose::URL_SAFE};
use candid::{CandidType, Decode, Deserialize, Encode, Principal};
use ed25519_dalek::{Signer, SigningKey};
use ic_auth_types::{SignInResponse, SignedDelegation};
use ic_auth_verifier::{keccak256, sha3_256, verify_delegation_chain};
use pocket_ic::{PocketIc, PocketIcBuilder, RejectResponse, common::rest::BlobCompression};
use serde::de::DeserializeOwned;
use serde_bytes::ByteBuf;
use std::{collections::BTreeMap, path::PathBuf};

const DOMAIN: &str = "example.com";
const STATEMENT: &str = "Sign in to example.com";
const SESSION_MS: u64 = 3_600_000;
const ED25519_DER_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

#[derive(CandidType)]
enum CanisterArgs {
    Init(State),
    Upgrade(UpgradeArgs),
}

#[derive(CandidType)]
struct UpgradeArgs {
    session_expires_in_ms: Option<u64>,
    governance_canister: Option<Principal>,
}

#[derive(CandidType, Debug, Deserialize, PartialEq)]
struct State {
    domains: BTreeMap<String, String>,
    statement: String,
    session_expires_in_ms: u64,
    governance_canister: Option<Principal>,
}

fn wasm() -> Vec<u8> {
    let directory = std::env::var_os("DMSG_WASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release")
        });
    std::fs::read(directory.join("ic_signin_with.wasm")).expect("build ic_signin_with Wasm first")
}

fn session_der(key: &SigningKey) -> Vec<u8> {
    let mut der = ED25519_DER_PREFIX.to_vec();
    der.extend(key.verifying_key().to_bytes());
    der
}

struct Fixture {
    ic: PocketIc,
    id: Principal,
    controller: Principal,
}

impl Fixture {
    fn new() -> Self {
        // The NNS subnet provides the root key; the canister lands on the
        // application subnet, which charges cycles like mainnet.
        let ic = PocketIcBuilder::new()
            .with_nns_subnet()
            .with_application_subnet()
            .build();
        let controller = Principal::from_slice(&[9; 29]);
        let id = ic.create_canister_with_settings(Some(controller), None);
        ic.add_cycles(id, 100_000_000_000_000);
        let args = Some(CanisterArgs::Init(State {
            session_expires_in_ms: SESSION_MS,
            governance_canister: None,
            statement: STATEMENT.to_string(),
            domains: BTreeMap::from([(DOMAIN.to_string(), format!("https://{DOMAIN}"))]),
        }));
        ic.install_canister(id, wasm(), Encode!(&args).unwrap(), Some(controller));
        Self { ic, id, controller }
    }

    fn now_ms(&self) -> u64 {
        self.ic.get_time().as_nanos_since_unix_epoch() / 1_000_000
    }

    fn query<T: CandidType + DeserializeOwned>(&self, method: &str, args: Vec<u8>) -> T {
        let res = self
            .ic
            .query_call(self.id, Principal::anonymous(), method, args)
            .unwrap();
        Decode!(&res, T).unwrap()
    }

    fn update<T: CandidType + DeserializeOwned>(
        &self,
        sender: Principal,
        method: &str,
        args: Vec<u8>,
    ) -> Result<T, RejectResponse> {
        let res = self.ic.update_call(self.id, sender, method, args)?;
        Ok(Decode!(&res, T).unwrap())
    }

    /// Runs a round first: the instructions of the last install rate-limit an
    /// install_code sent right after it.
    fn upgrade(&self, args: Option<CanisterArgs>) -> Result<(), RejectResponse> {
        self.ic.tick();
        self.ic.upgrade_canister(
            self.id,
            wasm(),
            Encode!(&args).unwrap(),
            Some(self.controller),
        )
    }

    fn info(&self) -> State {
        self.query::<Result<State, String>>("info", Encode!().unwrap())
            .unwrap()
    }

    /// Fetch the certified delegation and check it chains to `session` under
    /// PocketIC's root key, as an agent would before using it.
    fn check_delegation(&self, res: &SignInResponse, session: &[u8], now_ms: u64) {
        let signed = self
            .query::<Result<SignedDelegation, String>>(
                "get_delegation",
                Encode!(
                    &ByteBuf::from(res.seed.to_vec()),
                    &ByteBuf::from(session.to_vec()),
                    &res.expiration
                )
                .unwrap(),
            )
            .unwrap();
        let root_key = self.ic.root_key().unwrap();
        verify_delegation_chain(
            &res.user_key,
            session,
            &[signed.into()],
            now_ms,
            Some(&root_key),
        )
        .unwrap();
    }
}

#[test]
fn solana_signatures_authorize_only_their_session_key() {
    let f = Fixture::new();
    let wallet = SigningKey::from_bytes(&[1; 32]);
    let address = bs58::encode(wallet.verifying_key().to_bytes()).into_string();
    let session = session_der(&SigningKey::from_bytes(&[3; 32]));
    let now = f.now_ms();

    let message = f
        .query::<Result<String, String>>(
            "get_sign_in_with_solana_message",
            Encode!(&DOMAIN, &address, &now, &ByteBuf::from(session.clone())).unwrap(),
        )
        .unwrap();
    // Clients can check that the message commits to their session key.
    let nonce = sha3_256(&[now.to_be_bytes().as_slice(), &session].concat());
    assert!(message.contains(&format!("\nNonce: {}\n", URL_SAFE.encode(&nonce[..12]))));
    let message_sig = ByteBuf::from(wallet.sign(message.as_bytes()).to_bytes().to_vec());

    let sign_in = |session: &[u8]| {
        f.update::<Result<SignInResponse, String>>(
            Principal::anonymous(),
            "sign_in_with_solana",
            Encode!(
                &DOMAIN,
                &address,
                &now,
                &ByteBuf::from(session.to_vec()),
                &message_sig
            )
            .unwrap(),
        )
        .unwrap()
    };
    let res = sign_in(&session).unwrap();
    assert_eq!(res.expiration, (now + SESSION_MS) * 1_000_000);
    assert_eq!(res.seed.as_slice(), wallet.verifying_key().as_bytes());
    f.check_delegation(&res, &session, now);

    // Whoever sees the signature can't bind it to a key of their own.
    let other = session_der(&SigningKey::from_bytes(&[4; 32]));
    assert!(sign_in(&other).unwrap_err().contains("verification failed"));
}

#[test]
fn ethereum_signatures_authorize_only_their_session_key() {
    let f = Fixture::new();
    let wallet = k256::ecdsa::SigningKey::from_slice(&[2; 32]).unwrap();
    let point = wallet.verifying_key().to_sec1_point(false);
    let address = format!(
        "0x{}",
        hex::encode(&keccak256(&point.as_bytes()[1..])[12..])
    );
    let session = session_der(&SigningKey::from_bytes(&[3; 32]));
    let now = f.now_ms();

    let message = f
        .query::<Result<String, String>>(
            "get_sign_in_with_ethereum_message",
            Encode!(
                &DOMAIN,
                &address,
                &1u32,
                &now,
                &ByteBuf::from(session.clone())
            )
            .unwrap(),
        )
        .unwrap();
    let digest = keccak256(
        &[
            format!("\x19Ethereum Signed Message:\n{}", message.len()).as_bytes(),
            message.as_bytes(),
        ]
        .concat(),
    );
    let (signature, recovery_id) = wallet.sign_prehash_recoverable(&digest);
    let mut message_sig = signature.to_bytes().to_vec();
    message_sig.push(recovery_id.to_byte() + 27);
    let message_sig = ByteBuf::from(message_sig);

    let sign_in = |session: &[u8]| {
        f.update::<Result<SignInResponse, String>>(
            Principal::anonymous(),
            "sign_in_with_ethereum",
            Encode!(
                &DOMAIN,
                &address,
                &1u32,
                &now,
                &ByteBuf::from(session.to_vec()),
                &message_sig
            )
            .unwrap(),
        )
        .unwrap()
    };
    let res = sign_in(&session).unwrap();
    assert_eq!(res.expiration, (now + SESSION_MS) * 1_000_000);
    f.check_delegation(&res, &session, now);

    let other = session_der(&SigningKey::from_bytes(&[4; 32]));
    assert_eq!(
        sign_in(&other).unwrap_err(),
        "signature does not match expected address"
    );
}

#[test]
fn inspect_message_refuses_ingress_nobody_should_pay_for() {
    let f = Fixture::new();
    let anonymous = Principal::anonymous();
    let balance = f.ic.cycle_balance(f.id);

    // Queries sent as update calls, oversized sign-ins and admin calls from
    // anyone but a controller never reach the canister, so it pays nothing.
    assert!(
        f.update::<Result<Principal, String>>(anonymous, "whoami", Encode!().unwrap())
            .is_err()
    );
    let session = ByteBuf::from(vec![1; 44]);
    let message_sig = ByteBuf::from(vec![0; 64]);
    let oversized = "x".repeat(2_000);
    assert!(
        f.update::<Result<SignInResponse, String>>(
            anonymous,
            "sign_in_with_solana",
            Encode!(&oversized, &"", &0u64, &session, &message_sig).unwrap(),
        )
        .is_err()
    );
    let domain = ("other.example.com", "https://other.example.com");
    assert!(
        f.update::<Result<(), String>>(
            anonymous,
            "admin_update_domain",
            Encode!(&domain.0, &domain.1).unwrap()
        )
        .is_err()
    );
    assert_eq!(f.ic.cycle_balance(f.id), balance);

    // A bounded sign-in is accepted and answered by the method itself.
    let err = f
        .update::<Result<SignInResponse, String>>(
            anonymous,
            "sign_in_with_solana",
            Encode!(&DOMAIN, &"1", &0u64, &session, &message_sig).unwrap(),
        )
        .unwrap()
        .unwrap_err();
    assert_eq!(err, "invalid solana address");
    assert!(f.ic.cycle_balance(f.id) < balance);

    f.update::<Result<(), String>>(
        f.controller,
        "admin_update_domain",
        Encode!(&domain.0, &domain.1).unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(f.info().domains.len(), 2);
}

#[test]
fn upgrades_keep_the_config_and_refuse_the_memory_manager_layout() {
    let f = Fixture::new();
    let governance = Principal::from_slice(&[7; 29]);
    let args = Some(CanisterArgs::Upgrade(UpgradeArgs {
        session_expires_in_ms: None,
        governance_canister: Some(governance),
    }));
    f.upgrade(args).unwrap();
    assert_eq!(
        f.info(),
        State {
            domains: BTreeMap::from([(DOMAIN.to_string(), format!("https://{DOMAIN}"))]),
            statement: STATEMENT.to_string(),
            session_expires_in_ms: SESSION_MS,
            governance_canister: Some(governance),
        }
    );

    // Releases up to v3.0.1 start stable memory with a MemoryManager header,
    // which the new cell would silently replace with an empty config.
    let mut image = vec![0; 65_536];
    image[..3].copy_from_slice(b"MGR");
    f.ic.set_stable_memory(f.id, image, BlobCompression::NoCompression);
    let err = f.upgrade(None).unwrap_err();
    assert!(err.reject_message.contains("reinstall"), "{err}");
}
