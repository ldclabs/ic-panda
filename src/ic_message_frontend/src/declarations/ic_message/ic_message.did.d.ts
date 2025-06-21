import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface Account {
  'owner' : Principal,
  'subaccount' : [] | [Uint8Array | number[]],
}
export interface ArchivedBlocks {
  'args' : Array<GetBlocksRequest>,
  'callback' : [Principal, string],
}
export interface BlockWithId { 'id' : bigint, 'block' : ICRC3Value }
export type CanisterKind = { 'Cose' : null } |
  { 'Channel' : null } |
  { 'Profile' : null };
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
export interface ChannelECDHInput {
  'ecdh_remote' : [] | [[Uint8Array | number[], Uint8Array | number[]]],
  'ecdh_pub' : [] | [Uint8Array | number[]],
}
export interface ChannelFilesState {
  'files_size_total' : bigint,
  'file_max_size' : bigint,
  'files_total' : bigint,
  'file_storage' : [Principal, number],
}
export interface ChannelInfo {
  'id' : number,
  'dek' : Uint8Array | number[],
  'gas' : bigint,
  'updated_at' : bigint,
  'ecdh_request' : Array<
    [
      Principal,
      [
        Uint8Array | number[],
        [] | [[Uint8Array | number[], Uint8Array | number[]]],
      ],
    ]
  >,
  'members' : Array<Principal>,
  'managers' : Array<Principal>,
  'name' : string,
  'paid' : bigint,
  'description' : string,
  'created_at' : bigint,
  'created_by' : Principal,
  'deleted_messages' : Uint32Array | number[],
  'canister' : Principal,
  'image' : string,
  'message_start' : number,
  'latest_message_at' : bigint,
  'latest_message_by' : Principal,
  'latest_message_id' : number,
  'files_state' : [] | [ChannelFilesState],
  'my_setting' : ChannelSetting,
}
export interface ChannelKEKInput {
  'id' : number,
  'kek' : Uint8Array | number[],
  'canister' : Principal,
}
export interface ChannelSetting {
  'updated_at' : bigint,
  'mute' : boolean,
  'ecdh_remote' : [] | [[Uint8Array | number[], Uint8Array | number[]]],
  'unread' : number,
  'last_read' : number,
  'ecdh_pub' : [] | [Uint8Array | number[]],
}
export interface ChannelTopupInput {
  'id' : number,
  'canister' : Principal,
  'payer' : Principal,
  'amount' : bigint,
}
export interface CreateChannelInput {
  'dek' : Uint8Array | number[],
  'managers' : Array<[Principal, ChannelECDHInput]>,
  'name' : string,
  'paid' : bigint,
  'description' : string,
  'created_by' : Principal,
  'image' : string,
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
export interface GetArchivesArgs { 'from' : [] | [Principal] }
export interface GetBlocksRequest { 'start' : bigint, 'length' : bigint }
export interface GetBlocksResult {
  'log_length' : bigint,
  'blocks' : Array<BlockWithId>,
  'archived_blocks' : Array<ArchivedBlocks>,
}
export interface ICRC3ArchiveInfo {
  'end' : bigint,
  'canister_id' : Principal,
  'start' : bigint,
}
export interface ICRC3DataCertificate {
  'certificate' : Uint8Array | number[],
  'hash_tree' : Uint8Array | number[],
}
export type ICRC3Value = { 'Int' : bigint } |
  { 'Map' : Array<[string, ICRC3Value]> } |
  { 'Nat' : bigint } |
  { 'Blob' : Uint8Array | number[] } |
  { 'Text' : string } |
  { 'Array' : Array<ICRC3Value> };
export interface InitArgs {
  'managers' : Array<Principal>,
  'name' : string,
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
export interface Price {
  'name_l1' : bigint,
  'name_l2' : bigint,
  'name_l3' : bigint,
  'name_l5' : bigint,
  'name_l7' : bigint,
  'channel' : bigint,
}
export interface QueryStats {
  'response_payload_bytes_total' : bigint,
  'num_instructions_total' : bigint,
  'num_calls_total' : bigint,
  'request_payload_bytes_total' : bigint,
}
export type Result = { 'Ok' : null } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : Array<UserInfo> } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : ChannelInfo } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : UserInfo } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : CanisterStatusResult } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : StateInfo } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : Uint8Array | number[] } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : Array<string> } |
  { 'Err' : string };
export type Result_8 = { 'Ok' : string } |
  { 'Err' : string };
export interface StateInfo {
  'latest_usernames' : Array<string>,
  'managers' : Array<Principal>,
  'name' : string,
  'profile_canisters' : Array<Principal>,
  'names_total' : bigint,
  'transfer_out_total' : bigint,
  'next_block_height' : bigint,
  'matured_channel_canisters' : Array<Principal>,
  'users_total' : bigint,
  'price' : Price,
  'next_block_phash' : Uint8Array | number[],
  'cose_canisters' : Array<Principal>,
  'incoming_total' : bigint,
  'channel_canisters' : Array<Principal>,
}
export interface SupportedBlockType { 'url' : string, 'block_type' : string }
export interface UpdateKVInput {
  'upsert_kv' : Array<[string, Uint8Array | number[]]>,
  'remove_kv' : Array<string>,
}
export interface UpdatePriceInput {
  'name_l1' : [] | [bigint],
  'name_l2' : [] | [bigint],
  'name_l3' : [] | [bigint],
  'name_l5' : [] | [bigint],
  'name_l7' : [] | [bigint],
  'channel' : [] | [bigint],
}
export interface UpgradeArgs {
  'managers' : [] | [Array<Principal>],
  'name' : [] | [string],
  'schnorr_key_name' : [] | [string],
}
export interface UserInfo {
  'id' : Principal,
  'username' : [] | [string],
  'cose_canister' : [] | [Principal],
  'name' : string,
  'image' : string,
  'profile_canister' : Principal,
}
export interface _SERVICE {
  'admin_add_canister' : ActorMethod<[CanisterKind, Principal], Result>,
  'admin_add_managers' : ActorMethod<[Array<Principal>], Result>,
  'admin_collect_token' : ActorMethod<[Account, bigint], Result>,
  'admin_remove_managers' : ActorMethod<[Array<Principal>], Result>,
  'admin_update_price' : ActorMethod<[UpdatePriceInput], Result>,
  'batch_get_users' : ActorMethod<[Array<Principal>], Result_1>,
  'create_channel' : ActorMethod<[CreateChannelInput], Result_2>,
  'get_by_username' : ActorMethod<[string], Result_3>,
  'get_canister_status' : ActorMethod<[], Result_4>,
  'get_state' : ActorMethod<[], Result_5>,
  'get_user' : ActorMethod<[[] | [Principal]], Result_3>,
  'icrc3_get_archives' : ActorMethod<
    [GetArchivesArgs],
    Array<ICRC3ArchiveInfo>
  >,
  'icrc3_get_blocks' : ActorMethod<[Array<GetBlocksRequest>], GetBlocksResult>,
  'icrc3_get_tip_certificate' : ActorMethod<[], [] | [ICRC3DataCertificate]>,
  'icrc3_supported_block_types' : ActorMethod<[], Array<SupportedBlockType>>,
  'my_iv' : ActorMethod<[], Result_6>,
  'register_username' : ActorMethod<[string, [] | [string]], Result_3>,
  'save_channel_kek' : ActorMethod<[ChannelKEKInput], Result>,
  'search_username' : ActorMethod<[string], Result_7>,
  'topup_channel' : ActorMethod<[ChannelTopupInput], Result_2>,
  'transfer_username' : ActorMethod<[Principal], Result>,
  'update_my_ecdh' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result
  >,
  'update_my_image' : ActorMethod<[string], Result>,
  'update_my_kv' : ActorMethod<[UpdateKVInput], Result>,
  'update_my_name' : ActorMethod<[string], Result_3>,
  'update_my_username' : ActorMethod<[string], Result_3>,
  'validate2_admin_add_canister' : ActorMethod<
    [CanisterKind, Principal],
    Result_8
  >,
  'validate2_admin_add_managers' : ActorMethod<[Array<Principal>], Result_8>,
  'validate2_admin_collect_token' : ActorMethod<[Account, bigint], Result_8>,
  'validate2_admin_remove_managers' : ActorMethod<[Array<Principal>], Result_8>,
  'validate2_admin_update_price' : ActorMethod<[UpdatePriceInput], Result_8>,
  'validate_admin_add_canister' : ActorMethod<
    [CanisterKind, Principal],
    Result
  >,
  'validate_admin_add_managers' : ActorMethod<[Array<Principal>], Result>,
  'validate_admin_collect_token' : ActorMethod<[Account, bigint], Result>,
  'validate_admin_remove_managers' : ActorMethod<[Array<Principal>], Result>,
  'validate_admin_update_price' : ActorMethod<[UpdatePriceInput], Result>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
