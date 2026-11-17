//! Synthetic Google Play bindings, not an actual purchase or verified token.
use candid::Principal;
use dmsg_protocol::{billing::beneficiary, google_play::*};
use dmsg_types::{AccountId, Hash, billing::PlanId, google_play::*};
use serde_json::json;

fn main() {
    let home = Principal::from_slice(&[1, 1]);
    let account = AccountId([3; 12]);
    let binding = account_token(home, &account);
    let transaction = PlayTransaction {
        beneficiary: beneficiary(home, &account),
        token_hash: Hash::new([5; 32]),
        order_id: Some("GPA.0000-0000-0000-00000..0".into()),
        product_id: "example.plus".into(),
        option_id: "annual".into(),
        offer_id: None,
        account_binding: binding.clone(),
        package_name: "net.dmsg.android".into(),
        environment: PlayEnvironment::Production,
        state: PlayState::Active,
        purchased_at_ms: 1_800_000_000_000,
        term_starts_at_ms: Some(1_800_000_000_000),
        expires_at_ms: Some(1_831_536_000_000),
        linked_token_hash: None,
        base_contract_id: None,
        replaced: false,
        revision: 1,
        checked_at_ms: 1_800_000_000_000,
    };
    let kinds = [
        PlayProductKind::Plan {
            plan: PlanId::Plus,
            base_plan_id: "annual".into(),
        },
        PlayProductKind::Storage {
            product: Hash::new([6; 32]),
            purchase_option_id: "buy".into(),
        },
    ];
    let mut cases: Vec<_> = kinds.iter().map(|kind| {
        let mut t = transaction.clone();
        if matches!(kind, PlayProductKind::Storage { .. }) {
            t.product_id = "example.storage".into(); t.option_id = "buy".into(); t.order_id = None; t.expires_at_ms = None;
            t.base_contract_id = Some(transaction_id(&transaction, &kinds[0]));
            t.term_starts_at_ms = None;
        }
        json!({"kind":kind,"transaction":t,"candid_hex":hex::encode(candid::encode_one(&t).unwrap()),"transaction_id":hex::encode(transaction_id(&t,kind).as_slice()),"token_id":hex::encode(token_id(&t.environment,&t.package_name,&t.token_hash).as_slice())})
    }).collect();
    let mut transferred = transaction.clone();
    transferred.token_hash = Hash::new([7; 32]);
    transferred.linked_token_hash = Some(transaction.token_hash);
    transferred.purchased_at_ms += 86_400_000;
    transferred.checked_at_ms = transferred.purchased_at_ms;
    cases.push(json!({"kind":kinds[0],"transaction":transferred,"candid_hex":hex::encode(candid::encode_one(&transferred).unwrap()),"transaction_id":hex::encode(transaction_id(&transferred,&kinds[0]).as_slice()),"token_id":hex::encode(token_id(&transferred.environment,&transferred.package_name,&transferred.token_hash).as_slice())}));
    println!("{}", serde_json::to_string_pretty(&json!({"protocol":"dmsg-google-play/1","home":home.to_text(),"account":account.to_string(),"account_binding":binding,"cases":cases})).unwrap());
}
