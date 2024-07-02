import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface ChatInput {
  'top_p' : [] | [number],
  'challenge' : [] | [Uint8Array | number[]],
  'messages' : [] | [Array<string>],
  'temperature' : [] | [number],
  'seed' : [] | [bigint],
  'max_tokens' : [] | [number],
  'prompt' : string,
}
export interface ChatOutput {
  'instructions' : bigint,
  'tokens' : number,
  'message' : string,
}
export interface CreateFileInput {
  'ert' : [] | [string],
  'status' : [] | [number],
  'content' : [] | [Uint8Array | number[]],
  'hash' : [] | [Uint8Array | number[]],
  'name' : string,
  'crc32' : [] | [number],
  'size' : [] | [bigint],
  'content_type' : string,
  'parent' : number,
}
export interface CreateFileOutput { 'id' : number, 'created_at' : bigint }
export interface FileInfo {
  'id' : number,
  'ert' : [] | [string],
  'status' : number,
  'updated_at' : bigint,
  'hash' : [] | [Uint8Array | number[]],
  'name' : string,
  'size' : bigint,
  'content_type' : string,
  'created_at' : bigint,
  'filled' : bigint,
  'chunks' : number,
  'parent' : number,
}
export interface LoadModelInput {
  'tokenizer_id' : number,
  'config_id' : number,
  'model_id' : number,
}
export interface LoadModelOutput {
  'total_instructions' : bigint,
  'load_mode_instructions' : bigint,
  'load_file_instructions' : bigint,
}
export type Result = { 'Ok' : LoadModelOutput } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : null } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : ChatOutput } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : CreateFileOutput } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : FileInfo } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : State } |
  { 'Err' : null };
export type Result_6 = { 'Ok' : UpdateFileChunkOutput } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : UpdateFileOutput } |
  { 'Err' : string };
export interface State {
  'managers' : Array<Principal>,
  'ai_config' : number,
  'ai_model' : number,
  'chat_count' : bigint,
  'ai_tokenizer' : number,
  'file_id' : number,
}
export interface UpdateFileChunkInput {
  'id' : number,
  'chunk_index' : number,
  'content' : Uint8Array | number[],
  'crc32' : [] | [number],
}
export interface UpdateFileChunkOutput {
  'updated_at' : bigint,
  'filled' : bigint,
}
export interface UpdateFileInput {
  'id' : number,
  'ert' : [] | [string],
  'status' : [] | [number],
  'hash' : [] | [Uint8Array | number[]],
  'name' : [] | [string],
  'content_type' : [] | [string],
  'parent' : [] | [number],
}
export interface UpdateFileOutput { 'updated_at' : bigint }
export interface _SERVICE {
  'admin_load_model' : ActorMethod<[LoadModelInput], Result>,
  'admin_set_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'api_version' : ActorMethod<[], number>,
  'chat' : ActorMethod<[ChatInput], Result_2>,
  'create_file' : ActorMethod<
    [CreateFileInput, [] | [Uint8Array | number[]]],
    Result_3
  >,
  'delete_file' : ActorMethod<[number, [] | [Uint8Array | number[]]], Result_1>,
  'get_file_info' : ActorMethod<
    [number, [] | [Uint8Array | number[]]],
    Result_4
  >,
  'list_files' : ActorMethod<
    [number, [] | [number], [] | [number], [] | [Uint8Array | number[]]],
    Array<FileInfo>
  >,
  'state' : ActorMethod<[], Result_5>,
  'update_chat' : ActorMethod<[ChatInput], Result_2>,
  'update_file_chunk' : ActorMethod<
    [UpdateFileChunkInput, [] | [Uint8Array | number[]]],
    Result_6
  >,
  'update_file_info' : ActorMethod<
    [UpdateFileInput, [] | [Uint8Array | number[]]],
    Result_7
  >,
  'validate_admin_set_managers' : ActorMethod<[Array<Principal>], Result_1>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
