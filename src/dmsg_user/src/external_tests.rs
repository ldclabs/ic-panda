use super::*;
use dmsg_protocol::{approval_message, decode_canonical};
use ed25519_dalek::{Signer, SigningKey};

#[path = "../../dmsg_types/tests/support/integration.rs"]
mod fixtures;

fn account() -> AccountState {
    let sk = SigningKey::from_bytes(&[7; 32]);
    let input = DeviceInput {
        device_id: Hash::new([7; 32]),
        signing_pub: sk.verifying_key().to_bytes().into(),
        hpke_pub: Hash::new([8; 32]),
        role: ControllerRole::Administrator,
        capabilities: vec![Capability::RootManage, Capability::FormalApprove],
    };
    let caller = fixtures::principal(9);
    let home = fixtures::principal(1);
    let op = Hash::new([1; 32]);
    let expiry = fixtures::NOW + MINUTE;
    let proof = sk.sign(
        digest(
            "dmsg/create-account/v1",
            &(home, caller, &input, op, expiry),
        )
        .as_slice(),
    );
    crate::account::create(
        home,
        fixtures::principal(2),
        AccountId([1; 12]),
        caller,
        &CreateAccount {
            device: input,
            op_id: op,
            expires_at: expiry,
            proof: proof.to_bytes().to_vec().into(),
        },
        fixtures::NOW,
    )
    .unwrap()
}

fn signed(account: &AccountState, request: &AuthenticationRequest) -> Approval {
    let mut approval = Approval {
        device_id: Hash::new([7; 32]),
        security_epoch: account.security_epoch,
        sequence: account.devices[&Hash::new([7; 32])].next_sequence,
        request_id: request.operation_id,
        expires_at: request.expires_at_ms,
        signature: vec![].into(),
    };
    approval.signature = SigningKey::from_bytes(&[7; 32])
        .sign(
            approval_message(
                account.home_user,
                &account.account_id,
                AUTH_DOMAIN,
                request,
                &approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    approval
}

fn result(account: &AccountState, request: AuthenticationRequest) -> ExternalBody {
    ExternalBody::Authentication(AuthenticationResult {
        version: 1,
        expires_at_ms: request.expires_at_ms,
        request,
        account_id: account.account_id.clone(),
        home_user: account.home_user,
        security_epoch: account.security_epoch,
        device_id: Hash::new([7; 32]),
        approved_at_ms: fixtures::NOW,
    })
}

#[test]
fn device_approval_is_purpose_separated_and_rechecked_after_a_callback() {
    let mut account = account();
    let request = fixtures::authentication();
    let approval = signed(&account, &request);
    let caller = fixtures::principal(9);
    let at = fixtures::NOW;
    precheck(&account, caller, &approval, AUTH_DOMAIN, &request, at).unwrap();
    assert!(precheck(
        &account,
        caller,
        &approval,
        APPLICATION_DOMAIN,
        &request,
        at
    )
    .is_err());
    assert!(precheck(
        &account,
        caller,
        &approval,
        AUTH_DOMAIN,
        &request,
        approval.expires_at
    )
    .is_err());
    account.sensitive_policy.frozen = true;
    assert_eq!(
        precheck(&account, caller, &approval, AUTH_DOMAIN, &request, at),
        Err(Error::Locked)
    );
    account.sensitive_policy.frozen = false;
    account.security_epoch += 1;
    assert_eq!(
        precheck(&account, caller, &approval, AUTH_DOMAIN, &request, at),
        Err(Error::PolicyStale)
    );
    account.security_epoch -= 1;
    account
        .devices
        .get_mut(&approval.device_id)
        .unwrap()
        .revoked_at = Some(at);
    assert_eq!(
        precheck(&account, caller, &approval, AUTH_DOMAIN, &request, at),
        Err(Error::DeviceNotApproved)
    );
}

#[test]
fn failed_quota_does_not_consume_sequence_and_replay_does_not_charge_again() {
    let mut account = account();
    let mut state = ExternalState::default();
    let request = fixtures::authentication();
    let approval = signed(&account, &request);
    let at = fixtures::NOW;
    let body = result(&account, request.clone());
    let fingerprint = digest(AUTH_DOMAIN, &(&request, &approval));
    state.hour = at / (60 * MINUTE);
    state.used = MAX_HOURLY_APPROVALS;
    let saved_account = account.clone();
    let saved_state = state.clone();
    assert_eq!(
        commit(
            &mut state,
            &mut account,
            &approval,
            fingerprint,
            body.clone(),
            at
        ),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(saved_account, account);
    assert_eq!(saved_state, state);
    state.used = 0;
    precheck(
        &account,
        fixtures::principal(9),
        &approval,
        AUTH_DOMAIN,
        &request,
        at,
    )
    .unwrap();
    commit(
        &mut state,
        &mut account,
        &approval,
        fingerprint,
        body.clone(),
        at,
    )
    .unwrap();
    assert_eq!(state.used, 1);
    assert_eq!(account.devices[&approval.device_id].next_sequence, 1);
    assert_eq!(
        existing(
            &state,
            &account,
            fixtures::principal(9),
            approval.request_id,
            fingerprint,
            at
        ),
        Ok(Some(body))
    );
    assert_eq!(state.used, 1);
    assert!(existing(
        &state,
        &account,
        fixtures::principal(9),
        approval.request_id,
        Hash::new([99; 32]),
        at
    )
    .is_err());
    assert!(existing(
        &state,
        &account,
        fixtures::principal(99),
        approval.request_id,
        fingerprint,
        at
    )
    .is_err());
    assert!(existing(
        &state,
        &account,
        fixtures::principal(9),
        approval.request_id,
        fingerprint,
        approval.expires_at
    )
    .is_err());
}

#[test]
fn authentication_is_independent_of_formal_execution_and_vault_state() {
    let account = account();
    let request = fixtures::authentication();
    let approval = signed(&account, &request);
    assert!(!account.recovery_checked);
    assert_eq!(account.vault_write_state, VaultWriteState::Uninitialized);
    let budget = account.budget.clone();
    let mut changed = account.clone();
    let mut state = ExternalState::default();
    precheck(
        &changed,
        fixtures::principal(9),
        &approval,
        AUTH_DOMAIN,
        &request,
        fixtures::NOW,
    )
    .unwrap();
    commit(
        &mut state,
        &mut changed,
        &approval,
        Hash::new([5; 32]),
        result(&account, request),
        fixtures::NOW,
    )
    .unwrap();
    assert_eq!(changed.budget, budget);
    assert!(changed.execution_expirations.is_empty());
    changed.status = AccountStatus::RecoveryDisputed;
    assert_eq!(
        current_device(&changed, approval.device_id, approval.security_epoch),
        Err(Error::Locked)
    );
}

#[test]
fn stored_approval_preserves_exact_bytes_through_stable_encoding() {
    let mut account = account();
    let request = fixtures::authentication();
    let approval = signed(&account, &request);
    let mut state = ExternalState::default();
    let body = result(&account, request.clone());
    commit(
        &mut state,
        &mut account,
        &approval,
        Hash::new([5; 32]),
        body,
        fixtures::NOW,
    )
    .unwrap();
    let bytes = canonical(&state);
    let decoded: ExternalState = decode_canonical(&bytes).unwrap();
    assert_eq!(state, decoded);
}
