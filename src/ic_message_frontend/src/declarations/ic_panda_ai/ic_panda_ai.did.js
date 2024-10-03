export const idlFactory = ({ IDL }) => {
  const Value = IDL.Rec();
  const LoadModelInput = IDL.Record({
    'tokenizer_id' : IDL.Nat32,
    'config_id' : IDL.Nat32,
    'model_id' : IDL.Nat32,
  });
  const LoadModelOutput = IDL.Record({
    'total_instructions' : IDL.Nat64,
    'load_mode_instructions' : IDL.Nat64,
    'load_file_instructions' : IDL.Nat64,
  });
  const Result = IDL.Variant({ 'Ok' : LoadModelOutput, 'Err' : IDL.Text });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const ChatInput = IDL.Record({
    'top_p' : IDL.Opt(IDL.Float32),
    'challenge' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'messages' : IDL.Opt(IDL.Vec(IDL.Text)),
    'temperature' : IDL.Opt(IDL.Float32),
    'seed' : IDL.Opt(IDL.Nat64),
    'max_tokens' : IDL.Opt(IDL.Nat32),
    'prompt' : IDL.Text,
  });
  const ChatOutput = IDL.Record({
    'instructions' : IDL.Nat64,
    'tokens' : IDL.Nat32,
    'message' : IDL.Text,
  });
  const Result_2 = IDL.Variant({ 'Ok' : ChatOutput, 'Err' : IDL.Text });
  Value.fill(
    IDL.Variant({
      'Int' : IDL.Int,
      'Map' : IDL.Vec(IDL.Tuple(IDL.Text, Value)),
      'Nat' : IDL.Nat,
      'Nat64' : IDL.Nat64,
      'Blob' : IDL.Vec(IDL.Nat8),
      'Text' : IDL.Text,
      'Array' : IDL.Vec(Value),
    })
  );
  const CreateFileInput = IDL.Record({
    'status' : IDL.Opt(IDL.Int8),
    'content' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'custom' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, Value))),
    'hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Text,
    'crc32' : IDL.Opt(IDL.Nat32),
    'size' : IDL.Opt(IDL.Nat64),
    'content_type' : IDL.Text,
    'parent' : IDL.Nat32,
  });
  const CreateFileOutput = IDL.Record({
    'id' : IDL.Nat32,
    'created_at' : IDL.Nat64,
  });
  const Result_3 = IDL.Variant({ 'Ok' : CreateFileOutput, 'Err' : IDL.Text });
  const Result_4 = IDL.Variant({ 'Ok' : IDL.Bool, 'Err' : IDL.Text });
  const FileInfo = IDL.Record({
    'ex' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, Value))),
    'id' : IDL.Nat32,
    'status' : IDL.Int8,
    'updated_at' : IDL.Nat64,
    'custom' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, Value))),
    'hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Text,
    'size' : IDL.Nat64,
    'content_type' : IDL.Text,
    'created_at' : IDL.Nat64,
    'filled' : IDL.Nat64,
    'chunks' : IDL.Nat32,
    'parent' : IDL.Nat32,
  });
  const Result_5 = IDL.Variant({ 'Ok' : IDL.Vec(FileInfo), 'Err' : IDL.Text });
  const StateInfo = IDL.Record({
    'total_chunks' : IDL.Nat64,
    'managers' : IDL.Vec(IDL.Principal),
    'total_files' : IDL.Nat64,
    'ai_config' : IDL.Nat32,
    'ai_model' : IDL.Nat32,
    'max_file_size' : IDL.Nat64,
    'visibility' : IDL.Nat8,
    'chat_count' : IDL.Nat64,
    'ai_tokenizer' : IDL.Nat32,
    'file_id' : IDL.Nat32,
  });
  const Result_6 = IDL.Variant({ 'Ok' : StateInfo, 'Err' : IDL.Null });
  const UpdateFileChunkInput = IDL.Record({
    'id' : IDL.Nat32,
    'chunk_index' : IDL.Nat32,
    'content' : IDL.Vec(IDL.Nat8),
    'crc32' : IDL.Opt(IDL.Nat32),
  });
  const UpdateFileChunkOutput = IDL.Record({
    'updated_at' : IDL.Nat64,
    'filled' : IDL.Nat64,
  });
  const Result_7 = IDL.Variant({
    'Ok' : UpdateFileChunkOutput,
    'Err' : IDL.Text,
  });
  const UpdateFileInput = IDL.Record({
    'id' : IDL.Nat32,
    'status' : IDL.Opt(IDL.Int8),
    'custom' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, Value))),
    'hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Opt(IDL.Text),
    'content_type' : IDL.Opt(IDL.Text),
  });
  const UpdateFileOutput = IDL.Record({ 'updated_at' : IDL.Nat64 });
  const Result_8 = IDL.Variant({ 'Ok' : UpdateFileOutput, 'Err' : IDL.Text });
  return IDL.Service({
    'admin_load_model' : IDL.Func([LoadModelInput], [Result], []),
    'admin_set_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result_1], []),
    'api_version' : IDL.Func([], [IDL.Nat16], ['query']),
    'chat' : IDL.Func([ChatInput], [Result_2], ['query']),
    'create_file' : IDL.Func(
        [CreateFileInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_3],
        [],
      ),
    'delete_file' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_4],
        [],
      ),
    'list_files' : IDL.Func(
        [
          IDL.Nat32,
          IDL.Opt(IDL.Nat32),
          IDL.Opt(IDL.Nat32),
          IDL.Opt(IDL.Vec(IDL.Nat8)),
        ],
        [Result_5],
        ['query'],
      ),
    'state' : IDL.Func([], [Result_6], ['query']),
    'update_chat' : IDL.Func([ChatInput], [Result_2], []),
    'update_file_chunk' : IDL.Func(
        [UpdateFileChunkInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_7],
        [],
      ),
    'update_file_info' : IDL.Func(
        [UpdateFileInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_8],
        [],
      ),
    'validate_admin_set_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result_1],
        [],
      ),
  });
};
export const init = ({ IDL }) => { return []; };
