export const idlFactory = ({ IDL }) => {
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
  const CreateFileInput = IDL.Record({
    'ert' : IDL.Opt(IDL.Text),
    'status' : IDL.Opt(IDL.Int8),
    'content' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Text,
    'crc32' : IDL.Opt(IDL.Nat32),
    'size' : IDL.Opt(IDL.Nat),
    'content_type' : IDL.Text,
    'parent' : IDL.Nat32,
  });
  const CreateFileOutput = IDL.Record({
    'id' : IDL.Nat32,
    'created_at' : IDL.Nat,
  });
  const Result_3 = IDL.Variant({ 'Ok' : CreateFileOutput, 'Err' : IDL.Text });
  const FileInfo = IDL.Record({
    'id' : IDL.Nat32,
    'ert' : IDL.Opt(IDL.Text),
    'status' : IDL.Int8,
    'updated_at' : IDL.Nat,
    'hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Text,
    'size' : IDL.Nat,
    'content_type' : IDL.Text,
    'created_at' : IDL.Nat,
    'filled' : IDL.Nat,
    'chunks' : IDL.Nat32,
    'parent' : IDL.Nat32,
  });
  const Result_4 = IDL.Variant({ 'Ok' : FileInfo, 'Err' : IDL.Text });
  const State = IDL.Record({
    'managers' : IDL.Vec(IDL.Principal),
    'ai_config' : IDL.Nat32,
    'ai_model' : IDL.Nat32,
    'chat_count' : IDL.Nat64,
    'ai_tokenizer' : IDL.Nat32,
    'file_id' : IDL.Nat32,
  });
  const Result_5 = IDL.Variant({ 'Ok' : State, 'Err' : IDL.Null });
  const UpdateFileChunkInput = IDL.Record({
    'id' : IDL.Nat32,
    'chunk_index' : IDL.Nat32,
    'content' : IDL.Vec(IDL.Nat8),
    'crc32' : IDL.Opt(IDL.Nat32),
  });
  const UpdateFileChunkOutput = IDL.Record({
    'updated_at' : IDL.Nat,
    'filled' : IDL.Nat,
  });
  const Result_6 = IDL.Variant({
    'Ok' : UpdateFileChunkOutput,
    'Err' : IDL.Text,
  });
  const UpdateFileInput = IDL.Record({
    'id' : IDL.Nat32,
    'ert' : IDL.Opt(IDL.Text),
    'status' : IDL.Opt(IDL.Int8),
    'hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'name' : IDL.Opt(IDL.Text),
    'content_type' : IDL.Opt(IDL.Text),
    'parent' : IDL.Opt(IDL.Nat32),
  });
  const UpdateFileOutput = IDL.Record({ 'updated_at' : IDL.Nat });
  const Result_7 = IDL.Variant({ 'Ok' : UpdateFileOutput, 'Err' : IDL.Text });
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
        [Result_1],
        [],
      ),
    'get_file_info' : IDL.Func(
        [IDL.Nat32, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_4],
        ['query'],
      ),
    'list_files' : IDL.Func(
        [
          IDL.Nat32,
          IDL.Opt(IDL.Nat32),
          IDL.Opt(IDL.Nat32),
          IDL.Opt(IDL.Vec(IDL.Nat8)),
        ],
        [IDL.Vec(FileInfo)],
        ['query'],
      ),
    'state' : IDL.Func([], [Result_5], ['query']),
    'update_chat' : IDL.Func([ChatInput], [Result_2], []),
    'update_file_chunk' : IDL.Func(
        [UpdateFileChunkInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_6],
        [],
      ),
    'update_file_info' : IDL.Func(
        [UpdateFileInput, IDL.Opt(IDL.Vec(IDL.Nat8))],
        [Result_7],
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
