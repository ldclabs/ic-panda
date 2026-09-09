// Generated from the public dmsg_user.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const UserInit = IDL.Record({
    'handle_canister' : IDL.Principal,
    'home_cose' : IDL.Principal,
    'issuer_namespace' : IDL.Text,
    'daily_new_accounts' : IDL.Nat32,
    'payment_canister' : IDL.Principal,
    'environment' : Environment,
    'max_accounts' : IDL.Nat64,
  });
  const Error = IDL.Variant({
    'MigrationKeyUnavailable' : IDL.Null,
    'LegacyWriteDisabled' : IDL.Null,
    'InvalidInput' : IDL.Text,
    'RekeyRequired' : IDL.Null,
    'VersionConflict' : IDL.Null,
    'ExecutionUnknown' : IDL.Null,
    'IntegrityFailed' : IDL.Null,
    'IdTimestampOutOfRange' : IDL.Null,
    'NotFound' : IDL.Null,
    'FeeBlocked' : IDL.Null,
    'DeviceNotApproved' : IDL.Null,
    'Locked' : IDL.Null,
    'RecoveryIncomplete' : IDL.Null,
    'IdCapacityExceeded' : IDL.Null,
    'PolicyStale' : IDL.Null,
    'IdGeneratorStateConflict' : IDL.Null,
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
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  const HandleAction = IDL.Variant({
    'AcceptTransfer' : IDL.Null,
    'Register' : IDL.Null,
    'Transfer' : IDL.Null,
    'ClaimLegacy' : IDL.Null,
  });
  const HandleIntent = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'handle_canister' : IDL.Principal,
    'target_account' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'action' : HandleAction,
    'op_id' : IDL.Vec(IDL.Nat8),
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
  const CreateAccount = IDL.Record({
    'op_id' : IDL.Vec(IDL.Nat8),
    'device' : DeviceInput,
    'proof' : IDL.Vec(IDL.Nat8),
    'expires_at' : IDL.Nat64,
  });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Nat8), 'Err' : Error });
  const RootTarget = IDL.Variant({
    'Candidate' : IDL.Record({
      'op_id' : IDL.Vec(IDL.Nat8),
      'generation' : IDL.Nat64,
    }),
    'Current' : IDL.Record({ 'generation' : IDL.Nat64 }),
  });
  const Approval = IDL.Record({
    'request_id' : IDL.Vec(IDL.Nat8),
    'signature' : IDL.Vec(IDL.Nat8),
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'expires_at' : IDL.Nat64,
    'sequence' : IDL.Nat64,
  });
  const DeriveRootRequest = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'target' : RootTarget,
    'max_cycles' : IDL.Nat,
    'approval' : Approval,
    'transport_public_key' : IDL.Vec(IDL.Nat8),
  });
  const Algorithm = IDL.Variant({
    'VetKdBls12381' : IDL.Null,
    'Ed25519' : IDL.Null,
    'EcdsaSecp256k1' : IDL.Null,
  });
  const KeyPurpose = IDL.Variant({
    'ContentRoot' : IDL.Null,
    'FileAttestation' : IDL.Null,
    'Statement' : IDL.Null,
  });
  const KeyDescriptor = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'algorithm' : Algorithm,
    'key_generation' : IDL.Nat64,
    'public_key_fingerprint' : IDL.Vec(IDL.Nat8),
    'derivation_version' : IDL.Nat16,
    'public_key' : IDL.Vec(IDL.Nat8),
    'key_id' : IDL.Vec(IDL.Nat8),
    'home_cose' : IDL.Principal,
    'environment' : Environment,
    'master_key_name' : IDL.Text,
    'purpose' : KeyPurpose,
  });
  const SignedArtifact = IDL.Record({
    'cose_sign1' : IDL.Vec(IDL.Nat8),
    'cose_key' : IDL.Vec(IDL.Nat8),
  });
  const ExecutionOutput = IDL.Variant({
    'EncryptedRootKey' : IDL.Record({
      'key' : KeyDescriptor,
      'encrypted_key' : IDL.Vec(IDL.Nat8),
    }),
    'Signature' : IDL.Record({
      'key' : KeyDescriptor,
      'artifact' : SignedArtifact,
    }),
  });
  const ExecutionOutcome = IDL.Variant({
    'Failed' : Error,
    'Executing' : IDL.Null,
    'Authorized' : IDL.Null,
    'Unknown' : Error,
    'ResultExpired' : IDL.Null,
    'Completed' : ExecutionOutput,
  });
  const ExecutionResult = IDL.Record({
    'request_id' : IDL.Vec(IDL.Nat8),
    'charged_cycles' : IDL.Nat,
    'outcome' : ExecutionOutcome,
  });
  const Result_2 = IDL.Variant({ 'Ok' : ExecutionResult, 'Err' : Error });
  const AccountStatus = IDL.Variant({
    'Active' : IDL.Null,
    'RecoveryDisputed' : IDL.Null,
  });
  const RootReservation = IDL.Record({
    'op_id' : IDL.Vec(IDL.Nat8),
    'generation' : IDL.Nat64,
    'security_epoch' : IDL.Nat64,
    'expected_generation' : IDL.Nat64,
    'expires_at' : IDL.Nat64,
  });
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
  const SensitivePolicy = IDL.Record({
    'daily_cycles' : IDL.Nat,
    'daily_executions' : IDL.Nat32,
    'frozen' : IDL.Bool,
    'allowed_purposes' : IDL.Vec(KeyPurpose),
  });
  const RecoveryPolicy = IDL.Record({
    'delay_ms' : IDL.Nat64,
    'generation' : IDL.Nat64,
    'hpke_pub' : IDL.Vec(IDL.Nat8),
    'signing_pub' : IDL.Vec(IDL.Nat8),
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
  const Device = IDL.Record({
    'added_at' : IDL.Nat64,
    'added_by' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'revoked_at' : IDL.Opt(IDL.Nat64),
    'next_sequence' : IDL.Nat64,
    'input' : DeviceInput,
  });
  const VaultWriteState = IDL.Variant({
    'RekeyRequired' : IDL.Null,
    'Ready' : IDL.Null,
    'Uninitialized' : IDL.Null,
  });
  const AccountInfo = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'status' : AccountStatus,
    'account_version' : IDL.Nat64,
    'auth_bindings' : IDL.Vec(IDL.Principal),
    'recovery_nonce' : IDL.Nat64,
    'root_slot' : IDL.Opt(RootReservation),
    'security_epoch' : IDL.Nat64,
    'pending_recovery' : IDL.Opt(PendingRecovery),
    'issuer' : IDL.Text,
    'home_cose' : IDL.Principal,
    'home_user' : IDL.Principal,
    'sensitive_policy' : SensitivePolicy,
    'recovery' : IDL.Opt(RecoveryPolicy),
    'current_root' : IDL.Opt(ContentRootRef),
    'recovery_checked' : IDL.Bool,
    'devices' : IDL.Vec(IDL.Tuple(IDL.Vec(IDL.Nat8), Device)),
    'vault_write_state' : VaultWriteState,
  });
  const Result_3 = IDL.Variant({ 'Ok' : AccountInfo, 'Err' : Error });
  const SecuritySnapshot = IDL.Record({
    'content_root_generation' : IDL.Nat64,
    'account_id' : IDL.Vec(IDL.Nat8),
    'account_version' : IDL.Nat64,
    'recovery_nonce' : IDL.Nat64,
    'content_root_digest' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'devices_root' : IDL.Vec(IDL.Nat8),
    'recovery_hpke_pub' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'schema' : IDL.Nat16,
    'security_epoch' : IDL.Nat64,
    'recovery_root_version' : IDL.Nat64,
    'issuer' : IDL.Text,
    'home_cose' : IDL.Principal,
    'home_user' : IDL.Principal,
    'recovery_delay_ms' : IDL.Opt(IDL.Nat64),
    'recovery_signing_pub' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'vault_write_state' : VaultWriteState,
    'account_status' : AccountStatus,
    'pending_recovery_digest' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Result_4 = IDL.Variant({
    'Ok' : IDL.Tuple(
      SecuritySnapshot,
      IDL.Vec(IDL.Tuple(IDL.Vec(IDL.Nat8), Device)),
    ),
    'Err' : Error,
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
  const Result_5 = IDL.Variant({ 'Ok' : CertifiedBatch, 'Err' : Error });
  const OperationReceipt = IDL.Record({
    'id' : IDL.Vec(IDL.Nat8),
    'account_version' : IDL.Nat64,
    'digest' : IDL.Vec(IDL.Nat8),
  });
  const Result_6 = IDL.Variant({ 'Ok' : OperationReceipt, 'Err' : Error });
  const Result_7 = IDL.Variant({
    'Ok' : IDL.Opt(PendingRecovery),
    'Err' : Error,
  });
  const Result_8 = IDL.Variant({
    'Ok' : IDL.Opt(ContentRootRef),
    'Err' : Error,
  });
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
    'account_id' : IDL.Vec(IDL.Nat8),
    'command' : AccountCommand,
    'approval' : Approval,
    'expected_version' : IDL.Nat64,
  });
  const SigningAlgorithm = IDL.Variant({
    'Ed25519' : IDL.Null,
    'EcdsaSecp256k1' : IDL.Null,
  });
  const SigningKeyRef = IDL.Record({
    'kid' : IDL.Vec(IDL.Nat8),
    'algorithm' : SigningAlgorithm,
    'public_key_fingerprint' : IDL.Vec(IDL.Nat8),
  });
  const StatementContent = IDL.Variant({
    'Text' : IDL.Text,
    'Digest' : IDL.Record({
      'sha256' : IDL.Vec(IDL.Nat8),
      'content_type' : IDL.Opt(IDL.Text),
      'location' : IDL.Opt(IDL.Text),
    }),
  });
  const Statement = IDL.Record({
    'issued_at' : IDL.Opt(IDL.Int64),
    'content' : StatementContent,
    'subject' : IDL.Opt(IDL.Text),
    'issuer' : IDL.Text,
  });
  const SignRequest = IDL.Record({
    'key' : SigningKeyRef,
    'account_id' : IDL.Vec(IDL.Nat8),
    'statement' : Statement,
    'origin' : IDL.Text,
    'max_cycles' : IDL.Nat,
    'approval' : Approval,
  });
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const PaymentOffer = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'quote_scope' : IDL.Vec(IDL.Nat8),
    'issued_at' : IDL.Nat64,
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
  const Result_9 = IDL.Variant({ 'Ok' : IDL.Nat64, 'Err' : Error });
  return IDL.Service({
    'begin_auth_binding' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result],
        [],
      ),
    'complete_recovery' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'consume_handle_authorization' : IDL.Func([HandleIntent], [Result], []),
    'create_account' : IDL.Func([CreateAccount], [Result_1], []),
    'derive_root' : IDL.Func([DeriveRootRequest], [Result_2], []),
    'get_account' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_3], ['query']),
    'get_device_bundle' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_4], ['query']),
    'get_execution' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_2],
        ['query'],
      ),
    'get_execution_receipt' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_5],
        ['query'],
      ),
    'get_operation' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_6],
        ['query'],
      ),
    'get_recovery_request' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_7],
        ['query'],
      ),
    'get_root_ref' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_8], ['query']),
    'mutate_account' : IDL.Func([AccountMutation], [Result_6], []),
    'my_account' : IDL.Func([], [IDL.Opt(IDL.Vec(IDL.Nat8))], ['query']),
    'prune_auth_bindings' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [],
      ),
    'reconcile_execution' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_2],
        [],
      ),
    'reconfirm_recovery' : IDL.Func(
        [IDL.Vec(IDL.Nat8), RecoveryConfirmation, IDL.Vec(IDL.Nat8)],
        [Result],
        [],
      ),
    'request_recovery' : IDL.Func(
        [
          IDL.Vec(IDL.Nat8),
          RecoveryRequest,
          IDL.Vec(IDL.Nat8),
          IDL.Vec(IDL.Nat8),
        ],
        [Result],
        [],
      ),
    'security_snapshot_batch' : IDL.Func(
        [IDL.Vec(IDL.Vec(IDL.Nat8))],
        [Result_5],
        ['query'],
      ),
    'sign' : IDL.Func([SignRequest], [Result_2], []),
    'verify_payment_offer' : IDL.Func([SignedOffer], [Result_9], []),
  });
};
export const init = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const UserInit = IDL.Record({
    'handle_canister' : IDL.Principal,
    'home_cose' : IDL.Principal,
    'issuer_namespace' : IDL.Text,
    'daily_new_accounts' : IDL.Nat32,
    'payment_canister' : IDL.Principal,
    'environment' : Environment,
    'max_accounts' : IDL.Nat64,
  });
  return [UserInit];
};
