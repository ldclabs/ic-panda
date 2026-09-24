//! Fixed development fixtures; no real identities, balances or rate policy.
use dmsg_protocol::{canonical, integration::*};
use dmsg_types::integration::*;
#[path = "../tests/support/app_action.rs"]
mod action_fixtures;
#[path = "../tests/support/integration.rs"]
mod fixtures;
mod support;
use fixtures::*;
use support::vector;

fn main() {
    let q = quote_panda(&offer(), &app(), &product(), &rate(), NOW).unwrap();
    let mut values = vec![
        vector("app", canonical(&app())),
        vector("product", canonical(&product())),
        vector(
            "project_offer",
            commitment_bytes("dmsg/commerce/offer/v2", &offer()),
        ),
        vector(
            "account_offer",
            commitment_bytes("dmsg/commerce/offer/v2", &account_offer()),
        ),
        vector(
            "authentication",
            commitment_bytes("dmsg/authentication/request/v1", &authentication()),
        ),
        vector(
            "approval",
            commitment_bytes("dmsg/application/approval/v1", &approval()),
        ),
        vector(
            "panda_quote",
            commitment_bytes("dmsg/commerce/panda-quote/v2", &q),
        ),
        vector(
            "cash_a",
            commitment_bytes("dmsg/commerce/cash-quote/v2", &cash(principal(6))),
        ),
        vector(
            "cash_b",
            commitment_bytes("dmsg/commerce/cash-quote/v2", &cash(principal(7))),
        ),
        vector(
            "decision",
            commitment_bytes("dmsg/commerce/decision/v2", &decision()),
        ),
        vector("max_u128", canonical(&u128::MAX)),
        vector("commerce_version", canonical(&COMMERCE_VERSION)),
    ];
    for (index, command) in action_fixtures::commands().into_iter().enumerate() {
        let mut action = action_fixtures::action();
        action.command = command;
        action.input_hash = dmsg_protocol::app_action::action_input_hash(&action.command);
        values.push(vector(
            &format!("app_action_{index}"),
            commitment_bytes("dmsg/app-action/v1", &action),
        ));
    }
    {
        use dmsg_protocol::*;
        use dmsg_types::{cose::*, *};
        use ed25519_dalek::{Signer, SigningKey};
        let action = action_fixtures::action();
        let account = action.signing_account;
        let key = SigningKey::from_bytes(&[7; 32]);
        let public = key.verifying_key().to_bytes();
        let fingerprint =
            key_thumbprint(&public_cose_key(&Algorithm::Ed25519, &[], &public).unwrap()).unwrap();
        let approval = Approval {
            device_id: Hash::new([1; 32]),
            security_epoch: 1,
            sequence: 0,
            request_id: execution_request_id(&account, 1, Hash::new([1; 32]), 0),
            expires_at: action.expires_at_ms,
            signature: Default::default(),
        };
        let request = AppActionSignRequest {
            account_id: account,
            key: SigningKeyRef {
                algorithm: SigningAlgorithm::Ed25519,
                kid: fingerprint.to_vec().into(),
                public_key_fingerprint: fingerprint,
            },
            issuer: account_issuer("https://example.test/u/", &account),
            action,
            max_cycles: 100_000_000_000,
            approval,
        };
        let execution = request.clone().into_execution().unwrap();
        let ExecutionKind::Sign { to_be_signed, .. } = &execution.kind else {
            panic!()
        };
        let artifact = finish_cose(
            to_be_signed,
            &public,
            key.sign(to_be_signed).to_bytes().to_vec(),
        )
        .unwrap();
        values.push(vector("app_action_request", canonical(&request)));
        values.push(vector("app_action_artifact", canonical(&artifact)));
        values.push(vector("app_action_signing_bytes", to_be_signed.to_vec()));
        values.push(vector(
            "app_action_approval",
            canonical(&execution.approval_message(principal(1))),
        ));
    }
    println!("{}", serde_json::to_string_pretty(&values).unwrap());
}
