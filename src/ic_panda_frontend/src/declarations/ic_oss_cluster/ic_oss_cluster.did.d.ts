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
export type ChainArgs = { 'Upgrade' : UpgradeArgs } |
  { 'Init' : InitArgs };
export interface ClusterInfo {
  'ecdsa_token_public_key' : string,
  'schnorr_ed25519_token_public_key' : string,
  'bucket_wasm_total' : bigint,
  'ecdsa_key_name' : string,
  'managers' : Array<Principal>,
  'name' : string,
  'bucket_deployed_total' : bigint,
  'token_expiration' : bigint,
  'weak_ed25519_token_public_key' : string,
  'bucket_latest_version' : Uint8Array | number[],
  'schnorr_key_name' : string,
  'bucket_deployment_logs' : bigint,
  'subject_authz_total' : bigint,
}
export interface DeployWasmInput {
  'args' : [] | [Uint8Array | number[]],
  'canister' : Principal,
}
export interface InitArgs {
  'ecdsa_key_name' : string,
  'name' : string,
  'token_expiration' : bigint,
  'bucket_topup_threshold' : bigint,
  'bucket_topup_amount' : bigint,
  'schnorr_key_name' : string,
}
export type Result = { 'Ok' : Uint8Array | number[] } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : null } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : Array<Uint8Array | number[]> } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : bigint } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : Array<BucketDeploymentInfo> } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : WasmInfo } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : Array<Principal> } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : ClusterInfo } |
  { 'Err' : string };
export type Result_8 = { 'Ok' : Array<[Principal, string]> } |
  { 'Err' : string };
export type Result_9 = { 'Ok' : string } |
  { 'Err' : string };
export interface Token {
  'subject' : Principal,
  'audience' : Principal,
  'policies' : string,
}
export interface UpgradeArgs {
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
  'admin_deploy_bucket' : ActorMethod<
    [DeployWasmInput, [] | [Uint8Array | number[]]],
    Result_1
  >,
  'admin_detach_policies' : ActorMethod<[Token], Result_1>,
  'admin_ed25519_access_token' : ActorMethod<[Token], Result>,
  'admin_remove_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_set_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_sign_access_token' : ActorMethod<[Token], Result>,
  'admin_topup_all_buckets' : ActorMethod<[], Result_3>,
  'admin_upgrade_all_buckets' : ActorMethod<
    [[] | [Uint8Array | number[]]],
    Result_1
  >,
  'admin_weak_access_token' : ActorMethod<[Token, bigint, bigint], Result>,
  'bucket_deployment_logs' : ActorMethod<
    [[] | [bigint], [] | [bigint]],
    Result_4
  >,
  'ed25519_access_token' : ActorMethod<[Principal], Result>,
  'get_bucket_wasm' : ActorMethod<[Uint8Array | number[]], Result_5>,
  'get_buckets' : ActorMethod<[], Result_6>,
  'get_cluster_info' : ActorMethod<[], Result_7>,
  'get_deployed_buckets' : ActorMethod<[], Result_4>,
  'get_subject_policies' : ActorMethod<[Principal], Result_8>,
  'get_subject_policies_for' : ActorMethod<[Principal, Principal], Result_9>,
  'validate2_admin_add_wasm' : ActorMethod<
    [AddWasmInput, [] | [Uint8Array | number[]]],
    Result_9
  >,
  'validate2_admin_batch_call_buckets' : ActorMethod<
    [Array<Principal>, string, [] | [Uint8Array | number[]]],
    Result_9
  >,
  'validate2_admin_deploy_bucket' : ActorMethod<
    [DeployWasmInput, [] | [Uint8Array | number[]]],
    Result_9
  >,
  'validate2_admin_set_managers' : ActorMethod<[Array<Principal>], Result_9>,
  'validate2_admin_upgrade_all_buckets' : ActorMethod<
    [[] | [Uint8Array | number[]]],
    Result_9
  >,
  'validate_admin_add_managers' : ActorMethod<[Array<Principal>], Result_9>,
  'validate_admin_add_wasm' : ActorMethod<
    [AddWasmInput, [] | [Uint8Array | number[]]],
    Result_1
  >,
  'validate_admin_batch_call_buckets' : ActorMethod<
    [Array<Principal>, string, [] | [Uint8Array | number[]]],
    Result_2
  >,
  'validate_admin_deploy_bucket' : ActorMethod<
    [DeployWasmInput, [] | [Uint8Array | number[]]],
    Result_1
  >,
  'validate_admin_remove_managers' : ActorMethod<[Array<Principal>], Result_9>,
  'validate_admin_set_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'validate_admin_upgrade_all_buckets' : ActorMethod<
    [[] | [Uint8Array | number[]]],
    Result_1
  >,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
