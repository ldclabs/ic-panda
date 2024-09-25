export const idlFactory = ({ IDL }) => {
  const GetBlocksResult = IDL.Rec();
  const ICRC3Value = IDL.Rec();
  const UpgradeArgs = IDL.Record({
    'managers' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'name' : IDL.Opt(IDL.Text),
    'schnorr_key_name' : IDL.Opt(IDL.Text),
  });
  const InitArgs = IDL.Record({
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
    'schnorr_key_name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  const CanisterKind = IDL.Variant({
    'Cose' : IDL.Null,
    'Channel' : IDL.Null,
    'Profile' : IDL.Null,
  });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const UpdatePriceInput = IDL.Record({
    'name_l1' : IDL.Opt(IDL.Nat64),
    'name_l2' : IDL.Opt(IDL.Nat64),
    'name_l3' : IDL.Opt(IDL.Nat64),
    'name_l5' : IDL.Opt(IDL.Nat64),
    'name_l7' : IDL.Opt(IDL.Nat64),
    'channel' : IDL.Opt(IDL.Nat64),
  });
  const UserInfo = IDL.Record({
    'id' : IDL.Principal,
    'username' : IDL.Opt(IDL.Text),
    'cose_canister' : IDL.Opt(IDL.Principal),
    'name' : IDL.Text,
    'image' : IDL.Text,
    'profile_canister' : IDL.Principal,
  });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Vec(UserInfo), 'Err' : IDL.Text });
  const ChannelECDHInput = IDL.Record({
    'ecdh_remote' : IDL.Opt(IDL.Tuple(IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8))),
    'ecdh_pub' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const CreateChannelInput = IDL.Record({
    'dek' : IDL.Vec(IDL.Nat8),
    'managers' : IDL.Vec(IDL.Tuple(IDL.Principal, ChannelECDHInput)),
    'name' : IDL.Text,
    'paid' : IDL.Nat64,
    'description' : IDL.Text,
    'created_by' : IDL.Principal,
    'image' : IDL.Text,
  });
  const ChannelSetting = IDL.Record({
    'updated_at' : IDL.Nat64,
    'mute' : IDL.Bool,
    'ecdh_remote' : IDL.Opt(IDL.Tuple(IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8))),
    'unread' : IDL.Nat32,
    'last_read' : IDL.Nat32,
    'ecdh_pub' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const ChannelInfo = IDL.Record({
    'id' : IDL.Nat32,
    'dek' : IDL.Vec(IDL.Nat8),
    'gas' : IDL.Nat64,
    'updated_at' : IDL.Nat64,
    'ecdh_request' : IDL.Vec(
      IDL.Tuple(
        IDL.Principal,
        IDL.Tuple(
          IDL.Vec(IDL.Nat8),
          IDL.Opt(IDL.Tuple(IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8))),
        ),
      )
    ),
    'members' : IDL.Vec(IDL.Principal),
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
    'paid' : IDL.Nat64,
    'description' : IDL.Text,
    'created_at' : IDL.Nat64,
    'created_by' : IDL.Principal,
    'deleted_messages' : IDL.Vec(IDL.Nat32),
    'canister' : IDL.Principal,
    'image' : IDL.Text,
    'message_start' : IDL.Nat32,
    'latest_message_at' : IDL.Nat64,
    'latest_message_by' : IDL.Principal,
    'latest_message_id' : IDL.Nat32,
    'my_setting' : ChannelSetting,
  });
  const Result_2 = IDL.Variant({ 'Ok' : ChannelInfo, 'Err' : IDL.Text });
  const Result_3 = IDL.Variant({ 'Ok' : UserInfo, 'Err' : IDL.Text });
  const CanisterStatusType = IDL.Variant({
    'stopped' : IDL.Null,
    'stopping' : IDL.Null,
    'running' : IDL.Null,
  });
  const LogVisibility = IDL.Variant({
    'controllers' : IDL.Null,
    'public' : IDL.Null,
  });
  const DefiniteCanisterSettings = IDL.Record({
    'freezing_threshold' : IDL.Nat,
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
  const CanisterStatusResponse = IDL.Record({
    'status' : CanisterStatusType,
    'memory_size' : IDL.Nat,
    'cycles' : IDL.Nat,
    'settings' : DefiniteCanisterSettings,
    'query_stats' : QueryStats,
    'idle_cycles_burned_per_day' : IDL.Nat,
    'module_hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'reserved_cycles' : IDL.Nat,
  });
  const Result_4 = IDL.Variant({
    'Ok' : CanisterStatusResponse,
    'Err' : IDL.Text,
  });
  const Price = IDL.Record({
    'name_l1' : IDL.Nat64,
    'name_l2' : IDL.Nat64,
    'name_l3' : IDL.Nat64,
    'name_l5' : IDL.Nat64,
    'name_l7' : IDL.Nat64,
    'channel' : IDL.Nat64,
  });
  const StateInfo = IDL.Record({
    'latest_usernames' : IDL.Vec(IDL.Text),
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
    'profile_canisters' : IDL.Vec(IDL.Principal),
    'names_total' : IDL.Nat64,
    'transfer_out_total' : IDL.Nat,
    'next_block_height' : IDL.Nat64,
    'matured_channel_canisters' : IDL.Vec(IDL.Principal),
    'users_total' : IDL.Nat64,
    'price' : Price,
    'next_block_phash' : IDL.Vec(IDL.Nat8),
    'cose_canisters' : IDL.Vec(IDL.Principal),
    'incoming_total' : IDL.Nat,
    'channel_canisters' : IDL.Vec(IDL.Principal),
  });
  const Result_5 = IDL.Variant({ 'Ok' : StateInfo, 'Err' : IDL.Text });
  const GetArchivesArgs = IDL.Record({ 'from' : IDL.Opt(IDL.Principal) });
  const ICRC3ArchiveInfo = IDL.Record({
    'end' : IDL.Nat,
    'canister_id' : IDL.Principal,
    'start' : IDL.Nat,
  });
  const GetBlocksRequest = IDL.Record({
    'start' : IDL.Nat,
    'length' : IDL.Nat,
  });
  ICRC3Value.fill(
    IDL.Variant({
      'Int' : IDL.Int,
      'Map' : IDL.Vec(IDL.Tuple(IDL.Text, ICRC3Value)),
      'Nat' : IDL.Nat,
      'Blob' : IDL.Vec(IDL.Nat8),
      'Text' : IDL.Text,
      'Array' : IDL.Vec(ICRC3Value),
    })
  );
  const BlockWithId = IDL.Record({ 'id' : IDL.Nat, 'block' : ICRC3Value });
  const ArchivedBlocks = IDL.Record({
    'args' : IDL.Vec(GetBlocksRequest),
    'callback' : IDL.Func(
        [IDL.Vec(GetBlocksRequest)],
        [GetBlocksResult],
        ['query'],
      ),
  });
  GetBlocksResult.fill(
    IDL.Record({
      'log_length' : IDL.Nat,
      'blocks' : IDL.Vec(BlockWithId),
      'archived_blocks' : IDL.Vec(ArchivedBlocks),
    })
  );
  const ICRC3DataCertificate = IDL.Record({
    'certificate' : IDL.Vec(IDL.Nat8),
    'hash_tree' : IDL.Vec(IDL.Nat8),
  });
  const SupportedBlockType = IDL.Record({
    'url' : IDL.Text,
    'block_type' : IDL.Text,
  });
  const Result_6 = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Nat8), 'Err' : IDL.Text });
  const ChannelKEKInput = IDL.Record({
    'id' : IDL.Nat32,
    'kek' : IDL.Vec(IDL.Nat8),
    'canister' : IDL.Principal,
  });
  const Result_7 = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Text), 'Err' : IDL.Text });
  const UpdateKVInput = IDL.Record({
    'upsert_kv' : IDL.Vec(IDL.Tuple(IDL.Text, IDL.Vec(IDL.Nat8))),
    'remove_kv' : IDL.Vec(IDL.Text),
  });
  const Result_8 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  return IDL.Service({
    'admin_add_canister' : IDL.Func(
        [CanisterKind, IDL.Principal],
        [Result],
        [],
      ),
    'admin_add_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_collect_token' : IDL.Func([Account, IDL.Nat], [Result], []),
    'admin_remove_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_update_price' : IDL.Func([UpdatePriceInput], [Result], []),
    'batch_get_users' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_1],
        ['query'],
      ),
    'create_channel' : IDL.Func([CreateChannelInput], [Result_2], []),
    'get_by_username' : IDL.Func([IDL.Text], [Result_3], ['query']),
    'get_canister_status' : IDL.Func([], [Result_4], ['query']),
    'get_state' : IDL.Func([], [Result_5], ['query']),
    'get_user' : IDL.Func([IDL.Opt(IDL.Principal)], [Result_3], ['query']),
    'icrc3_get_archives' : IDL.Func(
        [GetArchivesArgs],
        [IDL.Vec(ICRC3ArchiveInfo)],
        ['query'],
      ),
    'icrc3_get_blocks' : IDL.Func(
        [IDL.Vec(GetBlocksRequest)],
        [GetBlocksResult],
        ['query'],
      ),
    'icrc3_get_tip_certificate' : IDL.Func(
        [],
        [IDL.Opt(ICRC3DataCertificate)],
        ['query'],
      ),
    'icrc3_supported_block_types' : IDL.Func(
        [],
        [IDL.Vec(SupportedBlockType)],
        ['query'],
      ),
    'my_iv' : IDL.Func([], [Result_6], ['query']),
    'register_username' : IDL.Func(
        [IDL.Text, IDL.Opt(IDL.Text)],
        [Result_3],
        [],
      ),
    'save_channel_kek' : IDL.Func([ChannelKEKInput], [Result], []),
    'search_username' : IDL.Func([IDL.Text], [Result_7], ['query']),
    'transfer_username' : IDL.Func([IDL.Principal], [Result], []),
    'update_my_ecdh' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result],
        [],
      ),
    'update_my_image' : IDL.Func([IDL.Text], [Result], []),
    'update_my_kv' : IDL.Func([UpdateKVInput], [Result], []),
    'update_my_name' : IDL.Func([IDL.Text], [Result_3], []),
    'validate2_admin_update_price' : IDL.Func(
        [UpdatePriceInput],
        [Result_8],
        [],
      ),
    'validate_admin_add_canister' : IDL.Func(
        [CanisterKind, IDL.Principal],
        [Result],
        [],
      ),
    'validate_admin_add_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'validate_admin_collect_token' : IDL.Func([Account, IDL.Nat], [Result], []),
    'validate_admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'validate_admin_update_price' : IDL.Func([UpdatePriceInput], [Result], []),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'managers' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'name' : IDL.Opt(IDL.Text),
    'schnorr_key_name' : IDL.Opt(IDL.Text),
  });
  const InitArgs = IDL.Record({
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
    'schnorr_key_name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  return [IDL.Opt(ChainArgs)];
};
