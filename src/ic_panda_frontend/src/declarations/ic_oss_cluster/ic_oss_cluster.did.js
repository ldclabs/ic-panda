export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'name' : IDL.Opt(IDL.Text),
    'token_expiration' : IDL.Opt(IDL.Nat64),
    'bucket_topup_threshold' : IDL.Opt(IDL.Nat),
    'bucket_topup_amount' : IDL.Opt(IDL.Nat),
  });
  const InitArgs = IDL.Record({
    'ecdsa_key_name' : IDL.Text,
    'name' : IDL.Text,
    'token_expiration' : IDL.Nat64,
    'bucket_topup_threshold' : IDL.Nat,
    'bucket_topup_amount' : IDL.Nat,
    'schnorr_key_name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  const Result = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Nat8), 'Err' : IDL.Text });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const AddWasmInput = IDL.Record({
    'wasm' : IDL.Vec(IDL.Nat8),
    'description' : IDL.Text,
  });
  const Token = IDL.Record({
    'subject' : IDL.Principal,
    'audience' : IDL.Principal,
    'policies' : IDL.Text,
  });
  const Result_2 = IDL.Variant({
    'Ok' : IDL.Vec(IDL.Vec(IDL.Nat8)),
    'Err' : IDL.Text,
  });
  const DeployWasmInput = IDL.Record({
    'args' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'canister' : IDL.Principal,
  });
  const Result_3 = IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text });
  const BucketDeploymentInfo = IDL.Record({
    'args' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'prev_hash' : IDL.Vec(IDL.Nat8),
    'error' : IDL.Opt(IDL.Text),
    'deploy_at' : IDL.Nat64,
    'canister' : IDL.Principal,
    'wasm_hash' : IDL.Vec(IDL.Nat8),
  });
  const Result_4 = IDL.Variant({
    'Ok' : IDL.Vec(BucketDeploymentInfo),
    'Err' : IDL.Text,
  });
  const WasmInfo = IDL.Record({
    'hash' : IDL.Vec(IDL.Nat8),
    'wasm' : IDL.Vec(IDL.Nat8),
    'description' : IDL.Text,
    'created_at' : IDL.Nat64,
    'created_by' : IDL.Principal,
  });
  const Result_5 = IDL.Variant({ 'Ok' : WasmInfo, 'Err' : IDL.Text });
  const Result_6 = IDL.Variant({
    'Ok' : IDL.Vec(IDL.Principal),
    'Err' : IDL.Text,
  });
  const ClusterInfo = IDL.Record({
    'ecdsa_token_public_key' : IDL.Text,
    'schnorr_ed25519_token_public_key' : IDL.Text,
    'bucket_wasm_total' : IDL.Nat64,
    'ecdsa_key_name' : IDL.Text,
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
    'bucket_deployed_total' : IDL.Nat64,
    'token_expiration' : IDL.Nat64,
    'weak_ed25519_token_public_key' : IDL.Text,
    'bucket_latest_version' : IDL.Vec(IDL.Nat8),
    'schnorr_key_name' : IDL.Text,
    'bucket_deployment_logs' : IDL.Nat64,
    'subject_authz_total' : IDL.Nat64,
  });
  const Result_7 = IDL.Variant({ 'Ok' : ClusterInfo, 'Err' : IDL.Text });
  const Result_8 = IDL.Variant({
    'Ok' : IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Text)),
    'Err' : IDL.Text,
  });
  const Result_9 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  return IDL.Service({
    'access_token' : IDL.Func([IDL.Principal], [Result], []),
    'admin_add_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result_1], []),
    'admin_add_wasm' : IDL.Func(
        [AddWasmInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_1],
        [],
      ),
    'admin_attach_policies' : IDL.Func([Token], [Result_1], []),
    'admin_batch_call_buckets' : IDL.Func(
        [IDL.Vec(IDL.Principal), IDL.Text, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_2],
        [],
      ),
    'admin_deploy_bucket' : IDL.Func(
        [DeployWasmInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_1],
        [],
      ),
    'admin_detach_policies' : IDL.Func([Token], [Result_1], []),
    'admin_ed25519_access_token' : IDL.Func([Token], [Result], []),
    'admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_1],
        [],
      ),
    'admin_set_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result_1], []),
    'admin_sign_access_token' : IDL.Func([Token], [Result], []),
    'admin_topup_all_buckets' : IDL.Func([], [Result_3], []),
    'admin_upgrade_all_buckets' : IDL.Func(
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_1],
        [],
      ),
    'admin_weak_access_token' : IDL.Func(
        [Token, IDL.Nat64, IDL.Nat64],
        [Result],
        ['query'],
      ),
    'bucket_deployment_logs' : IDL.Func(
        [IDL.Opt(IDL.Nat), IDL.Opt(IDL.Nat)],
        [Result_4],
        ['query'],
      ),
    'ed25519_access_token' : IDL.Func([IDL.Principal], [Result], []),
    'get_bucket_wasm' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_5], ['query']),
    'get_buckets' : IDL.Func([], [Result_6], ['query']),
    'get_cluster_info' : IDL.Func([], [Result_7], ['query']),
    'get_deployed_buckets' : IDL.Func([], [Result_4], ['query']),
    'get_subject_policies' : IDL.Func([IDL.Principal], [Result_8], ['query']),
    'get_subject_policies_for' : IDL.Func(
        [IDL.Principal, IDL.Principal],
        [Result_9],
        ['query'],
      ),
    'validate2_admin_add_wasm' : IDL.Func(
        [AddWasmInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_9],
        [],
      ),
    'validate2_admin_batch_call_buckets' : IDL.Func(
        [IDL.Vec(IDL.Principal), IDL.Text, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_9],
        [],
      ),
    'validate2_admin_deploy_bucket' : IDL.Func(
        [DeployWasmInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_9],
        [],
      ),
    'validate2_admin_set_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_9],
        [],
      ),
    'validate2_admin_upgrade_all_buckets' : IDL.Func(
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_9],
        [],
      ),
    'validate_admin_add_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_9],
        [],
      ),
    'validate_admin_add_wasm' : IDL.Func(
        [AddWasmInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_1],
        [],
      ),
    'validate_admin_batch_call_buckets' : IDL.Func(
        [IDL.Vec(IDL.Principal), IDL.Text, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_2],
        [],
      ),
    'validate_admin_deploy_bucket' : IDL.Func(
        [DeployWasmInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_1],
        [],
      ),
    'validate_admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_9],
        [],
      ),
    'validate_admin_set_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_1],
        [],
      ),
    'validate_admin_upgrade_all_buckets' : IDL.Func(
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_1],
        [],
      ),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'name' : IDL.Opt(IDL.Text),
    'token_expiration' : IDL.Opt(IDL.Nat64),
    'bucket_topup_threshold' : IDL.Opt(IDL.Nat),
    'bucket_topup_amount' : IDL.Opt(IDL.Nat),
  });
  const InitArgs = IDL.Record({
    'ecdsa_key_name' : IDL.Text,
    'name' : IDL.Text,
    'token_expiration' : IDL.Nat64,
    'bucket_topup_threshold' : IDL.Nat,
    'bucket_topup_amount' : IDL.Nat,
    'schnorr_key_name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  return [IDL.Opt(ChainArgs)];
};
