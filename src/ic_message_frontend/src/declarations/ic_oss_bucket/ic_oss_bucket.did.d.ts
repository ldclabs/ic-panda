import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface BucketInfo {
  'status' : number,
  'total_chunks' : bigint,
  'trusted_eddsa_pub_keys' : Array<Uint8Array | number[]>,
  'managers' : Array<Principal>,
  'governance_canister' : [] | [Principal],
  'name' : string,
  'max_custom_data_size' : number,
  'auditors' : Array<Principal>,
  'total_files' : bigint,
  'max_children' : number,
  'enable_hash_index' : boolean,
  'max_file_size' : bigint,
  'folder_id' : number,
  'visibility' : number,
  'max_folder_depth' : number,
  'trusted_ecdsa_pub_keys' : Array<Uint8Array | number[]>,
  'total_folders' : bigint,
  'file_id' : number,
}
export type CanisterArgs = { 'Upgrade' : UpgradeArgs } |
  { 'Init' : InitArgs };
export interface CanisterStatusResponse {
  'status' : CanisterStatusType,
  'memory_size' : bigint,
  'cycles' : bigint,
  'settings' : DefiniteCanisterSettings,
  'query_stats' : QueryStats,
  'idle_cycles_burned_per_day' : bigint,
  'module_hash' : [] | [Uint8Array | number[]],
  'reserved_cycles' : bigint,
}
export type CanisterStatusType = { 'stopped' : null } |
  { 'stopping' : null } |
  { 'running' : null };
export interface CreateFileInput {
  'dek' : [] | [Uint8Array | number[]],
  'status' : [] | [number],
  'content' : [] | [Uint8Array | number[]],
  'custom' : [] | [Array<[string, MetadataValue]>],
  'hash' : [] | [Uint8Array | number[]],
  'name' : string,
  'size' : [] | [bigint],
  'content_type' : string,
  'parent' : number,
}
export interface CreateFileOutput { 'id' : number, 'created_at' : bigint }
export interface CreateFolderInput { 'name' : string, 'parent' : number }
export interface DefiniteCanisterSettings {
  'freezing_threshold' : bigint,
  'controllers' : Array<Principal>,
  'reserved_cycles_limit' : bigint,
  'log_visibility' : LogVisibility,
  'wasm_memory_limit' : bigint,
  'memory_allocation' : bigint,
  'compute_allocation' : bigint,
}
export interface FileInfo {
  'ex' : [] | [Array<[string, MetadataValue]>],
  'id' : number,
  'dek' : [] | [Uint8Array | number[]],
  'status' : number,
  'updated_at' : bigint,
  'custom' : [] | [Array<[string, MetadataValue]>],
  'hash' : [] | [Uint8Array | number[]],
  'name' : string,
  'size' : bigint,
  'content_type' : string,
  'created_at' : bigint,
  'filled' : bigint,
  'chunks' : number,
  'parent' : number,
}
export interface FolderInfo {
  'id' : number,
  'files' : Uint32Array | number[],
  'status' : number,
  'updated_at' : bigint,
  'name' : string,
  'folders' : Uint32Array | number[],
  'created_at' : bigint,
  'parent' : number,
}
export interface FolderName { 'id' : number, 'name' : string }
export interface InitArgs {
  'governance_canister' : [] | [Principal],
  'name' : string,
  'max_custom_data_size' : number,
  'max_children' : number,
  'enable_hash_index' : boolean,
  'max_file_size' : bigint,
  'visibility' : number,
  'max_folder_depth' : number,
  'file_id' : number,
}
export type LogVisibility = { 'controllers' : null } |
  { 'public' : null } |
  { 'allowed_viewers' : Array<Principal> };
export type MetadataValue = { 'Int' : bigint } |
  { 'Nat' : bigint } |
  { 'Blob' : Uint8Array | number[] } |
  { 'Text' : string };
export interface MoveInput { 'id' : number, 'to' : number, 'from' : number }
export interface QueryStats {
  'response_payload_bytes_total' : bigint,
  'num_instructions_total' : bigint,
  'num_calls_total' : bigint,
  'request_payload_bytes_total' : bigint,
}
export type Result = { 'Ok' : null } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : Uint32Array | number[] } |
  { 'Err' : string };
export type Result_10 = { 'Ok' : Array<FileInfo> } |
  { 'Err' : string };
export type Result_11 = { 'Ok' : Array<FolderInfo> } |
  { 'Err' : string };
export type Result_12 = { 'Ok' : UpdateFileOutput } |
  { 'Err' : string };
export type Result_13 = { 'Ok' : UpdateFileChunkOutput } |
  { 'Err' : string };
export type Result_14 = { 'Ok' : string } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : CreateFileOutput } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : boolean } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : BucketInfo } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : CanisterStatusResponse } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : Array<FolderName> } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : Array<[number, Uint8Array | number[]]> } |
  { 'Err' : string };
export type Result_8 = { 'Ok' : FileInfo } |
  { 'Err' : string };
export type Result_9 = { 'Ok' : FolderInfo } |
  { 'Err' : string };
export interface UpdateBucketInput {
  'status' : [] | [number],
  'trusted_eddsa_pub_keys' : [] | [Array<Uint8Array | number[]>],
  'name' : [] | [string],
  'max_custom_data_size' : [] | [number],
  'max_children' : [] | [number],
  'enable_hash_index' : [] | [boolean],
  'max_file_size' : [] | [bigint],
  'visibility' : [] | [number],
  'max_folder_depth' : [] | [number],
  'trusted_ecdsa_pub_keys' : [] | [Array<Uint8Array | number[]>],
}
export interface UpdateFileChunkInput {
  'id' : number,
  'chunk_index' : number,
  'content' : Uint8Array | number[],
}
export interface UpdateFileChunkOutput {
  'updated_at' : bigint,
  'filled' : bigint,
}
export interface UpdateFileInput {
  'id' : number,
  'status' : [] | [number],
  'custom' : [] | [Array<[string, MetadataValue]>],
  'hash' : [] | [Uint8Array | number[]],
  'name' : [] | [string],
  'size' : [] | [bigint],
  'content_type' : [] | [string],
}
export interface UpdateFileOutput { 'updated_at' : bigint }
export interface UpdateFolderInput {
  'id' : number,
  'status' : [] | [number],
  'name' : [] | [string],
}
export interface UpgradeArgs {
  'governance_canister' : [] | [Principal],
  'max_custom_data_size' : [] | [number],
  'max_children' : [] | [number],
  'enable_hash_index' : [] | [boolean],
  'max_file_size' : [] | [bigint],
  'max_folder_depth' : [] | [number],
}
export interface _SERVICE {
  'admin_add_auditors' : ActorMethod<[Array<Principal>], Result>,
  'admin_add_managers' : ActorMethod<[Array<Principal>], Result>,
  'admin_remove_auditors' : ActorMethod<[Array<Principal>], Result>,
  'admin_remove_managers' : ActorMethod<[Array<Principal>], Result>,
  'admin_set_auditors' : ActorMethod<[Array<Principal>], Result>,
  'admin_set_managers' : ActorMethod<[Array<Principal>], Result>,
  'admin_update_bucket' : ActorMethod<[UpdateBucketInput], Result>,
  'api_version' : ActorMethod<[], number>,
  'batch_delete_subfiles' : ActorMethod<
    [number, Uint32Array | number[], [] | [Uint8Array | number[]]],
    Result_1
  >,
  'create_file' : ActorMethod<
    [CreateFileInput, [] | [Uint8Array | number[]]],
    Result_2
  >,
  'create_folder' : ActorMethod<
    [CreateFolderInput, [] | [Uint8Array | number[]]],
    Result_2
  >,
  'delete_file' : ActorMethod<[number, [] | [Uint8Array | number[]]], Result_3>,
  'delete_folder' : ActorMethod<
    [number, [] | [Uint8Array | number[]]],
    Result_3
  >,
  'get_bucket_info' : ActorMethod<[[] | [Uint8Array | number[]]], Result_4>,
  'get_canister_status' : ActorMethod<[], Result_5>,
  'get_file_ancestors' : ActorMethod<
    [number, [] | [Uint8Array | number[]]],
    Result_6
  >,
  'get_file_chunks' : ActorMethod<
    [number, number, [] | [number], [] | [Uint8Array | number[]]],
    Result_7
  >,
  'get_file_info' : ActorMethod<
    [number, [] | [Uint8Array | number[]]],
    Result_8
  >,
  'get_file_info_by_hash' : ActorMethod<
    [Uint8Array | number[], [] | [Uint8Array | number[]]],
    Result_8
  >,
  'get_folder_ancestors' : ActorMethod<
    [number, [] | [Uint8Array | number[]]],
    Result_6
  >,
  'get_folder_info' : ActorMethod<
    [number, [] | [Uint8Array | number[]]],
    Result_9
  >,
  'list_files' : ActorMethod<
    [number, [] | [number], [] | [number], [] | [Uint8Array | number[]]],
    Result_10
  >,
  'list_folders' : ActorMethod<
    [number, [] | [number], [] | [number], [] | [Uint8Array | number[]]],
    Result_11
  >,
  'move_file' : ActorMethod<
    [MoveInput, [] | [Uint8Array | number[]]],
    Result_12
  >,
  'move_folder' : ActorMethod<
    [MoveInput, [] | [Uint8Array | number[]]],
    Result_12
  >,
  'update_file_chunk' : ActorMethod<
    [UpdateFileChunkInput, [] | [Uint8Array | number[]]],
    Result_13
  >,
  'update_file_info' : ActorMethod<
    [UpdateFileInput, [] | [Uint8Array | number[]]],
    Result_12
  >,
  'update_folder_info' : ActorMethod<
    [UpdateFolderInput, [] | [Uint8Array | number[]]],
    Result_12
  >,
  'validate2_admin_set_auditors' : ActorMethod<[Array<Principal>], Result_14>,
  'validate2_admin_set_managers' : ActorMethod<[Array<Principal>], Result_14>,
  'validate2_admin_update_bucket' : ActorMethod<[UpdateBucketInput], Result_14>,
  'validate_admin_add_auditors' : ActorMethod<[Array<Principal>], Result_14>,
  'validate_admin_add_managers' : ActorMethod<[Array<Principal>], Result_14>,
  'validate_admin_remove_auditors' : ActorMethod<[Array<Principal>], Result_14>,
  'validate_admin_remove_managers' : ActorMethod<[Array<Principal>], Result_14>,
  'validate_admin_set_auditors' : ActorMethod<[Array<Principal>], Result>,
  'validate_admin_set_managers' : ActorMethod<[Array<Principal>], Result>,
  'validate_admin_update_bucket' : ActorMethod<[UpdateBucketInput], Result>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
