import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface AddWasmInput {
  'wasm' : Uint8Array | number[],
  'description' : string,
}
export interface BucketDeploymentInfo {
  'args' : [] | [Uint8Array | number[]],
  'prev_hash' : Uint8Array | number[],
  'error' : [] | [string],
  'deploy_at' : bigint,
  'canister' : Principal,
  'wasm_hash' : Uint8Array | number[],
}
export interface CanisterSettings {
  'freezing_threshold' : [] | [bigint],
  'wasm_memory_threshold' : [] | [bigint],
  'controllers' : [] | [Array<Principal>],
  'reserved_cycles_limit' : [] | [bigint],
  'log_visibility' : [] | [LogVisibility],
  'wasm_memory_limit' : [] | [bigint],
  'memory_allocation' : [] | [bigint],
  'compute_allocation' : [] | [bigint],
}
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
export interface ClusterInfo {
  'ecdsa_token_public_key' : string,
  'schnorr_ed25519_token_public_key' : string,
  'bucket_wasm_total' : bigint,
  'ecdsa_key_name' : string,
  'managers' : Array<Principal>,
  'governance_canister' : [] | [Principal],
  'name' : string,
  'bucket_deployed_total' : bigint,
  'token_expiration' : bigint,
  'weak_ed25519_token_public_key' : string,
  'bucket_latest_version' : Uint8Array | number[],
  'schnorr_key_name' : string,
  'bucket_deployment_logs' : bigint,
  'subject_authz_total' : bigint,
  'committers' : Array<Principal>,
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
export interface DeployWasmInput {
  'args' : [] | [Uint8Array | number[]],
  'canister' : Principal,
}
export interface InitArgs {
  'ecdsa_key_name' : string,
  'governance_canister' : [] | [Principal],
  'name' : string,
  'token_expiration' : bigint,
  'bucket_topup_threshold' : bigint,
  'bucket_topup_amount' : bigint,
  'schnorr_key_name' : string,
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
export interface QueryStats {
  'response_payload_bytes_total' : bigint,
  'num_instructions_total' : bigint,
  'num_calls_total' : bigint,
  'request_payload_bytes_total' : bigint,
}
export type Result = { 'Ok' : Uint8Array | number[] } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : null } |
  { 'Err' : string };
export type Result_10 = { 'Ok' : Array<[Principal, string]> } |
  { 'Err' : string };
export type Result_11 = { 'Ok' : string } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : Array<Uint8Array | number[]> } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : Principal } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : bigint } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : Array<BucketDeploymentInfo> } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : WasmInfo } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : Array<Principal> } |
  { 'Err' : string };
export type Result_8 = { 'Ok' : CanisterStatusResult } |
  { 'Err' : string };
export type Result_9 = { 'Ok' : ClusterInfo } |
  { 'Err' : string };
export interface Token {
  'subject' : Principal,
  'audience' : Principal,
  'policies' : string,
}
export interface UpdateSettingsArgs {
  'canister_id' : Principal,
  'settings' : CanisterSettings,
}
export interface UpgradeArgs {
  'governance_canister' : [] | [Principal],
  'name' : [] | [string],
  'token_expiration' : [] | [bigint],
  'bucket_topup_threshold' : [] | [bigint],
  'bucket_topup_amount' : [] | [bigint],
}
export interface WasmInfo {
  'hash' : Uint8Array | number[],
  'wasm' : Uint8Array | number[],
  'description' : string,
  'created_at' : bigint,
  'created_by' : Principal,
}
export interface _SERVICE {
  'access_token' : ActorMethod<[Principal], Result>,
  'admin_add_committers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_add_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_add_wasm' : ActorMethod<
    [AddWasmInput, [] | [Uint8Array | number[]]],
    Result_1
  >,
  'admin_attach_policies' : ActorMethod<[Token], Result_1>,
  'admin_batch_call_buckets' : ActorMethod<
    [Array<Principal>, string, [] | [Uint8Array | number[]]],
    Result_2
  >,
  'admin_create_bucket' : ActorMethod<
    [[] | [CanisterSettings], [] | [Uint8Array | number[]]],
    Result_3
  >,
  'admin_create_bucket_on' : ActorMethod<
    [Principal, [] | [CanisterSettings], [] | [Uint8Array | number[]]],
    Result_3
  >,
  'admin_deploy_bucket' : ActorMethod<
    [DeployWasmInput, [] | [Uint8Array | number[]]],
    Result_1
  >,
  'admin_detach_policies' : ActorMethod<[Token], Result_1>,
  'admin_ed25519_access_token' : ActorMethod<[Token], Result>,
  'admin_remove_committers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_remove_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_set_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_sign_access_token' : ActorMethod<[Token], Result>,
  'admin_topup_all_buckets' : ActorMethod<[], Result_4>,
  'admin_update_bucket_canister_settings' : ActorMethod<
    [UpdateSettingsArgs],
    Result_1
  >,
  'admin_upgrade_all_buckets' : ActorMethod<
    [[] | [Uint8Array | number[]]],
    Result_1
  >,
  'admin_weak_access_token' : ActorMethod<[Token, bigint, bigint], Result>,
  'bucket_deployment_logs' : ActorMethod<
    [[] | [bigint], [] | [bigint]],
    Result_5
  >,
  'ed25519_access_token' : ActorMethod<[Principal], Result>,
  'get_bucket_wasm' : ActorMethod<[Uint8Array | number[]], Result_6>,
  'get_buckets' : ActorMethod<[], Result_7>,
  'get_canister_status' : ActorMethod<[[] | [Principal]], Result_8>,
  'get_cluster_info' : ActorMethod<[], Result_9>,
  'get_deployed_buckets' : ActorMethod<[], Result_5>,
  'get_subject_policies' : ActorMethod<[Principal], Result_10>,
  'get_subject_policies_for' : ActorMethod<[Principal, Principal], Result_11>,
  'validate2_admin_add_wasm' : ActorMethod<
    [AddWasmInput, [] | [Uint8Array | number[]]],
    Result_11
  >,
  'validate2_admin_batch_call_buckets' : ActorMethod<
    [Array<Principal>, string, [] | [Uint8Array | number[]]],
    Result_11
  >,
  'validate2_admin_deploy_bucket' : ActorMethod<
    [DeployWasmInput, [] | [Uint8Array | number[]]],
    Result_11
  >,
  'validate2_admin_set_managers' : ActorMethod<[Array<Principal>], Result_11>,
  'validate2_admin_upgrade_all_buckets' : ActorMethod<
    [[] | [Uint8Array | number[]]],
    Result_11
  >,
  'validate_admin_add_committers' : ActorMethod<[Array<Principal>], Result_11>,
  'validate_admin_add_managers' : ActorMethod<[Array<Principal>], Result_11>,
  'validate_admin_add_wasm' : ActorMethod<
    [AddWasmInput, [] | [Uint8Array | number[]]],
    Result_1
  >,
  'validate_admin_batch_call_buckets' : ActorMethod<
    [Array<Principal>, string, [] | [Uint8Array | number[]]],
    Result_2
  >,
  'validate_admin_create_bucket' : ActorMethod<
    [[] | [CanisterSettings], [] | [Uint8Array | number[]]],
    Result_11
  >,
  'validate_admin_create_bucket_on' : ActorMethod<
    [Principal, [] | [CanisterSettings], [] | [Uint8Array | number[]]],
    Result_11
  >,
  'validate_admin_deploy_bucket' : ActorMethod<
    [DeployWasmInput, [] | [Uint8Array | number[]]],
    Result_1
  >,
  'validate_admin_remove_committers' : ActorMethod<
    [Array<Principal>],
    Result_11
  >,
  'validate_admin_remove_managers' : ActorMethod<[Array<Principal>], Result_11>,
  'validate_admin_set_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'validate_admin_update_bucket_canister_settings' : ActorMethod<
    [UpdateSettingsArgs],
    Result_11
  >,
  'validate_admin_upgrade_all_buckets' : ActorMethod<
    [[] | [Uint8Array | number[]]],
    Result_1
  >,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
