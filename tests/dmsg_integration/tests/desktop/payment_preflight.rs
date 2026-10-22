use super::*;
use icrc_ledger_types::icrc1::account::Account;

/// Exercise the daemon boundary used by desktop and CLI, without trusting a
/// form's cached balance. No failed preflight may leave an approval request.
#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn unfunded_wallets_and_handles_do_not_create_approvals() {
    let env = Env::launch(
        "payment-preflight",
        Options {
            wallet: true,
            ..Options::default()
        },
    );
    let (b, d, http) = (&env.b, &env.deployment, &env.http);
    b.login(http, 43).unwrap();
    b.run(&["account", "create"]);
    let payer = Account::from(Principal::from_text(login_principal(43)).unwrap());
    let approvals = || {
        b.rpc(http, "approval/list", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .len()
    };
    let refuse = |method: &str, params: Value, code: &str| {
        let before = approvals();
        assert_eq!(b.rpc(http, method, params).unwrap_err(), code);
        assert_eq!(approvals(), before, "{method} left an approval request");
    };

    let readiness = b
        .rpc(http, "wallet/readiness", json!({"funding":true}))
        .unwrap();
    let ledger = Principal::from_text(readiness["balance"]["ledger"].as_str().unwrap()).unwrap();
    let quote = b.run(&["wallet", "quote"]);
    let fee: u128 = quote["ledger_fee_e8s"].as_str().unwrap().parse().unwrap();
    let create = || json!({"quote":quote["quote_id"],"name":"Preflight team"});
    refuse("wallet/create", create(), "INSUFFICIENT_FUNDS");
    mint(d, ledger, payer, u128::from(WALLET_PRICE));
    refuse("wallet/create", create(), "INSUFFICIENT_FUNDS");

    // Unknown balance and a changed fee also fail before approval creation.
    d.ic.stop_canister(ledger, None).unwrap();
    refuse("wallet/create", create(), "UNAVAILABLE");
    d.ic.start_canister(ledger, None).unwrap();
    set_ledger_fee(d, ledger, fee + 1);
    refuse("wallet/create", create(), "FEE_BLOCKED");
    set_ledger_fee(d, ledger, fee);

    mint(d, ledger, payer, fee);
    let ready = b
        .rpc(http, "wallet/readiness", json!({"funding":true}))
        .unwrap();
    assert_eq!(
        ready["balance"]["balance"],
        (u128::from(WALLET_PRICE) + fee).to_string()
    );
    let request = b.rpc(http, "wallet/create", create()).unwrap();
    assert_eq!(request["type"], "wallet");
    assert_eq!(balance(b, &ledger), u128::from(WALLET_PRICE) + fee);
    b.decide(http, &request, false).unwrap();

    // The previously sufficient balance is stale after an external payment.
    ledger_transfer(
        d,
        ledger,
        43,
        Account::from(d.peer),
        u128::from(WALLET_PRICE),
    );
    refuse("wallet/create", create(), "INSUFFICIENT_FUNDS");

    let name = "preflight_name";
    let price = b.rpc(http, "handle/price", json!({"name":name})).unwrap();
    let total: u128 = price["price"].as_str().unwrap().parse().unwrap();
    let fee: u128 = price["fee"].as_str().unwrap().parse().unwrap();
    let register = || json!({"name":name});
    refuse("handle/register", register(), "INSUFFICIENT_FUNDS");
    d.ic.stop_canister(d.name_ledger, None).unwrap();
    refuse("handle/register", register(), "UNAVAILABLE");
    d.ic.start_canister(d.name_ledger, None).unwrap();
    mint(d, d.name_ledger, payer, total);
    refuse("handle/register", register(), "INSUFFICIENT_FUNDS");

    // An existing allowance needs no additional approval transaction fee.
    let allowance = |amount: u128| {
        d.ic.update_call(
            d.name_ledger,
            d.peer,
            "approve_test",
            candid::encode_args((payer, Account::from(d.handle), amount)).unwrap(),
        )
        .unwrap();
    };
    allowance(total);
    let request = b.rpc(http, "handle/register", register()).unwrap();
    assert_eq!(request["type"], "handle");
    assert_eq!(balance(b, &d.name_ledger), total);
    b.decide(http, &request, false).unwrap();

    allowance(0);
    mint(d, d.name_ledger, payer, fee);
    // The selected payer, including a PEM identity, must fund the request.
    let pem = env.dir.join("unfunded-payer.pem");
    std::fs::write(&pem, ed25519_pem(44)).unwrap();
    b.run(&["payer", "use", pem.to_str().unwrap()]);
    refuse("handle/register", register(), "INSUFFICIENT_FUNDS");
    b.run(&["payer", "use"]);
    let request = b.rpc(http, "handle/register", register()).unwrap();
    assert_eq!(balance(b, &d.name_ledger), total + fee);
    assert_eq!(b.decide(http, &request, true).unwrap()["state"], "done");
    assert_eq!(balance(b, &d.name_ledger), 0);
    env.finish();
}
