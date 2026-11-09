//! First-party login against real Wasm and IC ingress verification.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use candid::{CandidType, Decode, Encode, Principal};
use cbor2::Value;
use dmsg_protocol::{passkey::hardware_der, sha256};
use dmsg_types::{auth::*, *};
use ic_agent::{
    Agent, Identity,
    agent::EnvelopeContent,
    identity::{BasicIdentity, DelegatedIdentity, Delegation, Signature, SignedDelegation},
};
use p256::ecdsa::{SigningKey, signature::Signer};
use pocket_ic::{PocketIc, PocketIcBuilder};
use serde::de::DeserializeOwned;
use serde_bytes::ByteBuf;
use std::{path::PathBuf, time::Duration};

fn wasm() -> Vec<u8> {
    let dir = std::env::var_os("DMSG_WASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release")
        });
    std::fs::read(dir.join("dmsg_auth.wasm")).expect("build dmsg_auth Wasm first")
}

#[derive(Clone)]
struct Device(SigningKey);

impl Device {
    fn new(n: u8) -> Self {
        Self(SigningKey::from_slice(&[n; 32]).unwrap())
    }

    fn der(&self) -> Vec<u8> {
        hardware_der(self.0.verifying_key().to_sec1_point(false).as_bytes()).unwrap()
    }
}

impl Identity for Device {
    fn sender(&self) -> std::result::Result<Principal, String> {
        Ok(Principal::self_authenticating(self.der()))
    }

    fn public_key(&self) -> Option<Vec<u8>> {
        Some(self.der())
    }

    fn sign(&self, content: &EnvelopeContent) -> std::result::Result<Signature, String> {
        self.sign_arbitrary(&content.to_request_id().signable())
    }

    fn sign_arbitrary(&self, bytes: &[u8]) -> std::result::Result<Signature, String> {
        let sig: p256::ecdsa::Signature = self.0.sign(bytes);
        Ok(Signature {
            public_key: Some(self.der()),
            signature: Some(sig.to_bytes().to_vec()),
            delegations: None,
        })
    }
}

fn response(challenge: &Hash, register: bool) -> PasskeyResponse {
    let key = SigningKey::from_slice(&[42; 32]).unwrap();
    let client = serde_json::to_vec(
        &serde_json::json!({"type": if register {"webauthn.create"} else {"webauthn.get"},
        "challenge": URL_SAFE_NO_PAD.encode(challenge.as_slice()), "origin": PASSKEY_ORIGIN}),
    )
    .unwrap();
    let mut data = [
        sha256(PASSKEY_RP.as_bytes()).as_slice(),
        &[if register { 0x45 } else { 5 }],
        &[0; 4],
    ]
    .concat();
    let signature = if register {
        let public = key.verifying_key().to_sec1_point(false);
        let int = |n: i64| Value::Integer(n.into());
        let cose = Value::Map(vec![
            (int(1), int(2)),
            (int(3), int(-7)),
            (int(-1), int(1)),
            (int(-2), Value::Bytes(public.as_bytes()[1..33].to_vec())),
            (int(-3), Value::Bytes(public.as_bytes()[33..].to_vec())),
        ]);
        data.extend([0; 16]);
        data.extend([0, 3, 1, 2, 3]);
        data.extend(cbor2::to_vec(&cose).unwrap());
        data = cbor2::to_vec(&Value::Map(vec![
            (Value::Text("fmt".into()), Value::Text("none".into())),
            (Value::Text("authData".into()), Value::Bytes(data)),
            (Value::Text("attStmt".into()), Value::Map(vec![])),
        ]))
        .unwrap();
        vec![]
    } else {
        let sig: p256::ecdsa::Signature =
            key.sign(&[data.as_slice(), sha256(&client).as_slice()].concat());
        sig.to_der().as_bytes().to_vec()
    };
    PasskeyResponse {
        credential_id: vec![1, 2, 3].into(),
        client_data_json: client.into(),
        authenticator_data: data.into(),
        signature: signature.into(),
    }
}

fn limits() -> AuthLimits {
    AuthLimits {
        max_identities: 100,
        daily_registrations: 100,
        requests_per_minute: 1000,
        renewals_per_minute: 1000,
        max_challenges: 32,
    }
}

struct Fixture {
    ic: PocketIc,
    auth: Principal,
    controller: Principal,
}

impl Fixture {
    fn new() -> Self {
        let ic = PocketIcBuilder::new()
            .with_initial_time((std::time::SystemTime::now() - Duration::from_secs(120)).into())
            .with_nns_subnet()
            .with_application_subnet()
            .build();
        let controller = Principal::from_slice(&[9; 29]);
        let auth = ic.create_canister_with_settings(Some(controller), None);
        ic.add_cycles(auth, 100_000_000_000_000);
        ic.install_canister(
            auth,
            wasm(),
            Encode!(&AuthInit {
                governance: controller,
                admission_key: None,
                limits: limits()
            })
            .unwrap(),
            Some(controller),
        );
        let f = Self {
            ic,
            auth,
            controller,
        };
        for _ in 0..10 {
            if f.stats().ready {
                return f;
            }
            f.ic.tick();
        }
        panic!("auth salt not ready")
    }

    fn update<T: CandidType + DeserializeOwned>(
        &self,
        sender: Principal,
        method: &str,
        args: Vec<u8>,
    ) -> T {
        let res = self
            .ic
            .update_call(self.auth, sender, method, args)
            .unwrap();
        Decode!(&res, T).unwrap()
    }

    fn stats(&self) -> AuthStats {
        let res = self
            .ic
            .query_call(
                self.auth,
                Principal::anonymous(),
                "auth_stats",
                Encode!().unwrap(),
            )
            .unwrap();
        Decode!(&res, AuthStats).unwrap()
    }

    fn challenge(&self, device: &Device, action: AuthAction) -> AuthChallenge {
        self.update::<Result<AuthChallenge>>(
            device.sender().unwrap(),
            "prepare_challenge",
            Encode!(&ChallengeRequest {
                public_key: device.der().into(),
                action
            })
            .unwrap(),
        )
        .unwrap()
    }

    fn delegation(&self, device: &Device) -> Result<AuthDelegation> {
        let res = self
            .ic
            .query_call(
                self.auth,
                Principal::anonymous(),
                "get_delegation",
                Encode!(&ByteBuf::from(device.der())).unwrap(),
            )
            .unwrap();
        Decode!(&res, Result<AuthDelegation>).unwrap()
    }

    fn register(&self, device: &Device) -> AuthDelegation {
        let c = self.challenge(device, AuthAction::Register);
        self.update::<Result<u64>>(
            device.sender().unwrap(),
            "register",
            Encode!(&RegisterPasskey {
                challenge: c.challenge,
                response: response(&c.challenge, true),
                admission: None
            })
            .unwrap(),
        )
        .unwrap();
        self.delegation(device).unwrap()
    }

    fn verify(&self, delegation: &AuthDelegation) {
        let msg = Delegation {
            pubkey: delegation.public_key.to_vec(),
            expiration: delegation.expiration,
            targets: None,
            permissions: None,
        };
        ic_auth_verifier::verify_canister_sig(
            &msg.signable(),
            &delegation.signature,
            &delegation.user_key,
            &self.ic.root_key().unwrap(),
            u128::from(self.ic.get_time().as_nanos_since_unix_epoch()),
            None,
        )
        .unwrap();
    }
}

#[test]
fn upgrade_renewal_replay_and_session_budget() {
    let f = Fixture::new();
    let first = Device::new(1);
    let original = f.register(&first);
    f.verify(&original);
    let stable = f.stats().stable_pages;
    // Pending challenges and signature witnesses disappear, stable grants do not.
    let pending = f.challenge(&Device::new(2), AuthAction::Login);
    f.ic.tick();
    f.ic.upgrade_canister(f.auth, wasm(), Encode!().unwrap(), Some(f.controller))
        .unwrap();
    assert_eq!(f.stats().stable_pages, stable);
    assert_eq!(f.stats().sessions, 1);
    assert_eq!(f.stats().challenges, 0);
    assert!(f.delegation(&first).is_err());
    f.ic.advance_time(Duration::from_secs(180 * 24 * 3600));
    f.ic.tick();
    let before = f.ic.cycle_balance(f.auth);
    f.update::<Result<u64>>(first.sender().unwrap(), "renew", Encode!().unwrap())
        .unwrap();
    let renew_cycles = before - f.ic.cycle_balance(f.auth);
    let renewed = f.delegation(&first).unwrap();
    f.verify(&renewed);
    assert_eq!(original.user_key, renewed.user_key);
    assert!(renewed.expiration > original.expiration);
    assert!(
        f.update::<Result<u64>>(
            Device::new(2).sender().unwrap(),
            "login",
            Encode!(&AuthenticatePasskey {
                challenge: pending.challenge,
                response: response(&pending.challenge, false)
            })
            .unwrap()
        )
        .is_err()
    );
    for n in 2..=16 {
        let device = Device::new(n);
        let c = f.challenge(&device, AuthAction::Login);
        f.update::<Result<u64>>(
            device.sender().unwrap(),
            "login",
            Encode!(&AuthenticatePasskey {
                challenge: c.challenge,
                response: response(&c.challenge, false)
            })
            .unwrap(),
        )
        .unwrap();
    }
    let outsider = Device::new(17);
    let c = f.challenge(&outsider, AuthAction::Login);
    assert_eq!(
        f.update::<Result<u64>>(
            outsider.sender().unwrap(),
            "login",
            Encode!(&AuthenticatePasskey {
                challenge: c.challenge,
                response: response(&c.challenge, false)
            })
            .unwrap()
        ),
        Err(Error::SessionLimitExceeded)
    );
    let mut throttled = limits();
    throttled.requests_per_minute = 1;
    f.update::<Result<()>>(
        f.controller,
        "admin_set_limits",
        Encode!(&throttled).unwrap(),
    )
    .unwrap();
    f.ic.advance_time(Duration::from_secs(60));
    f.ic.tick();
    let c = f.challenge(&outsider, AuthAction::Login);
    assert_eq!(
        f.update::<Result<u64>>(
            outsider.sender().unwrap(),
            "login",
            Encode!(&AuthenticatePasskey {
                challenge: c.challenge,
                response: response(&c.challenge, false)
            })
            .unwrap()
        ),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(f.stats().sessions, 16);
    f.update::<Result<()>>(
        f.controller,
        "admin_set_limits",
        Encode!(&limits()).unwrap(),
    )
    .unwrap();
    let c = f.challenge(&outsider, AuthAction::LogoutAll);
    let proof = AuthenticatePasskey {
        challenge: c.challenge,
        response: response(&c.challenge, false),
    };
    f.update::<Result<()>>(
        outsider.sender().unwrap(),
        "logout_all",
        Encode!(&proof).unwrap(),
    )
    .unwrap();
    assert_eq!(f.stats().sessions, 0);
    assert!(
        f.update::<Result<()>>(
            outsider.sender().unwrap(),
            "logout_all",
            Encode!(&proof).unwrap()
        )
        .is_err()
    );
    assert!(
        f.ic.update_call(f.auth, first.sender().unwrap(), "renew", Encode!().unwrap())
            .is_err()
    );
    println!(
        "auth PocketIC: renewal update cycles={renew_cycles}, stable_pages_after_1_identity_16_sessions={}, canister_wasm_bytes={}",
        f.stats().stable_pages,
        wasm().len()
    );
}

#[test]
fn p256_ingress_and_targeted_child_delegation_are_accepted_by_ic() {
    let mut f = Fixture::new();
    let device = Device::new(1);
    let root = f.register(&device);
    let identity = Principal::self_authenticating(&root.user_key);
    // A second canister authorizes only the login principal, proving the entire
    // canister -> hardware -> memory chain is checked by IC ingress validation.
    let business = f.ic.create_canister_with_settings(Some(f.controller), None);
    f.ic.add_cycles(business, 10_000_000_000_000);
    f.ic.install_canister(
        business,
        wasm(),
        Encode!(&AuthInit {
            governance: identity,
            admission_key: None,
            limits: limits()
        })
        .unwrap(),
        Some(f.controller),
    );
    f.ic.advance_time(Duration::from_secs(61));
    let url = f.ic.make_live(None).to_string();
    let root_key = f.ic.root_key().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let hardware = Agent::builder()
            .with_url(&url)
            .with_identity(device.clone())
            .build()
            .unwrap();
        hardware.set_root_key(root_key.clone());
        let res = hardware
            .update(&f.auth, "renew")
            .with_arg(Encode!().unwrap())
            .call_and_wait()
            .await
            .unwrap();
        Decode!(&res, Result<u64>).unwrap().unwrap();
        let ephemeral = BasicIdentity::from_raw_key(&[77; 32]);
        let parent = Delegation {
            pubkey: device.der(),
            expiration: root.expiration,
            targets: None,
            permissions: None,
        };
        let child = Delegation {
            pubkey: ephemeral.public_key().unwrap(),
            expiration: root.expiration,
            targets: Some(vec![business]),
            permissions: None,
        };
        let signature = device
            .sign_arbitrary(&child.signable())
            .unwrap()
            .signature
            .unwrap();
        let delegated = DelegatedIdentity::new_unchecked(
            root.user_key.to_vec(),
            Box::new(ephemeral),
            vec![
                SignedDelegation {
                    delegation: parent,
                    signature: root.signature.to_vec(),
                },
                SignedDelegation {
                    delegation: child,
                    signature,
                },
            ],
        );
        let agent = Agent::builder()
            .with_url(&url)
            .with_identity(delegated)
            .build()
            .unwrap();
        agent.set_root_key(root_key);
        assert_eq!(agent.get_principal().unwrap(), identity);
        let res = agent
            .update(&business, "admin_set_limits")
            .with_arg(Encode!(&limits()).unwrap())
            .call_and_wait()
            .await
            .unwrap();
        Decode!(&res, Result<()>).unwrap().unwrap();
        assert!(
            agent
                .update(&f.auth, "logout")
                .with_arg(Encode!().unwrap())
                .call_and_wait()
                .await
                .is_err(),
            "child targets must be enforced"
        );
    });
}
