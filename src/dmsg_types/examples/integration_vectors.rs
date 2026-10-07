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
            canonical(&(1u8, "dmsg/commerce/offer/v2", &offer())),
        ),
        vector(
            "account_offer",
            canonical(&(1u8, "dmsg/commerce/offer/v2", &account_offer())),
        ),
        vector(
            "authentication",
            canonical(&(1u8, "dmsg/authentication/request/v1", &authentication())),
        ),
        vector(
            "approval",
            canonical(&(1u8, "dmsg/application/approval/v1", &approval())),
        ),
        vector(
            "panda_quote",
            canonical(&(1u8, "dmsg/commerce/panda-quote/v2", &q)),
        ),
        vector(
            "cash_a",
            canonical(&(1u8, "dmsg/commerce/cash-quote/v2", &cash(principal(6)))),
        ),
        vector(
            "cash_b",
            canonical(&(1u8, "dmsg/commerce/cash-quote/v2", &cash(principal(7)))),
        ),
        vector(
            "decision",
            canonical(&(1u8, "dmsg/commerce/decision/v2", &decision())),
        ),
        vector("max_u128", canonical(&u128::MAX)),
        vector("commerce_version", canonical(&COMMERCE_VERSION)),
    ];
    values.push(vector("action_app", canonical(&action_fixtures::app())));
    for (index, command) in action_fixtures::commands().into_iter().enumerate() {
        let mut action = action_fixtures::action();
        action.command = command;
        values.push(vector(
            &format!("app_action_{index}"),
            canonical(&(1u8, "dmsg/app-action/v1", &action)),
        ));
    }
    {
        use dmsg_protocol::*;
        use dmsg_types::*;
        use ed25519_dalek::{Signer, SigningKey};
        let action = action_fixtures::action();
        let account = action.signing_account;
        let key = SigningKey::from_bytes(&[7; 32]);
        let public: Hash = key.verifying_key().to_bytes().into();
        let mut request = AppActionAttestRequest {
            account_id: account,
            issuer: account_issuer("https://example.test/u/", &account),
            action: action.clone(),
            signature: Default::default(),
            approval: Approval {
                device_id: Hash::new([1; 32]),
                security_epoch: 1,
                sequence: 0,
                request_id: execution_request_id(&account, 1, Hash::new([1; 32]), 0),
                expires_at: action.expires_at_ms,
                signature: Default::default(),
            },
        };
        let statement = app_action_statement(&request).unwrap();
        let prepared = prepare_attestation(&statement, &public).unwrap();
        request.signature = key.sign(&prepared.to_be_signed).to_bytes().into();
        let artifact = parse_signing_input(&prepared.to_be_signed)
            .unwrap()
            .into_signature(public.as_slice())
            .unwrap()
            .finish(request.signature.to_vec())
            .unwrap();
        values.push(vector("app_action_request", canonical(&request)));
        values.push(vector("app_action_artifact", canonical(&artifact)));
        values.push(vector(
            "app_action_signing_bytes",
            prepared.to_be_signed.clone(),
        ));
        values.push(vector(
            "app_action_approval",
            canonical(&approval_message(
                principal(1),
                &request.account_id,
                ATTEST_APPROVAL_DOMAIN,
                &attest_approval_command(&statement, &request.action.origin, &request.signature),
                &request.approval,
            )),
        ));
    }
    println!("{}", serde_json::to_string_pretty(&values).unwrap());
}
