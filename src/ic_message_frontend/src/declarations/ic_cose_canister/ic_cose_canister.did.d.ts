import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export type ChainArgs = { 'Upgrade' : UpgradeArgs } |
  { 'Init' : InitArgs };
export interface CreateNamespaceInput {
  'managers' : Array<Principal>,
  'desc' : [] | [string],
  'name' : string,
  'max_payload_size' : [] | [bigint],
  'auditors' : Array<Principal>,
  'users' : Array<Principal>,
  'visibility' : number,
}
export interface CreateSettingInput {
  'dek' : [] | [Uint8Array | number[]],
  'status' : [] | [number],
  'desc' : [] | [string],
  'tags' : [] | [Array<[string, string]>],
  'payload' : [] | [Uint8Array | number[]],
}
export interface CreateSettingOutput {
  'updated_at' : bigint,
  'created_at' : bigint,
  'version' : number,
}
export interface ECDHInput {
  'public_key' : Uint8Array | number[],
  'nonce' : Uint8Array | number[],
}
export interface ECDHOutput {
  'public_key' : Uint8Array | number[],
  'payload' : Uint8Array | number[],
}
export interface InitArgs {
  'freezing_threshold' : bigint,
  'ecdsa_key_name' : string,
  'name' : string,
  'schnorr_key_name' : string,
  'allowed_apis' : Array<string>,
  'subnet_size' : bigint,
  'vetkd_key_name' : string,
}
export interface NamespaceInfo {
  'status' : number,
  'updated_at' : bigint,
  'managers' : Array<Principal>,
  'payload_bytes_total' : bigint,
  'desc' : string,
  'name' : string,
  'max_payload_size' : bigint,
  'created_at' : bigint,
  'auditors' : Array<Principal>,
  'settings_total' : bigint,
  'user_settings_total' : bigint,
  'users' : Array<Principal>,
  'visibility' : number,
  'gas_balance' : bigint,
}
export interface PublicKeyInput {
  'ns' : string,
  'derivation_path' : Array<Uint8Array | number[]>,
}
export interface PublicKeyOutput {
  'public_key' : Uint8Array | number[],
  'chain_code' : Uint8Array | number[],
}
export type Result = { 'Ok' : null } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : NamespaceInfo } |
  { 'Err' : string };
export type Result_10 = { 'Ok' : StateInfo } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : Array<NamespaceInfo> } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : ECDHOutput } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : PublicKeyOutput } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : Uint8Array | number[] } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : bigint } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : CreateSettingOutput } |
  { 'Err' : string };
export type Result_8 = { 'Ok' : SettingInfo } |
  { 'Err' : string };
export type Result_9 = { 'Ok' : SettingArchivedPayload } |
  { 'Err' : string };
export type SchnorrAlgorithm = { 'ed25519' : null } |
  { 'bip340secp256k1' : null };
export interface SettingArchivedPayload {
  'dek' : [] | [Uint8Array | number[]],
  'version' : number,
  'deprecated' : boolean,
  'archived_at' : bigint,
  'payload' : [] | [Uint8Array | number[]],
}
export interface SettingInfo {
  'dek' : [] | [Uint8Array | number[]],
  'key' : Uint8Array | number[],
  'readers' : Array<Principal>,
  'status' : number,
  'updated_at' : bigint,
  'subject' : Principal,
  'desc' : string,
  'tags' : Array<[string, string]>,
  'created_at' : bigint,
  'version' : number,
  'payload' : [] | [Uint8Array | number[]],
}
export interface SettingPath {
  'ns' : string,
  'key' : Uint8Array | number[],
  'subject' : [] | [Principal],
  'version' : number,
  'user_owned' : boolean,
}
export interface SignIdentityInput { 'ns' : string, 'audience' : string }
export interface SignInput {
  'ns' : string,
  'derivation_path' : Array<Uint8Array | number[]>,
  'message' : Uint8Array | number[],
}
export interface StateInfo {
  'freezing_threshold' : bigint,
  'ecdsa_key_name' : string,
  'managers' : Array<Principal>,
  'name' : string,
  'auditors' : Array<Principal>,
  'schnorr_key_name' : string,
  'allowed_apis' : Array<string>,
  'subnet_size' : bigint,
  'namespace_total' : bigint,
  'vetkd_key_name' : string,
}
export interface UpdateNamespaceInput {
  'status' : [] | [number],
  'desc' : [] | [string],
  'name' : string,
  'max_payload_size' : [] | [bigint],
  'visibility' : [] | [number],
}
export interface UpdateSettingInfoInput {
  'status' : [] | [number],
  'desc' : [] | [string],
  'tags' : [] | [Array<[string, string]>],
}
export interface UpdateSettingPayloadInput {
  'dek' : [] | [Uint8Array | number[]],
  'status' : [] | [number],
  'deprecate_current' : [] | [boolean],
  'payload' : [] | [Uint8Array | number[]],
}
export interface UpgradeArgs {
  'freezing_threshold' : [] | [bigint],
  'name' : [] | [string],
  'subnet_size' : [] | [bigint],
}
export interface _SERVICE {
  'admin_add_allowed_apis' : ActorMethod<[Array<string>], Result>,
  'admin_add_auditors' : ActorMethod<[Array<Principal>], Result>,
  'admin_add_managers' : ActorMethod<[Array<Principal>], Result>,
  'admin_create_namespace' : ActorMethod<[CreateNamespaceInput], Result_1>,
  'admin_list_namespace' : ActorMethod<
    [[] | [string], [] | [number]],
    Result_2
  >,
  'admin_remove_allowed_apis' : ActorMethod<[Array<string>], Result>,
  'admin_remove_auditors' : ActorMethod<[Array<Principal>], Result>,
  'admin_remove_managers' : ActorMethod<[Array<Principal>], Result>,
  'ecdh_cose_encrypted_key' : ActorMethod<[SettingPath, ECDHInput], Result_3>,
  'ecdsa_public_key' : ActorMethod<[[] | [PublicKeyInput]], Result_4>,
  'ecdsa_sign' : ActorMethod<[SignInput], Result_5>,
  'namespace_add_auditors' : ActorMethod<[string, Array<Principal>], Result>,
  'namespace_add_managers' : ActorMethod<[string, Array<Principal>], Result>,
  'namespace_add_users' : ActorMethod<[string, Array<Principal>], Result>,
  'namespace_get_info' : ActorMethod<[string], Result_1>,
  'namespace_remove_auditors' : ActorMethod<[string, Array<Principal>], Result>,
  'namespace_remove_managers' : ActorMethod<[string, Array<Principal>], Result>,
  'namespace_remove_users' : ActorMethod<[string, Array<Principal>], Result>,
  'namespace_top_up' : ActorMethod<[string, bigint], Result_6>,
  'namespace_update_info' : ActorMethod<[UpdateNamespaceInput], Result>,
  'schnorr_public_key' : ActorMethod<
    [SchnorrAlgorithm, [] | [PublicKeyInput]],
    Result_4
  >,
  'schnorr_sign' : ActorMethod<[SchnorrAlgorithm, SignInput], Result_5>,
  'schnorr_sign_identity' : ActorMethod<
    [SchnorrAlgorithm, SignIdentityInput],
    Result_5
  >,
  'setting_add_readers' : ActorMethod<[SettingPath, Array<Principal>], Result>,
  'setting_create' : ActorMethod<[SettingPath, CreateSettingInput], Result_7>,
  'setting_get' : ActorMethod<[SettingPath], Result_8>,
  'setting_get_archived_payload' : ActorMethod<[SettingPath], Result_9>,
  'setting_get_info' : ActorMethod<[SettingPath], Result_8>,
  'setting_remove_readers' : ActorMethod<
    [SettingPath, Array<Principal>],
    Result
  >,
  'setting_update_info' : ActorMethod<
    [SettingPath, UpdateSettingInfoInput],
    Result_7
  >,
  'setting_update_payload' : ActorMethod<
    [SettingPath, UpdateSettingPayloadInput],
    Result_7
  >,
  'state_get_info' : ActorMethod<[], Result_10>,
  'validate_admin_add_allowed_apis' : ActorMethod<[Array<string>], Result>,
  'validate_admin_add_auditors' : ActorMethod<[Array<Principal>], Result>,
  'validate_admin_add_managers' : ActorMethod<[Array<Principal>], Result>,
  'validate_admin_remove_allowed_apis' : ActorMethod<[Array<string>], Result>,
  'validate_admin_remove_auditors' : ActorMethod<[Array<Principal>], Result>,
  'validate_admin_remove_managers' : ActorMethod<[Array<Principal>], Result>,
  'vetkd_encrypted_key' : ActorMethod<
    [SettingPath, Uint8Array | number[]],
    Result_5
  >,
  'vetkd_public_key' : ActorMethod<[SettingPath], Result_5>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
