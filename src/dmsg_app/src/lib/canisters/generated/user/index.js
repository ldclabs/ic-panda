// Generated from the public dmsg_user.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const UserInit = IDL.Record({
    'handle_canister' : IDL.Principal,
    'daily_new_subjects' : IDL.Nat32,
    'max_subjects' : IDL.Nat64,
    'home_cose' : IDL.Principal,
    'payment_canister' : IDL.Principal,
  });
  const Algorithm = IDL.Variant({
    'VetKdBls12381' : IDL.Null,
    'Ed25519' : IDL.Null,
    'Bip340' : IDL.Null,
    'EcdsaSecp256k1' : IDL.Null,
  });
  const KeyPurpose = IDL.Variant({
    'ContentRoot' : IDL.Null,
    'FileAttestation' : IDL.Null,
    'ProviderController' : IDL.Null,
    'Identity' : IDL.Null,
    'Statement' : IDL.Null,
  });
  const KeyRequest = IDL.Record({
    'algorithm' : Algorithm,
    'provider' : IDL.Opt(IDL.Text),
    'generation' : IDL.Nat64,
    'purpose' : KeyPurpose,
  });
  const ExecutionKind = IDL.Variant({
    'Sign' : IDL.Record({
      'key' : KeyRequest,
      'canonical_payload' : IDL.Vec(IDL.Nat8),
    }),
    'Derive' : IDL.Record({
      'generation' : IDL.Nat64,
      'transport_key' : IDL.Vec(IDL.Nat8),
      'root_op_id' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    }),
  });
  const Approval = IDL.Record({
    'request_id' : IDL.Vec(IDL.Nat8),
    'signature' : IDL.Vec(IDL.Nat8),
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'expires_at' : IDL.Nat64,
    'sequence' : IDL.Nat64,
  });
  const ExecuteRequest = IDL.Record({
    'subject' : IDL.Vec(IDL.Nat8),
    'kind' : ExecutionKind,
    'max_cycles' : IDL.Nat,
    'approval' : Approval,
  });
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const KeyDescriptor = IDL.Record({
    'algorithm' : Algorithm,
    'key_generation' : IDL.Nat64,
    'public_key_fingerprint' : IDL.Vec(IDL.Nat8),
    'provider' : IDL.Opt(IDL.Text),
    'subject' : IDL.Vec(IDL.Nat8),
    'derivation_version' : IDL.Nat16,
    'public_key' : IDL.Vec(IDL.Nat8),
    'key_id' : IDL.Vec(IDL.Nat8),
    'home_cose' : IDL.Principal,
    'environment' : Environment,
    'master_key_name' : IDL.Text,
    'purpose' : KeyPurpose,
  });
  const ExecutionStatus = IDL.Variant({
    'Failed' : IDL.Null,
    'Executing' : IDL.Null,
    'Authorized' : IDL.Null,
    'Unknown' : IDL.Null,
    'ResultExpired' : IDL.Null,
    'Completed' : IDL.Null,
  });
  const ExecutionResult = IDL.Record({
    'key' : IDL.Opt(KeyDescriptor),
    'status' : ExecutionStatus,
    'result' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'charged_cycles' : IDL.Nat,
  });
  const Error = IDL.Variant({
    'MigrationKeyUnavailable' : IDL.Null,
    'LegacyWriteDisabled' : IDL.Null,
    'InvalidInput' : IDL.Text,
    'RekeyRequired' : IDL.Null,
    'VersionConflict' : IDL.Null,
    'ExecutionUnknown' : IDL.Null,
    'IntegrityFailed' : IDL.Null,
    'NotFound' : IDL.Null,
    'FeeBlocked' : IDL.Null,
    'DeviceNotApproved' : IDL.Null,
    'Locked' : IDL.Null,
    'RecoveryIncomplete' : IDL.Null,
    'PolicyStale' : IDL.Null,
    'IdempotencyConflict' : IDL.Null,
    'UnsupportedProtocol' : IDL.Null,
    'Unavailable' : IDL.Text,
    'Forbidden' : IDL.Null,
    'ResultExpired' : IDL.Null,
    'Expired' : IDL.Null,
    'QuotaExceeded' : IDL.Null,
    'AuthRequired' : IDL.Null,
    'Pending' : IDL.Null,
  });
  const Result = IDL.Variant({ 'Ok' : ExecutionResult, 'Err' : Error });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  const HandleAction = IDL.Variant({
    'AcceptTransfer' : IDL.Null,
    'Register' : IDL.Null,
    'Transfer' : IDL.Null,
    'ClaimLegacy' : IDL.Null,
  });
  const HandleIntent = IDL.Record({
    'handle_canister' : IDL.Principal,
    'action' : HandleAction,
    'subject' : IDL.Vec(IDL.Nat8),
    'op_id' : IDL.Vec(IDL.Nat8),
    'target_subject' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'handle' : IDL.Text,
    'expected_version' : IDL.Nat64,
    'terms_digest' : IDL.Vec(IDL.Nat8),
  });
  const Capability = IDL.Variant({
    'FormalApprove' : IDL.Null,
    'ContentSign' : IDL.Null,
    'VaultUnlock' : IDL.Null,
    'RootManage' : IDL.Null,
    'PaymentOffer' : IDL.Null,
  });
  const ControllerRole = IDL.Variant({
    'Administrator' : IDL.Null,
    'Member' : IDL.Null,
  });
  const DeviceInput = IDL.Record({
    'capabilities' : IDL.Vec(Capability),
    'role' : ControllerRole,
    'device_id' : IDL.Vec(IDL.Nat8),
    'hpke_pub' : IDL.Vec(IDL.Nat8),
    'signing_pub' : IDL.Vec(IDL.Nat8),
  });
  const CreateSubject = IDL.Record({
    'op_id' : IDL.Vec(IDL.Nat8),
    'device' : DeviceInput,
    'proof' : IDL.Vec(IDL.Nat8),
    'expires_at' : IDL.Nat64,
  });
  const Result_2 = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Nat8), 'Err' : Error });
  const VaultWriteState = IDL.Variant({
    'RekeyRequired' : IDL.Null,
    'Ready' : IDL.Null,
    'Uninitialized' : IDL.Null,
  });
  const AccountStatus = IDL.Variant({
    'Active' : IDL.Null,
    'RecoveryDisputed' : IDL.Null,
  });
  const SecuritySnapshot = IDL.Record({
    'content_root_generation' : IDL.Nat64,
    'account_version' : IDL.Nat64,
    'recovery_nonce' : IDL.Nat64,
    'content_root_digest' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'devices_root' : IDL.Vec(IDL.Nat8),
    'recovery_hpke_pub' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'schema' : IDL.Nat16,
    'security_epoch' : IDL.Nat64,
    'recovery_root_version' : IDL.Nat64,
    'home_user' : IDL.Principal,
    'subject_id' : IDL.Vec(IDL.Nat8),
    'recovery_delay_ns' : IDL.Opt(IDL.Nat64),
    'recovery_signing_pub' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'vault_write_state' : VaultWriteState,
    'account_status' : AccountStatus,
    'pending_recovery_digest' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Device = IDL.Record({
    'added_at' : IDL.Nat64,
    'added_by' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'revoked_at' : IDL.Opt(IDL.Nat64),
    'next_sequence' : IDL.Nat64,
    'input' : DeviceInput,
  });
  const Result_3 = IDL.Variant({
    'Ok' : IDL.Tuple(
      SecuritySnapshot,
      IDL.Vec(IDL.Tuple(IDL.Vec(IDL.Nat8), Device)),
    ),
    'Err' : Error,
  });
  const ExecutionGrant = IDL.Record({
    'request_id' : IDL.Vec(IDL.Nat8),
    'device_sequence' : IDL.Nat64,
    'execution_sequence' : IDL.Nat64,
    'subject' : IDL.Vec(IDL.Nat8),
    'kind' : ExecutionKind,
    'approved_at' : IDL.Nat64,
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'home_cose' : IDL.Principal,
    'max_cycles' : IDL.Nat,
    'home_user' : IDL.Principal,
    'expires_at' : IDL.Nat64,
  });
  const AuthorizedExecution = IDL.Record({
    'result' : ExecutionResult,
    'command_digest' : IDL.Vec(IDL.Nat8),
    'grant' : ExecutionGrant,
  });
  const Result_4 = IDL.Variant({ 'Ok' : AuthorizedExecution, 'Err' : Error });
  const OperationReceipt = IDL.Record({
    'id' : IDL.Vec(IDL.Nat8),
    'account_version' : IDL.Nat64,
    'digest' : IDL.Vec(IDL.Nat8),
  });
  const Result_5 = IDL.Variant({ 'Ok' : OperationReceipt, 'Err' : Error });
  const RecoveryRequest = IDL.Record({
    'op_id' : IDL.Vec(IDL.Nat8),
    'generation' : IDL.Nat64,
    'device' : DeviceInput,
    'new_auth' : IDL.Principal,
    'expires_at' : IDL.Nat64,
  });
  const RecoveryConfirmation = IDL.Record({
    'request_id' : IDL.Vec(IDL.Nat8),
    'dispute' : IDL.Vec(IDL.Nat8),
    'expires_at' : IDL.Nat64,
  });
  const PendingRecovery = IDL.Record({
    'reconfirmed' : IDL.Bool,
    'request' : RecoveryRequest,
    'execute_after' : IDL.Nat64,
    'confirmation' : IDL.Opt(RecoveryConfirmation),
    'dispute' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Result_6 = IDL.Variant({
    'Ok' : IDL.Opt(PendingRecovery),
    'Err' : Error,
  });
  const ContentRootRef = IDL.Record({
    'key_generation' : IDL.Nat64,
    'derivation_version' : IDL.Nat16,
    'recovery_generation' : IDL.Nat64,
    'generation' : IDL.Nat64,
    'suite' : IDL.Text,
    'home_cose' : IDL.Principal,
    'bundle_digest' : IDL.Vec(IDL.Nat8),
  });
  const Result_7 = IDL.Variant({
    'Ok' : IDL.Opt(ContentRootRef),
    'Err' : Error,
  });
  const RootReservation = IDL.Record({
    'op_id' : IDL.Vec(IDL.Nat8),
    'generation' : IDL.Nat64,
    'security_epoch' : IDL.Nat64,
    'expected_generation' : IDL.Nat64,
    'expires_at' : IDL.Nat64,
  });
  const HandleAuthorization = IDL.Record({
    'intent' : HandleIntent,
    'consumed' : IDL.Bool,
    'expires_at' : IDL.Nat64,
  });
  const SensitivePolicy = IDL.Record({
    'daily_cycles' : IDL.Nat,
    'daily_executions' : IDL.Nat32,
    'frozen' : IDL.Bool,
    'allowed_purposes' : IDL.Vec(KeyPurpose),
  });
  const RecoveryPolicy = IDL.Record({
    'delay_ns' : IDL.Nat64,
    'generation' : IDL.Nat64,
    'hpke_pub' : IDL.Vec(IDL.Nat8),
    'signing_pub' : IDL.Vec(IDL.Nat8),
  });
  const Budget = IDL.Record({
    'day' : IDL.Nat64,
    'executions' : IDL.Nat32,
    'cycles' : IDL.Nat,
  });
  const Subject = IDL.Record({
    'status' : AccountStatus,
    'account_version' : IDL.Nat64,
    'auth_bindings' : IDL.Vec(IDL.Principal),
    'recovery_nonce' : IDL.Nat64,
    'root_slot' : IDL.Opt(RootReservation),
    'executions' : IDL.Vec(IDL.Tuple(IDL.Vec(IDL.Nat8), AuthorizedExecution)),
    'security_epoch' : IDL.Nat64,
    'pending_recovery' : IDL.Opt(PendingRecovery),
    'home_cose' : IDL.Principal,
    'home_user' : IDL.Principal,
    'subject_id' : IDL.Vec(IDL.Nat8),
    'operations' : IDL.Vec(OperationReceipt),
    'next_execution_sequence' : IDL.Nat64,
    'handle_authorizations' : IDL.Vec(
      IDL.Tuple(IDL.Vec(IDL.Nat8), HandleAuthorization)
    ),
    'sensitive_policy' : SensitivePolicy,
    'recovery' : IDL.Opt(RecoveryPolicy),
    'current_root' : IDL.Opt(ContentRootRef),
    'next_root_generation' : IDL.Nat64,
    'recovery_checked' : IDL.Bool,
    'budget' : Budget,
    'devices' : IDL.Vec(IDL.Tuple(IDL.Vec(IDL.Nat8), Device)),
    'vault_write_state' : VaultWriteState,
  });
  const Result_8 = IDL.Variant({ 'Ok' : Subject, 'Err' : Error });
  const AccountCommand = IDL.Variant({
    'DisputeRecovery' : IDL.Record({
      'op_id' : IDL.Vec(IDL.Nat8),
      'dispute' : IDL.Vec(IDL.Nat8),
    }),
    'ConfirmRecovery' : IDL.Record({ 'proof' : IDL.Vec(IDL.Nat8) }),
    'BindAuth' : IDL.Record({
      'principal' : IDL.Principal,
      'nonce' : IDL.Vec(IDL.Nat8),
    }),
    'ReserveRoot' : IDL.Record({
      'op_id' : IDL.Vec(IDL.Nat8),
      'expected_generation' : IDL.Nat64,
    }),
    'RemoveAuth' : IDL.Record({ 'principal' : IDL.Principal }),
    'AuthorizeHandle' : IDL.Record({ 'intent' : HandleIntent }),
    'SetRecovery' : IDL.Record({
      'proof' : IDL.Vec(IDL.Nat8),
      'policy' : RecoveryPolicy,
    }),
    'AddDevice' : IDL.Record({
      'device' : DeviceInput,
      'proof' : IDL.Vec(IDL.Nat8),
    }),
    'CommitRoot' : IDL.Record({
      'op_id' : IDL.Vec(IDL.Nat8),
      'root' : ContentRootRef,
      'expected_generation' : IDL.Nat64,
    }),
    'RevokeDevice' : IDL.Record({ 'device_id' : IDL.Vec(IDL.Nat8) }),
    'SetPolicy' : IDL.Record({ 'policy' : SensitivePolicy }),
  });
  const AccountMutation = IDL.Record({
    'subject' : IDL.Vec(IDL.Nat8),
    'command' : AccountCommand,
    'approval' : Approval,
    'expected_version' : IDL.Nat64,
  });
  const CertifiedEntry = IDL.Record({
    'key' : IDL.Vec(IDL.Nat8),
    'value' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'witness' : IDL.Vec(IDL.Nat8),
  });
  const CertifiedBatch = IDL.Record({
    'certificate' : IDL.Vec(IDL.Nat8),
    'schema' : IDL.Nat16,
    'entries' : IDL.Vec(CertifiedEntry),
    'canister' : IDL.Principal,
  });
  const Result_9 = IDL.Variant({ 'Ok' : CertifiedBatch, 'Err' : Error });
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const PaymentOffer = IDL.Record({
    'quote_scope' : IDL.Vec(IDL.Nat8),
    'issued_at' : IDL.Nat64,
    'subject' : IDL.Vec(IDL.Nat8),
    'recipient' : Account,
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'version' : IDL.Nat64,
    'ledger' : IDL.Principal,
    'offer_id' : IDL.Vec(IDL.Nat8),
    'home_payment' : IDL.Principal,
    'expires_at' : IDL.Nat64,
    'recipient_net' : IDL.Nat,
  });
  const SignedOffer = IDL.Record({
    'signature' : IDL.Vec(IDL.Nat8),
    'offer' : PaymentOffer,
  });
  const Result_10 = IDL.Variant({ 'Ok' : IDL.Nat64, 'Err' : Error });
  return IDL.Service({
    'authorize_and_execute' : IDL.Func([ExecuteRequest], [Result], []),
    'begin_auth_binding' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_1],
        [],
      ),
    'complete_recovery' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_1], []),
    'consume_handle_authorization' : IDL.Func([HandleIntent], [Result_1], []),
    'create_subject' : IDL.Func([CreateSubject], [Result_2], []),
    'get_device_bundle' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_3], ['query']),
    'get_execution' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_4],
        ['query'],
      ),
    'get_operation' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_5],
        ['query'],
      ),
    'get_recovery_request' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_6],
        ['query'],
      ),
    'get_root_ref' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_7], ['query']),
    'get_subject' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_8], ['query']),
    'mutate_account' : IDL.Func([AccountMutation], [Result_5], []),
    'my_subject' : IDL.Func([], [IDL.Opt(IDL.Vec(IDL.Nat8))], ['query']),
    'prune_auth_bindings' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [],
      ),
    'reconcile_execution' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result],
        [],
      ),
    'reconfirm_recovery' : IDL.Func(
        [IDL.Vec(IDL.Nat8), RecoveryConfirmation, IDL.Vec(IDL.Nat8)],
        [Result_1],
        [],
      ),
    'request_recovery' : IDL.Func(
        [
          IDL.Vec(IDL.Nat8),
          RecoveryRequest,
          IDL.Vec(IDL.Nat8),
          IDL.Vec(IDL.Nat8),
        ],
        [Result_1],
        [],
      ),
    'security_snapshot_batch' : IDL.Func(
        [IDL.Vec(IDL.Vec(IDL.Nat8))],
        [Result_9],
        ['query'],
      ),
    'verify_payment_offer' : IDL.Func([SignedOffer], [Result_10], []),
  });
};
export const init = ({ IDL }) => {
  const UserInit = IDL.Record({
    'handle_canister' : IDL.Principal,
    'daily_new_subjects' : IDL.Nat32,
    'max_subjects' : IDL.Nat64,
    'home_cose' : IDL.Principal,
    'payment_canister' : IDL.Principal,
  });
  return [UserInit];
};
