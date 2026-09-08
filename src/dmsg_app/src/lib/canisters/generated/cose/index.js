// Generated from the public dmsg_cose.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const Algorithm = IDL.Variant({
    'VetKdBls12381' : IDL.Null,
    'Ed25519' : IDL.Null,
    'Bip340' : IDL.Null,
    'EcdsaSecp256k1' : IDL.Null,
  });
  const MasterKey = IDL.Record({
    'algorithm' : Algorithm,
    'expected_fingerprint' : IDL.Vec(IDL.Nat8),
    'key_name' : IDL.Text,
  });
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const CoseInit = IDL.Record({
    'masters' : IDL.Vec(MasterKey),
    'executing_canister' : IDL.Principal,
    'derivation_version' : IDL.Nat16,
    'daily_cycles' : IDL.Nat,
    'initial_home_user' : IDL.Principal,
    'daily_executions' : IDL.Nat32,
    'environment' : Environment,
  });
  const KeyPurpose = IDL.Variant({
    'ContentRoot' : IDL.Null,
    'FileAttestation' : IDL.Null,
    'ProviderController' : IDL.Null,
    'Identity' : IDL.Null,
    'Statement' : IDL.Null,
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
  const Result = IDL.Variant({ 'Ok' : KeyDescriptor, 'Err' : Error });
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
  const Result_1 = IDL.Variant({ 'Ok' : ExecutionResult, 'Err' : Error });
  const Initialization = IDL.Variant({
    'Ready' : IDL.Null,
    'Uninitialized' : IDL.Null,
    'Initializing' : IDL.Null,
  });
  const KeyState = IDL.Record({
    'initialization' : Initialization,
    'fingerprints' : IDL.Vec(IDL.Vec(IDL.Nat8)),
    'error' : IDL.Opt(IDL.Text),
    'config' : CoseInit,
  });
  const Result_2 = IDL.Variant({ 'Ok' : KeyState, 'Err' : Error });
  const Result_3 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  return IDL.Service({
    'describe_key' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], ['query']),
    'execute' : IDL.Func([ExecutionGrant], [Result_1], []),
    'get_execution' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_1],
        ['query'],
      ),
    'initialize_keys' : IDL.Func([], [Result_2], []),
    'key_state' : IDL.Func([], [KeyState], ['query']),
    'register_subject' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_3], []),
  });
};
export const init = ({ IDL }) => {
  const Algorithm = IDL.Variant({
    'VetKdBls12381' : IDL.Null,
    'Ed25519' : IDL.Null,
    'Bip340' : IDL.Null,
    'EcdsaSecp256k1' : IDL.Null,
  });
  const MasterKey = IDL.Record({
    'algorithm' : Algorithm,
    'expected_fingerprint' : IDL.Vec(IDL.Nat8),
    'key_name' : IDL.Text,
  });
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const CoseInit = IDL.Record({
    'masters' : IDL.Vec(MasterKey),
    'executing_canister' : IDL.Principal,
    'derivation_version' : IDL.Nat16,
    'daily_cycles' : IDL.Nat,
    'initial_home_user' : IDL.Principal,
    'daily_executions' : IDL.Nat32,
    'environment' : Environment,
  });
  return [CoseInit];
};
