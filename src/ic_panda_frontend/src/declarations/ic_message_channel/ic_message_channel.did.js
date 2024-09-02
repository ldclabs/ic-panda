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
  const AddMessageInput = IDL.Record({
    'reply_to' : IDL.Opt(IDL.Nat32),
    'channel' : IDL.Nat32,
    'payload' : IDL.Vec(IDL.Nat8),
  });
  const AddMessageOutput = IDL.Record({
    'id' : IDL.Nat32,
    'kind' : IDL.Nat8,
    'created_at' : IDL.Nat64,
    'channel' : IDL.Nat32,
  });
  const Result = IDL.Variant({ 'Ok' : AddMessageOutput, 'Err' : IDL.Text });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
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
    'members' : IDL.Vec(IDL.Principal),
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
    'paid' : IDL.Nat64,
    'description' : IDL.Text,
    'created_at' : IDL.Nat64,
    'created_by' : IDL.Principal,
    'canister' : IDL.Principal,
    'image' : IDL.Text,
    'message_start' : IDL.Nat32,
    'latest_message_at' : IDL.Nat64,
    'latest_message_by' : IDL.Principal,
    'latest_message_id' : IDL.Nat32,
    'my_setting' : ChannelSetting,
  });
  const Result_2 = IDL.Variant({ 'Ok' : ChannelInfo, 'Err' : IDL.Text });
  const ChannelBasicInfo = IDL.Record({
    'id' : IDL.Nat32,
    'gas' : IDL.Nat64,
    'updated_at' : IDL.Nat64,
    'name' : IDL.Text,
    'paid' : IDL.Nat64,
    'canister' : IDL.Principal,
    'image' : IDL.Text,
    'latest_message_at' : IDL.Nat64,
    'latest_message_by' : IDL.Principal,
    'latest_message_id' : IDL.Nat32,
    'my_setting' : ChannelSetting,
  });
  const Result_3 = IDL.Variant({
    'Ok' : IDL.Vec(ChannelBasicInfo),
    'Err' : IDL.Text,
  });
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
  const Result_5 = IDL.Variant({
    'Ok' : IDL.Opt(ChannelInfo),
    'Err' : IDL.Text,
  });
  const Message = IDL.Record({
    'id' : IDL.Nat32,
    'reply_to' : IDL.Nat32,
    'kind' : IDL.Nat8,
    'created_at' : IDL.Nat64,
    'created_by' : IDL.Principal,
    'canister' : IDL.Principal,
    'channel' : IDL.Nat32,
    'payload' : IDL.Vec(IDL.Nat8),
  });
  const Result_6 = IDL.Variant({ 'Ok' : Message, 'Err' : IDL.Text });
  const StateInfo = IDL.Record({
    'channel_id' : IDL.Nat32,
    'incoming_gas' : IDL.Nat,
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
    'burned_gas' : IDL.Nat,
    'channels_total' : IDL.Nat64,
    'messages_total' : IDL.Nat64,
  });
  const Result_7 = IDL.Variant({ 'Ok' : StateInfo, 'Err' : IDL.Text });
  const Result_8 = IDL.Variant({ 'Ok' : IDL.Vec(Message), 'Err' : IDL.Text });
  const UpdateMySettingInput = IDL.Record({
    'id' : IDL.Nat32,
    'ecdh' : IDL.Opt(ChannelECDHInput),
    'mute' : IDL.Opt(IDL.Bool),
    'last_read' : IDL.Opt(IDL.Nat32),
  });
  const UpdateChannelMemberInput = IDL.Record({
    'id' : IDL.Nat32,
    'member' : IDL.Principal,
    'ecdh' : ChannelECDHInput,
  });
  const UpdateChannelInput = IDL.Record({
    'id' : IDL.Nat32,
    'name' : IDL.Opt(IDL.Text),
    'description' : IDL.Opt(IDL.Text),
    'image' : IDL.Opt(IDL.Text),
  });
  const Result_9 = IDL.Variant({
    'Ok' : IDL.Tuple(IDL.Nat64, IDL.Opt(Message)),
    'Err' : IDL.Text,
  });
  return IDL.Service({
    'add_message' : IDL.Func([AddMessageInput], [Result], []),
    'admin_add_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result_1], []),
    'admin_create_channel' : IDL.Func([CreateChannelInput], [Result_2], []),
    'admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_1],
        [],
      ),
    'batch_get_channels' : IDL.Func(
        [IDL.Vec(IDL.Nat32)],
        [Result_3],
        ['query'],
      ),
    'get_canister_status' : IDL.Func([], [Result_4], ['query']),
    'get_channel_if_update' : IDL.Func(
        [IDL.Nat32, IDL.Nat64],
        [Result_5],
        ['query'],
      ),
    'get_message' : IDL.Func([IDL.Nat32, IDL.Nat32], [Result_6], ['query']),
    'get_state' : IDL.Func([], [Result_7], ['query']),
    'list_messages' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Nat32), IDL.Opt(IDL.Nat32)],
        [Result_8],
        ['query'],
      ),
    'my_channels_if_update' : IDL.Func(
        [IDL.Opt(IDL.Nat64)],
        [Result_3],
        ['query'],
      ),
    'quit_channel' : IDL.Func([UpdateMySettingInput, IDL.Bool], [Result_1], []),
    'remove_member' : IDL.Func([UpdateChannelMemberInput], [Result_1], []),
    'update_channel' : IDL.Func([UpdateChannelInput], [Result_6], []),
    'update_manager' : IDL.Func([UpdateChannelMemberInput], [Result_9], []),
    'update_member' : IDL.Func([UpdateChannelMemberInput], [Result_9], []),
    'update_my_setting' : IDL.Func([UpdateMySettingInput], [Result_1], []),
    'validate_admin_add_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_1],
        [],
      ),
    'validate_admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_1],
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
