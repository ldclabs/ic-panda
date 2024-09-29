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
  'canister' : Principal,
  'image' : string,
  'message_start' : number,
  'latest_message_at' : bigint,
  'latest_message_by' : Principal,
  'latest_message_id' : number,
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
export interface InitArgs { 'managers' : Array<Principal>, 'name' : string }
export type LogVisibility = { 'controllers' : null } |
  { 'public' : null };
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
export type Result_10 = { 'Ok' : ChannelSetting } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : ChannelInfo } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : Array<ChannelBasicInfo> } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : CanisterStatusResponse } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : [] | [ChannelInfo] } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : Message } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : StateInfo } |
  { 'Err' : string };
export type Result_8 = { 'Ok' : Array<Message> } |
  { 'Err' : string };
export type Result_9 = { 'Ok' : [bigint, [] | [Message]] } |
  { 'Err' : string };
export interface StateInfo {
  'channel_id' : number,
  'incoming_gas' : bigint,
  'managers' : Array<Principal>,
  'name' : string,
  'burned_gas' : bigint,
  'channels_total' : bigint,
  'messages_total' : bigint,
}
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
export interface _SERVICE {
  'add_message' : ActorMethod<[AddMessageInput], Result>,
  'admin_add_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'admin_create_channel' : ActorMethod<[CreateChannelInput], Result_2>,
  'admin_remove_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'batch_get_channels' : ActorMethod<[Uint32Array | number[]], Result_3>,
  'get_canister_status' : ActorMethod<[], Result_4>,
  'get_channel_if_update' : ActorMethod<[number, bigint], Result_5>,
  'get_message' : ActorMethod<[number, number], Result_6>,
  'get_state' : ActorMethod<[], Result_7>,
  'leave_channel' : ActorMethod<[UpdateMySettingInput, boolean], Result_1>,
  'list_messages' : ActorMethod<
    [number, [] | [number], [] | [number]],
    Result_8
  >,
  'my_channels_if_update' : ActorMethod<[[] | [bigint]], Result_3>,
  'remove_member' : ActorMethod<[UpdateChannelMemberInput], Result_1>,
  'update_channel' : ActorMethod<[UpdateChannelInput], Result_6>,
  'update_manager' : ActorMethod<[UpdateChannelMemberInput], Result_9>,
  'update_member' : ActorMethod<[UpdateChannelMemberInput], Result_9>,
  'update_my_setting' : ActorMethod<[UpdateMySettingInput], Result_10>,
  'validate_admin_add_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'validate_admin_remove_managers' : ActorMethod<[Array<Principal>], Result_1>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
