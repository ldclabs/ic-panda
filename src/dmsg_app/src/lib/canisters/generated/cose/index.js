// Generated from the public dmsg_cose.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const Algorithm = IDL.Variant({
    'VetKdBls12381' : IDL.Null,
    'Ed25519' : IDL.Null,
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
    'issuer_namespace' : IDL.Text,
    'daily_executions' : IDL.Nat32,
    'environment' : Environment,
  });
  const KeyPurpose = IDL.Variant({
    'ContentRoot' : IDL.Null,
    'FileAttestation' : IDL.Null,
    'Statement' : IDL.Null,
  });
  const KeyRequest = IDL.Record({
    'algorithm' : Algorithm,
    'generation' : IDL.Nat64,
    'purpose' : KeyPurpose,
  });
  const ExecutionKind = IDL.Variant({
    'Sign' : IDL.Record({
      'key' : KeyRequest,
      'public_key_fingerprint' : IDL.Vec(IDL.Nat8),
      'origin' : IDL.Text,
      'to_be_signed' : IDL.Vec(IDL.Nat8),
    }),
    'Derive' : IDL.Record({
      'generation' : IDL.Nat64,
      'transport_key' : IDL.Vec(IDL.Nat8),
      'root_op_id' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    }),
  });
  const ExecutionGrant = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'request_id' : IDL.Vec(IDL.Nat8),
    'device_sequence' : IDL.Nat64,
    'execution_sequence' : IDL.Nat64,
    'kind' : ExecutionKind,
    'approved_at' : IDL.Nat64,
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'home_cose' : IDL.Principal,
    'max_cycles' : IDL.Nat,
    'home_user' : IDL.Principal,
    'expires_at' : IDL.Nat64,
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
  const Result = IDL.Variant({ 'Ok' : ExecutionResult, 'Err' : Error });
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
  const Result_1 = IDL.Variant({ 'Ok' : KeyState, 'Err' : Error });
  const SigningAlgorithm = IDL.Variant({
    'Ed25519' : IDL.Null,
    'EcdsaSecp256k1' : IDL.Null,
  });
  const SigningPurpose = IDL.Variant({
    'FileAttestation' : IDL.Null,
    'Statement' : IDL.Null,
  });
  const SigningKey = IDL.Record({
    'algorithm' : SigningAlgorithm,
    'purpose' : SigningPurpose,
  });
  const KeySelector = IDL.Variant({
    'ContentRoot' : IDL.Record({ 'generation' : IDL.Nat64 }),
    'Signing' : SigningKey,
  });
  const Result_2 = IDL.Variant({ 'Ok' : KeyDescriptor, 'Err' : Error });
  return IDL.Service({
    'execute' : IDL.Func([ExecutionGrant], [Result], []),
    'get_execution' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result],
        ['query'],
      ),
    'initialize_keys' : IDL.Func([], [Result_1], []),
    'key_state' : IDL.Func([], [KeyState], ['query']),
    'public_key' : IDL.Func(
        [IDL.Vec(IDL.Nat8), KeySelector],
        [Result_2],
        ['query'],
      ),
  });
};
export const init = ({ IDL }) => {
  const Algorithm = IDL.Variant({
    'VetKdBls12381' : IDL.Null,
    'Ed25519' : IDL.Null,
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
    'issuer_namespace' : IDL.Text,
    'daily_executions' : IDL.Nat32,
    'environment' : Environment,
  });
  return [CoseInit];
};
