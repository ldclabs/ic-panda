// Generated from the public dmsg_cose.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const MasterKey = IDL.Record({
    'expected_fingerprint' : IDL.Vec(IDL.Nat8),
    'key_name' : IDL.Text,
  });
  const CoseInit = IDL.Record({
    'executing_canister' : IDL.Principal,
    'derivation_version' : IDL.Nat16,
    'daily_cycles' : IDL.Nat,
    'issuer_namespace' : IDL.Text,
    'daily_executions' : IDL.Nat32,
    'environment' : Environment,
    'master' : MasterKey,
    'governance' : IDL.Principal,
    'user_homes' : IDL.Vec(IDL.Principal),
  });
  const Error = IDL.Variant({
    'MigrationKeyUnavailable' : IDL.Null,
    'LegacyWriteDisabled' : IDL.Null,
    'InvalidInput' : IDL.Text,
    'IntervalReserved' : IDL.Null,
    'NeuronOccupied' : IDL.Null,
    'RekeyRequired' : IDL.Null,
    'VersionConflict' : IDL.Null,
    'ExecutionUnknown' : IDL.Null,
    'IntegrityFailed' : IDL.Null,
    'IdTimestampOutOfRange' : IDL.Null,
    'NotFound' : IDL.Null,
    'FeeBlocked' : IDL.Null,
    'DeviceNotApproved' : IDL.Null,
    'Locked' : IDL.Null,
    'MembershipClosing' : IDL.Null,
    'RecoveryIncomplete' : IDL.Null,
    'IdCapacityExceeded' : IDL.Null,
    'PolicyStale' : IDL.Null,
    'IdGeneratorStateConflict' : IDL.Null,
    'IdempotencyConflict' : IDL.Null,
    'UnsupportedProtocol' : IDL.Null,
    'Unavailable' : IDL.Text,
    'MembershipStale' : IDL.Null,
    'Forbidden' : IDL.Null,
    'ResultExpired' : IDL.Null,
    'Expired' : IDL.Null,
    'MembershipIneligible' : IDL.Null,
    'QuotaExceeded' : IDL.Null,
    'AuthRequired' : IDL.Null,
    'Pending' : IDL.Null,
  });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  const CoseStats = IDL.Record({
    'executions_today' : IDL.Nat32,
    'results' : IDL.Nat64,
    'cycles' : IDL.Nat,
    'budget_day' : IDL.Nat64,
    'accounts' : IDL.Nat64,
    'in_flight' : IDL.Nat64,
    'max_accounts' : IDL.Nat64,
    'stable_pages' : IDL.Nat64,
    'unknown' : IDL.Nat64,
    'cycles_today' : IDL.Nat,
  });
  const ExecutionGrant = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'request_id' : IDL.Vec(IDL.Nat8),
    'device_sequence' : IDL.Nat64,
    'execution_sequence' : IDL.Nat64,
    'generation' : IDL.Nat64,
    'approved_at' : IDL.Nat64,
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'home_cose' : IDL.Principal,
    'max_cycles' : IDL.Nat,
    'home_user' : IDL.Principal,
    'transport_key' : IDL.Vec(IDL.Nat8),
    'expires_at' : IDL.Nat64,
  });
  const KeyDescriptor = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'key_generation' : IDL.Nat64,
    'public_key_fingerprint' : IDL.Vec(IDL.Nat8),
    'derivation_version' : IDL.Nat16,
    'public_key' : IDL.Vec(IDL.Nat8),
    'home_cose' : IDL.Principal,
    'environment' : Environment,
    'master_key_name' : IDL.Text,
  });
  const EncryptedRootKey = IDL.Record({
    'key' : KeyDescriptor,
    'encrypted_key' : IDL.Vec(IDL.Nat8),
  });
  const ExecutionOutcome = IDL.Variant({
    'Failed' : Error,
    'Executing' : IDL.Null,
    'Authorized' : IDL.Null,
    'Unknown' : Error,
    'ResultExpired' : IDL.Null,
    'Completed' : EncryptedRootKey,
  });
  const ExecutionResult = IDL.Record({
    'request_id' : IDL.Vec(IDL.Nat8),
    'cycles_cost_upper_bound' : IDL.Nat,
    'cycles_charged' : IDL.Nat,
    'outcome' : ExecutionOutcome,
  });
  const Result_1 = IDL.Variant({ 'Ok' : ExecutionResult, 'Err' : Error });
  const Initialization = IDL.Variant({
    'Ready' : IDL.Null,
    'Uninitialized' : IDL.Null,
    'Initializing' : IDL.Null,
  });
  const KeyState = IDL.Record({
    'initialization' : Initialization,
    'error' : IDL.Opt(IDL.Text),
    'fingerprint' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'config' : CoseInit,
  });
  const Result_2 = IDL.Variant({ 'Ok' : KeyState, 'Err' : Error });
  const ExecutionCleanup = IDL.Record({
    'next_after' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'homes_scanned' : IDL.Nat32,
    'results_removed' : IDL.Nat32,
  });
  const Result_3 = IDL.Variant({ 'Ok' : KeyDescriptor, 'Err' : Error });
  const Result_4 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  return IDL.Service({
    'admin_add_user_home' : IDL.Func([IDL.Principal], [Result], []),
    'admin_set_daily_budget' : IDL.Func([IDL.Nat32, IDL.Nat], [Result], []),
    'cose_stats' : IDL.Func([], [CoseStats], ['query']),
    'execute' : IDL.Func([ExecutionGrant], [Result_1], []),
    'get_execution' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_1],
        ['query'],
      ),
    'initialize_keys' : IDL.Func([], [Result_2], []),
    'key_state' : IDL.Func([], [KeyState], ['query']),
    'prune_executions' : IDL.Func(
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [ExecutionCleanup],
        [],
      ),
    'root_public_key' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_3],
        ['query'],
      ),
    'validate_admin_add_user_home' : IDL.Func(
        [IDL.Principal],
        [Result_4],
        ['query'],
      ),
    'validate_admin_set_daily_budget' : IDL.Func(
        [IDL.Nat32, IDL.Nat],
        [Result_4],
        ['query'],
      ),
    'validate_initialize_keys' : IDL.Func([], [Result_4], ['query']),
  });
};
export const init = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const MasterKey = IDL.Record({
    'expected_fingerprint' : IDL.Vec(IDL.Nat8),
    'key_name' : IDL.Text,
  });
  const CoseInit = IDL.Record({
    'executing_canister' : IDL.Principal,
    'derivation_version' : IDL.Nat16,
    'daily_cycles' : IDL.Nat,
    'issuer_namespace' : IDL.Text,
    'daily_executions' : IDL.Nat32,
    'environment' : Environment,
    'master' : MasterKey,
    'governance' : IDL.Principal,
    'user_homes' : IDL.Vec(IDL.Principal),
  });
  return [CoseInit];
};
