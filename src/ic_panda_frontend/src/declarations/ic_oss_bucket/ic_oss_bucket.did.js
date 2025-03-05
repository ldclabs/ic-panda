export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'governance_canister' : IDL.Opt(IDL.Principal),
    'max_custom_data_size' : IDL.Opt(IDL.Nat16),
    'max_children' : IDL.Opt(IDL.Nat16),
    'enable_hash_index' : IDL.Opt(IDL.Bool),
    'max_file_size' : IDL.Opt(IDL.Nat64),
    'max_folder_depth' : IDL.Opt(IDL.Nat8),
  });
  const InitArgs = IDL.Record({
    'governance_canister' : IDL.Opt(IDL.Principal),
    'name' : IDL.Text,
    'max_custom_data_size' : IDL.Nat16,
    'max_children' : IDL.Nat16,
    'enable_hash_index' : IDL.Bool,
    'max_file_size' : IDL.Nat64,
    'visibility' : IDL.Nat8,
    'max_folder_depth' : IDL.Nat8,
    'file_id' : IDL.Nat32,
  });
  const CanisterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const UpdateBucketInput = IDL.Record({
    'status' : IDL.Opt(IDL.Int8),
    'trusted_eddsa_pub_keys' : IDL.Opt(IDL.Vec(IDL.Vec(IDL.Nat8))),
    'name' : IDL.Opt(IDL.Text),
    'max_custom_data_size' : IDL.Opt(IDL.Nat16),
    'max_children' : IDL.Opt(IDL.Nat16),
    'enable_hash_index' : IDL.Opt(IDL.Bool),
    'max_file_size' : IDL.Opt(IDL.Nat64),
    'visibility' : IDL.Opt(IDL.Nat8),
    'max_folder_depth' : IDL.Opt(IDL.Nat8),
    'trusted_ecdsa_pub_keys' : IDL.Opt(IDL.Vec(IDL.Vec(IDL.Nat8))),
  });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Nat32), 'Err' : IDL.Text });
  const MetadataValue = IDL.Variant({
    'Int' : IDL.Int,
    'Nat' : IDL.Nat,
    'Blob' : IDL.Vec(IDL.Nat8),
    'Text' : IDL.Text,
  });
  const CreateFileInput = IDL.Record({
    'dek' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'status' : IDL.Opt(IDL.Int8),
    'content' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'custom' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, MetadataValue))),
    'hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Text,
    'size' : IDL.Opt(IDL.Nat64),
    'content_type' : IDL.Text,
    'parent' : IDL.Nat32,
  });
  const CreateFileOutput = IDL.Record({
    'id' : IDL.Nat32,
    'created_at' : IDL.Nat64,
  });
  const Result_2 = IDL.Variant({ 'Ok' : CreateFileOutput, 'Err' : IDL.Text });
  const CreateFolderInput = IDL.Record({
    'name' : IDL.Text,
    'parent' : IDL.Nat32,
  });
  const Result_3 = IDL.Variant({ 'Ok' : IDL.Bool, 'Err' : IDL.Text });
  const BucketInfo = IDL.Record({
    'status' : IDL.Int8,
    'total_chunks' : IDL.Nat64,
    'trusted_eddsa_pub_keys' : IDL.Vec(IDL.Vec(IDL.Nat8)),
    'managers' : IDL.Vec(IDL.Principal),
    'governance_canister' : IDL.Opt(IDL.Principal),
    'name' : IDL.Text,
    'max_custom_data_size' : IDL.Nat16,
    'auditors' : IDL.Vec(IDL.Principal),
    'total_files' : IDL.Nat64,
    'max_children' : IDL.Nat16,
    'enable_hash_index' : IDL.Bool,
    'max_file_size' : IDL.Nat64,
    'folder_id' : IDL.Nat32,
    'visibility' : IDL.Nat8,
    'max_folder_depth' : IDL.Nat8,
    'trusted_ecdsa_pub_keys' : IDL.Vec(IDL.Vec(IDL.Nat8)),
    'total_folders' : IDL.Nat64,
    'file_id' : IDL.Nat32,
  });
  const Result_4 = IDL.Variant({ 'Ok' : BucketInfo, 'Err' : IDL.Text });
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
  const FolderName = IDL.Record({ 'id' : IDL.Nat32, 'name' : IDL.Text });
  const Result_6 = IDL.Variant({
    'Ok' : IDL.Vec(FolderName),
    'Err' : IDL.Text,
  });
  const Result_7 = IDL.Variant({
    'Ok' : IDL.Vec(IDL.Tuple(IDL.Nat32, IDL.Vec(IDL.Nat8))),
    'Err' : IDL.Text,
  });
  const FileInfo = IDL.Record({
    'ex' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, MetadataValue))),
    'id' : IDL.Nat32,
    'dek' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'status' : IDL.Int8,
    'updated_at' : IDL.Nat64,
    'custom' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, MetadataValue))),
    'hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Text,
    'size' : IDL.Nat64,
    'content_type' : IDL.Text,
    'created_at' : IDL.Nat64,
    'filled' : IDL.Nat64,
    'chunks' : IDL.Nat32,
    'parent' : IDL.Nat32,
  });
  const Result_8 = IDL.Variant({ 'Ok' : FileInfo, 'Err' : IDL.Text });
  const FolderInfo = IDL.Record({
    'id' : IDL.Nat32,
    'files' : IDL.Vec(IDL.Nat32),
    'status' : IDL.Int8,
    'updated_at' : IDL.Nat64,
    'name' : IDL.Text,
    'folders' : IDL.Vec(IDL.Nat32),
    'created_at' : IDL.Nat64,
    'parent' : IDL.Nat32,
  });
  const Result_9 = IDL.Variant({ 'Ok' : FolderInfo, 'Err' : IDL.Text });
  const Result_10 = IDL.Variant({ 'Ok' : IDL.Vec(FileInfo), 'Err' : IDL.Text });
  const Result_11 = IDL.Variant({
    'Ok' : IDL.Vec(FolderInfo),
    'Err' : IDL.Text,
  });
  const MoveInput = IDL.Record({
    'id' : IDL.Nat32,
    'to' : IDL.Nat32,
    'from' : IDL.Nat32,
  });
  const UpdateFileOutput = IDL.Record({ 'updated_at' : IDL.Nat64 });
  const Result_12 = IDL.Variant({ 'Ok' : UpdateFileOutput, 'Err' : IDL.Text });
  const UpdateFileChunkInput = IDL.Record({
    'id' : IDL.Nat32,
    'chunk_index' : IDL.Nat32,
    'content' : IDL.Vec(IDL.Nat8),
  });
  const UpdateFileChunkOutput = IDL.Record({
    'updated_at' : IDL.Nat64,
    'filled' : IDL.Nat64,
  });
  const Result_13 = IDL.Variant({
    'Ok' : UpdateFileChunkOutput,
    'Err' : IDL.Text,
  });
  const UpdateFileInput = IDL.Record({
    'id' : IDL.Nat32,
    'status' : IDL.Opt(IDL.Int8),
    'custom' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, MetadataValue))),
    'hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Opt(IDL.Text),
    'size' : IDL.Opt(IDL.Nat64),
    'content_type' : IDL.Opt(IDL.Text),
  });
  const UpdateFolderInput = IDL.Record({
    'id' : IDL.Nat32,
    'status' : IDL.Opt(IDL.Int8),
    'name' : IDL.Opt(IDL.Text),
  });
  const Result_14 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  return IDL.Service({
    'admin_add_auditors' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_add_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_remove_auditors' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_remove_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_set_auditors' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_set_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_update_bucket' : IDL.Func([UpdateBucketInput], [Result], []),
    'api_version' : IDL.Func([], [IDL.Nat16], ['query']),
    'batch_delete_subfiles' : IDL.Func(
        [IDL.Nat32, IDL.Vec(IDL.Nat32), IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_1],
        [],
      ),
    'create_file' : IDL.Func(
        [CreateFileInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_2],
        [],
      ),
    'create_folder' : IDL.Func(
        [CreateFolderInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_2],
        [],
      ),
    'delete_file' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_3],
        [],
      ),
    'delete_folder' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_3],
        [],
      ),
    'get_bucket_info' : IDL.Func(
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_4],
        ['query'],
      ),
    'get_canister_status' : IDL.Func([], [Result_5], []),
    'get_file_ancestors' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_6],
        ['query'],
      ),
    'get_file_chunks' : IDL.Func(
        [IDL.Nat32, IDL.Nat32, IDL.Opt(IDL.Nat32), IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_7],
        ['query'],
      ),
    'get_file_info' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_8],
        ['query'],
      ),
    'get_file_info_by_hash' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_8],
        ['query'],
      ),
    'get_folder_ancestors' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_6],
        ['query'],
      ),
    'get_folder_info' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_9],
        ['query'],
      ),
    'list_files' : IDL.Func(
        [
          IDL.Nat32,
          IDL.Opt(IDL.Nat32),
          IDL.Opt(IDL.Nat32),
          IDL.Opt(IDL.Vec(IDL.Nat8)),
        ],
        [Result_10],
        ['query'],
      ),
    'list_folders' : IDL.Func(
        [
          IDL.Nat32,
          IDL.Opt(IDL.Nat32),
          IDL.Opt(IDL.Nat32),
          IDL.Opt(IDL.Vec(IDL.Nat8)),
        ],
        [Result_11],
        ['query'],
      ),
    'move_file' : IDL.Func(
        [MoveInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_12],
        [],
      ),
    'move_folder' : IDL.Func(
        [MoveInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_12],
        [],
      ),
    'update_file_chunk' : IDL.Func(
        [UpdateFileChunkInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_13],
        [],
      ),
    'update_file_info' : IDL.Func(
        [UpdateFileInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_12],
        [],
      ),
    'update_folder_info' : IDL.Func(
        [UpdateFolderInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_12],
        [],
      ),
    'validate2_admin_set_auditors' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_14],
        [],
      ),
    'validate2_admin_set_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_14],
        [],
      ),
    'validate2_admin_update_bucket' : IDL.Func(
        [UpdateBucketInput],
        [Result_14],
        [],
      ),
    'validate_admin_add_auditors' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_14],
        [],
      ),
    'validate_admin_add_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_14],
        [],
      ),
    'validate_admin_remove_auditors' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_14],
        [],
      ),
    'validate_admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_14],
        [],
      ),
    'validate_admin_set_auditors' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'validate_admin_set_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'validate_admin_update_bucket' : IDL.Func(
        [UpdateBucketInput],
        [Result],
        [],
      ),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'governance_canister' : IDL.Opt(IDL.Principal),
    'max_custom_data_size' : IDL.Opt(IDL.Nat16),
    'max_children' : IDL.Opt(IDL.Nat16),
    'enable_hash_index' : IDL.Opt(IDL.Bool),
    'max_file_size' : IDL.Opt(IDL.Nat64),
    'max_folder_depth' : IDL.Opt(IDL.Nat8),
  });
  const InitArgs = IDL.Record({
    'governance_canister' : IDL.Opt(IDL.Principal),
    'name' : IDL.Text,
    'max_custom_data_size' : IDL.Nat16,
    'max_children' : IDL.Nat16,
    'enable_hash_index' : IDL.Bool,
    'max_file_size' : IDL.Nat64,
    'visibility' : IDL.Nat8,
    'max_folder_depth' : IDL.Nat8,
    'file_id' : IDL.Nat32,
  });
  const CanisterArgs = IDL.Variant({
    'Upgrade' : UpgradeArgs,
    'Init' : InitArgs,
  });
  return [IDL.Opt(CanisterArgs)];
};
