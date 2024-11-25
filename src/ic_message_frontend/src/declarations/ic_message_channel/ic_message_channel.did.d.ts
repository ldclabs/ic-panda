import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface AddMessageInput {
  'reply_to' : [] | [number],
  'channel' : number,
  'payload' : Uint8Array | number[],
}
export interface AddMessageOutput {
  'id' : number,
  'kind' : number,
  'created_at' : bigint,
  'channel' : number,
}
export type CanisterKind = { 'OssBucket' : null } |
  { 'OssCluster' : null };
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
export type ChainArgs = { 'Upgrade' : UpgradeArgs } |
  { 'Init' : InitArgs };
export interface ChannelBasicInfo {
  'id' : number,
  'gas' : bigint,
  'updated_at' : bigint,
  'name' : string,
  'paid' : bigint,
  'canister' : Principal,
  'image' : string,
  'latest_message_at' : bigint,
  'latest_message_by' : Principal,
  'latest_message_id' : number,
  'my_setting' : ChannelSetting,
}
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
  'controllers' : Array<Principal>,
  'reserved_cycles_limit' : bigint,
  'log_visibility' : LogVisibility,
  'wasm_memory_limit' : bigint,
  'memory_allocation' : bigint,
  'compute_allocation' : bigint,
}
export interface DeleteMessageInput { 'id' : number, 'channel' : number }
export interface DownloadFilesToken {
  'storage' : [Principal, number],
  'access_token' : Uint8Array | number[],
}
export interface InitArgs { 'managers' : Array<Principal>, 'name' : string }
export type LogVisibility = { 'controllers' : null } |
  { 'public' : null } |
  { 'allowed_viewers' : Array<Principal> };
export interface Message {
  'id' : number,
  'reply_to' : number,
  'kind' : number,
  'created_at' : bigint,
  'created_by' : Principal,
  'payload' : Uint8Array | number[],
}
export interface QueryStats {
  'response_payload_bytes_total' : bigint,
  'num_instructions_total' : bigint,
  'num_calls_total' : bigint,
  'request_payload_bytes_total' : bigint,
}
export type Result = { 'Ok' : AddMessageOutput } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : null } |
  { 'Err' : string };
export type Result_10 = { 'Ok' : Uint32Array | number[] } |
  { 'Err' : string };
export type Result_11 = { 'Ok' : [bigint, [] | [Message]] } |
  { 'Err' : string };
export type Result_12 = { 'Ok' : ChannelSetting } |
  { 'Err' : string };
export type Result_13 = { 'Ok' : UploadFileOutput } |
  { 'Err' : string };
export type Result_14 = { 'Ok' : string } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : ChannelInfo } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : Array<ChannelBasicInfo> } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : DownloadFilesToken } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : CanisterStatusResponse } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : [] | [ChannelInfo] } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : Message } |
  { 'Err' : string };
export type Result_8 = { 'Ok' : StateInfo } |
  { 'Err' : string };
export type Result_9 = { 'Ok' : Array<Message> } |
  { 'Err' : string };
export interface StateInfo {
  'channel_id' : number,
  'incoming_gas' : bigint,
  'managers' : Array<Principal>,
  'name' : string,
  'ic_oss_cluster' : [] | [Principal],
  'ic_oss_buckets' : Array<Principal>,
  'burned_gas' : bigint,
  'channels_total' : bigint,
  'messages_total' : bigint,
}
export interface TruncateMessageInput { 'to' : number, 'channel' : number }
export interface UpdateChannelInput {
  'id' : number,
  'name' : [] | [string],
  'description' : [] | [string],
  'image' : [] | [string],
}
export interface UpdateChannelMemberInput {
  'id' : number,
  'member' : Principal,
  'ecdh' : ChannelECDHInput,
}
export interface UpdateChannelStorageInput {
  'id' : number,
  'file_max_size' : bigint,
}
export interface UpdateMySettingInput {
  'id' : number,
  'ecdh' : [] | [ChannelECDHInput],
  'mute' : [] | [boolean],
  'last_read' : [] | [number],
}
export interface UpgradeArgs {
  'managers' : [] | [Array<Principal>],
  'name' : [] | [string],
}
export interface UploadFileInput {
  'size' : bigint,
  'content_type' : string,
  'channel' : number,
}
export interface UploadFileOutput {
  'id' : number,
  'storage' : [Principal, number],
  'name' : string,
  'access_token' : Uint8Array | number[],
}
export interface _SERVICE {
  'add_message' : ActorMethod<[AddMessageInput], Result>,
  'admin_add_canister' : ActorMethod<[CanisterKind, Principal], Result_1>,
  'admin_add_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_create_channel' : ActorMethod<[CreateChannelInput], Result_2>,
  'admin_remove_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_topup_channel' : ActorMethod<[ChannelTopupInput], Result_2>,
  'batch_get_channels' : ActorMethod<[Uint32Array | number[]], Result_3>,
  'delete_message' : ActorMethod<[DeleteMessageInput], Result_1>,
  'download_files_token' : ActorMethod<[number], Result_4>,
  'get_canister_status' : ActorMethod<[], Result_5>,
  'get_channel_if_update' : ActorMethod<[number, bigint], Result_6>,
  'get_message' : ActorMethod<[number, number], Result_7>,
  'get_state' : ActorMethod<[], Result_8>,
  'leave_channel' : ActorMethod<[UpdateMySettingInput, boolean], Result_1>,
  'list_messages' : ActorMethod<
    [number, [] | [number], [] | [number]],
    Result_9
  >,
  'my_channel_ids' : ActorMethod<[], Result_10>,
  'my_channels_if_update' : ActorMethod<[[] | [bigint]], Result_3>,
  'remove_member' : ActorMethod<[UpdateChannelMemberInput], Result_1>,
  'truncate_messages' : ActorMethod<[TruncateMessageInput], Result_1>,
  'update_channel' : ActorMethod<[UpdateChannelInput], Result_7>,
  'update_manager' : ActorMethod<[UpdateChannelMemberInput], Result_11>,
  'update_member' : ActorMethod<[UpdateChannelMemberInput], Result_11>,
  'update_my_setting' : ActorMethod<[UpdateMySettingInput], Result_12>,
  'update_storage' : ActorMethod<[UpdateChannelStorageInput], Result_7>,
  'upload_file_token' : ActorMethod<[UploadFileInput], Result_13>,
  'upload_image_token' : ActorMethod<[UploadFileInput], Result_13>,
  'validate2_admin_add_managers' : ActorMethod<[Array<Principal>], Result_14>,
  'validate2_admin_remove_managers' : ActorMethod<
    [Array<Principal>],
    Result_14
  >,
  'validate_admin_add_canister' : ActorMethod<
    [CanisterKind, Principal],
    Result_14
  >,
  'validate_admin_add_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'validate_admin_remove_managers' : ActorMethod<[Array<Principal>], Result_1>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
