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
  const CanisterKind = IDL.Variant({
    'OssBucket' : IDL.Null,
    'OssCluster' : IDL.Null,
  });
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
  const ChannelFilesState = IDL.Record({
    'files_size_total' : IDL.Nat64,
    'file_max_size' : IDL.Nat64,
    'files_total' : IDL.Nat64,
    'file_storage' : IDL.Tuple(IDL.Principal, IDL.Nat32),
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
    'files_state' : IDL.Opt(ChannelFilesState),
    'my_setting' : ChannelSetting,
  });
  const Result_2 = IDL.Variant({ 'Ok' : ChannelInfo, 'Err' : IDL.Text });
  const ChannelTopupInput = IDL.Record({
    'id' : IDL.Nat32,
    'canister' : IDL.Principal,
    'payer' : IDL.Principal,
    'amount' : IDL.Nat64,
  });
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
  const DeleteMessageInput = IDL.Record({
    'id' : IDL.Nat32,
    'channel' : IDL.Nat32,
  });
  const DownloadFilesToken = IDL.Record({
    'storage' : IDL.Tuple(IDL.Principal, IDL.Nat32),
    'access_token' : IDL.Vec(IDL.Nat8),
  });
  const Result_4 = IDL.Variant({ 'Ok' : DownloadFilesToken, 'Err' : IDL.Text });
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
  const Result_5 = IDL.Variant({
    'Ok' : CanisterStatusResponse,
    'Err' : IDL.Text,
  });
  const Result_6 = IDL.Variant({
    'Ok' : IDL.Opt(ChannelInfo),
    'Err' : IDL.Text,
  });
  const Message = IDL.Record({
    'id' : IDL.Nat32,
    'reply_to' : IDL.Nat32,
    'kind' : IDL.Nat8,
    'created_at' : IDL.Nat64,
    'created_by' : IDL.Principal,
    'payload' : IDL.Vec(IDL.Nat8),
  });
  const Result_7 = IDL.Variant({ 'Ok' : Message, 'Err' : IDL.Text });
  const StateInfo = IDL.Record({
    'channel_id' : IDL.Nat32,
    'incoming_gas' : IDL.Nat,
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
    'ic_oss_cluster' : IDL.Opt(IDL.Principal),
    'ic_oss_buckets' : IDL.Vec(IDL.Principal),
    'burned_gas' : IDL.Nat,
    'channels_total' : IDL.Nat64,
    'messages_total' : IDL.Nat64,
  });
  const Result_8 = IDL.Variant({ 'Ok' : StateInfo, 'Err' : IDL.Text });
  const UpdateMySettingInput = IDL.Record({
    'id' : IDL.Nat32,
    'ecdh' : IDL.Opt(ChannelECDHInput),
    'mute' : IDL.Opt(IDL.Bool),
    'last_read' : IDL.Opt(IDL.Nat32),
  });
  const Result_9 = IDL.Variant({ 'Ok' : IDL.Vec(Message), 'Err' : IDL.Text });
  const Result_10 = IDL.Variant({
    'Ok' : IDL.Vec(IDL.Nat32),
    'Err' : IDL.Text,
  });
  const UpdateChannelMemberInput = IDL.Record({
    'id' : IDL.Nat32,
    'member' : IDL.Principal,
    'ecdh' : ChannelECDHInput,
  });
  const TruncateMessageInput = IDL.Record({
    'to' : IDL.Nat32,
    'channel' : IDL.Nat32,
  });
  const UpdateChannelInput = IDL.Record({
    'id' : IDL.Nat32,
    'name' : IDL.Opt(IDL.Text),
    'description' : IDL.Opt(IDL.Text),
    'image' : IDL.Opt(IDL.Text),
  });
  const Result_11 = IDL.Variant({
    'Ok' : IDL.Tuple(IDL.Nat64, IDL.Opt(Message)),
    'Err' : IDL.Text,
  });
  const Result_12 = IDL.Variant({ 'Ok' : ChannelSetting, 'Err' : IDL.Text });
  const UpdateChannelStorageInput = IDL.Record({
    'id' : IDL.Nat32,
    'file_max_size' : IDL.Nat64,
  });
  const UploadFileInput = IDL.Record({
    'size' : IDL.Nat64,
    'content_type' : IDL.Text,
    'channel' : IDL.Nat32,
  });
  const UploadFileOutput = IDL.Record({
    'id' : IDL.Nat32,
    'storage' : IDL.Tuple(IDL.Principal, IDL.Nat32),
    'name' : IDL.Text,
    'access_token' : IDL.Vec(IDL.Nat8),
  });
  const Result_13 = IDL.Variant({ 'Ok' : UploadFileOutput, 'Err' : IDL.Text });
  const Result_14 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  return IDL.Service({
    'add_message' : IDL.Func([AddMessageInput], [Result], []),
    'admin_add_canister' : IDL.Func(
        [CanisterKind, IDL.Principal],
        [Result_1],
        [],
      ),
    'admin_add_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result_1], []),
    'admin_create_channel' : IDL.Func([CreateChannelInput], [Result_2], []),
    'admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_1],
        [],
      ),
    'admin_topup_channel' : IDL.Func([ChannelTopupInput], [Result_2], []),
    'batch_get_channels' : IDL.Func(
        [IDL.Vec(IDL.Nat32)],
        [Result_3],
        ['query'],
      ),
    'delete_message' : IDL.Func([DeleteMessageInput], [Result_1], []),
    'download_files_token' : IDL.Func([IDL.Nat32], [Result_4], []),
    'get_canister_status' : IDL.Func([], [Result_5], ['query']),
    'get_channel_if_update' : IDL.Func(
        [IDL.Nat32, IDL.Nat64],
        [Result_6],
        ['query'],
      ),
    'get_message' : IDL.Func([IDL.Nat32, IDL.Nat32], [Result_7], ['query']),
    'get_state' : IDL.Func([], [Result_8], ['query']),
    'leave_channel' : IDL.Func(
        [UpdateMySettingInput, IDL.Bool],
        [Result_1],
        [],
      ),
    'list_messages' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Nat32), IDL.Opt(IDL.Nat32)],
        [Result_9],
        ['query'],
      ),
    'my_channel_ids' : IDL.Func([], [Result_10], ['query']),
    'my_channels_if_update' : IDL.Func(
        [IDL.Opt(IDL.Nat64)],
        [Result_3],
        ['query'],
      ),
    'remove_member' : IDL.Func([UpdateChannelMemberInput], [Result_1], []),
    'truncate_messages' : IDL.Func([TruncateMessageInput], [Result_1], []),
    'update_channel' : IDL.Func([UpdateChannelInput], [Result_7], []),
    'update_manager' : IDL.Func([UpdateChannelMemberInput], [Result_11], []),
    'update_member' : IDL.Func([UpdateChannelMemberInput], [Result_11], []),
    'update_my_setting' : IDL.Func([UpdateMySettingInput], [Result_12], []),
    'update_storage' : IDL.Func([UpdateChannelStorageInput], [Result_7], []),
    'upload_file_token' : IDL.Func([UploadFileInput], [Result_13], []),
    'upload_image_token' : IDL.Func([UploadFileInput], [Result_13], []),
    'validate2_admin_add_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_14],
        [],
      ),
    'validate2_admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_14],
        [],
      ),
    'validate_admin_add_canister' : IDL.Func(
        [CanisterKind, IDL.Principal],
        [Result_14],
        [],
      ),
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
