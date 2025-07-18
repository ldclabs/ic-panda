export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'managers' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'name' : IDL.Opt(IDL.Text),
  });
  const InitArgs = IDL.Record({
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  const CanisterKind = IDL.Variant({
    'OssBucket' : IDL.Null,
    'OssCluster' : IDL.Null,
  });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const MemoryMetrics = IDL.Record({
    'wasm_binary_size' : IDL.Nat,
    'wasm_chunk_store_size' : IDL.Nat,
    'canister_history_size' : IDL.Nat,
    'stable_memory_size' : IDL.Nat,
    'snapshots_size' : IDL.Nat,
    'wasm_memory_size' : IDL.Nat,
    'global_memory_size' : IDL.Nat,
    'custom_sections_size' : IDL.Nat,
  });
  const CanisterStatusType = IDL.Variant({
    'stopped' : IDL.Null,
    'stopping' : IDL.Null,
    'running' : IDL.Null,
  });
  const LogVisibility = IDL.Variant({
    'controllers' : IDL.Null,
    'public' : IDL.Null,
    'allowed_viewers' : IDL.Vec(IDL.Principal),
  });
  const DefiniteCanisterSettings = IDL.Record({
    'freezing_threshold' : IDL.Nat,
    'wasm_memory_threshold' : IDL.Nat,
    'controllers' : IDL.Vec(IDL.Principal),
    'reserved_cycles_limit' : IDL.Nat,
    'log_visibility' : LogVisibility,
    'wasm_memory_limit' : IDL.Nat,
    'memory_allocation' : IDL.Nat,
    'compute_allocation' : IDL.Nat,
  });
  const QueryStats = IDL.Record({
    'response_payload_bytes_total' : IDL.Nat,
    'num_instructions_total' : IDL.Nat,
    'num_calls_total' : IDL.Nat,
    'request_payload_bytes_total' : IDL.Nat,
  });
  const CanisterStatusResult = IDL.Record({
    'memory_metrics' : MemoryMetrics,
    'status' : CanisterStatusType,
    'memory_size' : IDL.Nat,
    'cycles' : IDL.Nat,
    'settings' : DefiniteCanisterSettings,
    'query_stats' : QueryStats,
    'idle_cycles_burned_per_day' : IDL.Nat,
    'module_hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'reserved_cycles' : IDL.Nat,
  });
  const Result_1 = IDL.Variant({
    'Ok' : CanisterStatusResult,
    'Err' : IDL.Text,
  });
  const ChannelSetting = IDL.Record({
    'pin' : IDL.Nat32,
    'alias' : IDL.Text,
    'tags' : IDL.Vec(IDL.Text),
  });
  const Link = IDL.Record({
    'uri' : IDL.Text,
    'title' : IDL.Text,
    'image' : IDL.Opt(IDL.Text),
  });
  const ProfileInfo = IDL.Record({
    'id' : IDL.Principal,
    'bio' : IDL.Text,
    'active_at' : IDL.Nat64,
    'created_at' : IDL.Nat64,
    'channels' : IDL.Opt(
      IDL.Vec(IDL.Tuple(IDL.Tuple(IDL.Principal, IDL.Nat64), ChannelSetting))
    ),
    'image_file' : IDL.Opt(IDL.Tuple(IDL.Principal, IDL.Nat32)),
    'links' : IDL.Vec(Link),
    'tokens' : IDL.Vec(IDL.Principal),
    'canister' : IDL.Principal,
    'ecdh_pub' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'following' : IDL.Opt(IDL.Vec(IDL.Principal)),
  });
  const Result_2 = IDL.Variant({ 'Ok' : ProfileInfo, 'Err' : IDL.Text });
  const StateInfo = IDL.Record({
    'managers' : IDL.Vec(IDL.Principal),
    'profiles_total' : IDL.Nat64,
    'name' : IDL.Text,
    'ic_oss_cluster' : IDL.Opt(IDL.Principal),
    'ic_oss_buckets' : IDL.Vec(IDL.Principal),
  });
  const Result_3 = IDL.Variant({ 'Ok' : StateInfo, 'Err' : IDL.Text });
  const UpdateProfileInput = IDL.Record({
    'bio' : IDL.Opt(IDL.Text),
    'remove_channels' : IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Nat64)),
    'upsert_channels' : IDL.Vec(
      IDL.Tuple(IDL.Tuple(IDL.Principal, IDL.Nat64), ChannelSetting)
    ),
    'follow' : IDL.Vec(IDL.Principal),
    'unfollow' : IDL.Vec(IDL.Principal),
  });
  const UploadImageInput = IDL.Record({
    'size' : IDL.Nat64,
    'content_type' : IDL.Text,
  });
  const UploadImageOutput = IDL.Record({
    'name' : IDL.Text,
    'image' : IDL.Tuple(IDL.Principal, IDL.Nat32),
    'access_token' : IDL.Vec(IDL.Nat8),
  });
  const Result_4 = IDL.Variant({ 'Ok' : UploadImageOutput, 'Err' : IDL.Text });
  const Result_5 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  return IDL.Service({
    'admin_add_canister' : IDL.Func(
        [CanisterKind, IDL.Principal],
        [Result],
        [],
      ),
    'admin_add_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_remove_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_update_profile_ecdh_pub' : IDL.Func(
        [IDL.Principal, IDL.Vec(IDL.Nat8)],
        [Result],
        [],
      ),
    'admin_upsert_profile' : IDL.Func(
        [IDL.Principal, IDL.Opt(IDL.Tuple(IDL.Principal, IDL.Nat64))],
        [Result],
        [],
      ),
    'get_canister_status' : IDL.Func([], [Result_1], ['query']),
    'get_profile' : IDL.Func([IDL.Opt(IDL.Principal)], [Result_2], ['query']),
    'get_state' : IDL.Func([], [Result_3], ['query']),
    'update_links' : IDL.Func([IDL.Vec(Link)], [Result], []),
    'update_profile' : IDL.Func([UpdateProfileInput], [Result_2], []),
    'update_profile_ecdh_pub' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'update_tokens' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'upload_image_token' : IDL.Func([UploadImageInput], [Result_4], []),
    'validate2_admin_add_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_5],
        [],
      ),
    'validate2_admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_5],
        [],
      ),
    'validate_admin_add_canister' : IDL.Func(
        [CanisterKind, IDL.Principal],
        [Result_5],
        [],
      ),
    'validate_admin_add_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'validate_admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'managers' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'name' : IDL.Opt(IDL.Text),
  });
  const InitArgs = IDL.Record({
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  return [IDL.Opt(ChainArgs)];
};
