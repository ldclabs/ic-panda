import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export type CanisterKind = { 'OssBucket' : null } |
  { 'OssCluster' : null };
export interface CanisterStatusResult {
  'memory_metrics' : MemoryMetrics,
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
export type ChainArgs = { 'Upgrade' : UpgradeArgs } |
  { 'Init' : InitArgs };
export interface ChannelSetting {
  'pin' : number,
  'alias' : string,
  'tags' : Array<string>,
}
export interface DefiniteCanisterSettings {
  'freezing_threshold' : bigint,
  'wasm_memory_threshold' : bigint,
  'controllers' : Array<Principal>,
  'reserved_cycles_limit' : bigint,
  'log_visibility' : LogVisibility,
  'wasm_memory_limit' : bigint,
  'memory_allocation' : bigint,
  'compute_allocation' : bigint,
}
export interface InitArgs { 'managers' : Array<Principal>, 'name' : string }
export interface Link {
  'uri' : string,
  'title' : string,
  'image' : [] | [string],
}
export type LogVisibility = { 'controllers' : null } |
  { 'public' : null } |
  { 'allowed_viewers' : Array<Principal> };
export interface MemoryMetrics {
  'wasm_binary_size' : bigint,
  'wasm_chunk_store_size' : bigint,
  'canister_history_size' : bigint,
  'stable_memory_size' : bigint,
  'snapshots_size' : bigint,
  'wasm_memory_size' : bigint,
  'global_memory_size' : bigint,
  'custom_sections_size' : bigint,
}
export interface ProfileInfo {
  'id' : Principal,
  'bio' : string,
  'active_at' : bigint,
  'created_at' : bigint,
  'channels' : [] | [Array<[[Principal, bigint], ChannelSetting]>],
  'image_file' : [] | [[Principal, number]],
  'links' : Array<Link>,
  'tokens' : Array<Principal>,
  'canister' : Principal,
  'ecdh_pub' : [] | [Uint8Array | number[]],
  'following' : [] | [Array<Principal>],
}
export interface QueryStats {
  'response_payload_bytes_total' : bigint,
  'num_instructions_total' : bigint,
  'num_calls_total' : bigint,
  'request_payload_bytes_total' : bigint,
}
export type Result = { 'Ok' : null } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : CanisterStatusResult } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : ProfileInfo } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : StateInfo } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : UploadImageOutput } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : string } |
  { 'Err' : string };
export interface StateInfo {
  'managers' : Array<Principal>,
  'profiles_total' : bigint,
  'name' : string,
  'ic_oss_cluster' : [] | [Principal],
  'ic_oss_buckets' : Array<Principal>,
}
export interface UpdateProfileInput {
  'bio' : [] | [string],
  'remove_channels' : Array<[Principal, bigint]>,
  'upsert_channels' : Array<[[Principal, bigint], ChannelSetting]>,
  'follow' : Array<Principal>,
  'unfollow' : Array<Principal>,
}
export interface UpgradeArgs {
  'managers' : [] | [Array<Principal>],
  'name' : [] | [string],
}
export interface UploadImageInput { 'size' : bigint, 'content_type' : string }
export interface UploadImageOutput {
  'name' : string,
  'image' : [Principal, number],
  'access_token' : Uint8Array | number[],
}
export interface _SERVICE {
  'admin_add_canister' : ActorMethod<[CanisterKind, Principal], Result>,
  'admin_add_managers' : ActorMethod<[Array<Principal>], Result>,
  'admin_remove_managers' : ActorMethod<[Array<Principal>], Result>,
  'admin_update_profile_ecdh_pub' : ActorMethod<
    [Principal, Uint8Array | number[]],
    Result
  >,
  'admin_upsert_profile' : ActorMethod<
    [Principal, [] | [[Principal, bigint]]],
    Result
  >,
  'get_canister_status' : ActorMethod<[], Result_1>,
  'get_profile' : ActorMethod<[[] | [Principal]], Result_2>,
  'get_state' : ActorMethod<[], Result_3>,
  'update_links' : ActorMethod<[Array<Link>], Result>,
  'update_profile' : ActorMethod<[UpdateProfileInput], Result_2>,
  'update_profile_ecdh_pub' : ActorMethod<[Uint8Array | number[]], Result>,
  'update_tokens' : ActorMethod<[Array<Principal>], Result>,
  'upload_image_token' : ActorMethod<[UploadImageInput], Result_4>,
  'validate2_admin_add_managers' : ActorMethod<[Array<Principal>], Result_5>,
  'validate2_admin_remove_managers' : ActorMethod<[Array<Principal>], Result_5>,
  'validate_admin_add_canister' : ActorMethod<
    [CanisterKind, Principal],
    Result_5
  >,
  'validate_admin_add_managers' : ActorMethod<[Array<Principal>], Result>,
  'validate_admin_remove_managers' : ActorMethod<[Array<Principal>], Result>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
