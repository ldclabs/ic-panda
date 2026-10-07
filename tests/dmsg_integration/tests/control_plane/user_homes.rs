use super::*;
use dmsg_types::billing::ExecutionUsage;

type Rendered = std::result::Result<String, String>;

/// Add `home` through governance after checking the validator and access.
fn add_home(f: &Fixture, canister: Principal, governance: Principal, home: Principal) {
    let validate = || -> Rendered {
        query(
            &f.ic,
            canister,
            person(9),
            "validate_admin_add_user_home",
            (home,),
        )
    };
    assert!(validate()
        .unwrap()
        .starts_with(&format!("Add user home {home}")));
    let rejected: Rendered = query(
        &f.ic,
        canister,
        person(9),
        "validate_admin_add_user_home",
        (Principal::anonymous(),),
    );
    assert_eq!(rejected, Err("AuthRequired".into()));
    assert_denied(&f.ic, canister, person(9), "admin_add_user_home", (home,));
    for _ in 0..2 {
        let added: Result<()> = update(&f.ic, canister, governance, "admin_add_user_home", (home,));
        added.unwrap();
    }
    assert!(validate().unwrap().ends_with("Already listed; no change."));
}

#[test]
fn services_serve_accounts_of_a_user_home_added_by_governance() {
    let mut f = Fixture::new();
    let first = f.user;
    let local = f.root_account(1);
    let initialized: Result<candid::Reserved> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    let second = install_user_home(&f, f.commerce);
    f.user = second;
    let remote = f.root_account(2);
    let read = |home: Principal, account: &AccountId| -> Result<ExecutionResult> {
        query(
            &f.ic,
            f.cose,
            home,
            "get_execution",
            (account, Hash::new([1; 32])),
        )
    };
    let refresh = || -> Result<ExecutionUsage> {
        update(
            &f.ic,
            second,
            person(2),
            "refresh_execution_entitlement",
            (remote,),
        )
    };
    let order = f.order(&remote, 2, 1);
    let open = || -> Result<EscrowInfo> {
        update(
            &f.ic,
            f.payment,
            person(40),
            "open_escrow",
            (order.clone(),),
        )
    };

    // An unlisted home reaches no service.
    assert_eq!(read(second, &remote), Err(Error::Forbidden));
    assert_eq!(refresh(), Err(Error::Forbidden));
    assert_eq!(open(), Err(Error::NotFound));

    add_home(&f, f.cose, f.sns, second);
    add_home(&f, f.commerce, f.sns, second);
    add_home(&f, f.payment, Principal::from_slice(&[90]), second);
    add_home(&f, f.directory, f.sns, second);

    // Each account is served at its own home, and only there.
    assert_eq!(read(second, &remote), Err(Error::NotFound));
    assert_eq!(read(first, &remote), Err(Error::Forbidden));
    assert_eq!(read(second, &local), Err(Error::Forbidden));
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    assert_eq!(
        f.derive(2, &remote, transport.public_key()).status(),
        ExecutionStatus::Completed
    );
    refresh().unwrap();
    assert_eq!(open().unwrap().quote, order.quote);
    let state: KeyState = query(&f.ic, f.cose, person(9), "key_state", ());
    assert_eq!(state.config.user_homes, vec![first, second]);
    let directory: dmsg_types::agent::DirectoryInit =
        query(&f.ic, f.directory, person(9), "directory_config", ());
    assert_eq!(directory.user_homes, vec![first, second]);
}
