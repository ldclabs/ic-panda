import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export type CanisterArgs = { 'Upgrade' : UpgradeArgs } |
  { 'Init' : InitArgs };
export interface Delegation {
  'pubkey' : Uint8Array | number[],
  'targets' : [] | [Array<Principal>],
  'expiration' : bigint,
}
export interface InitArgs {
  'session_expires_in_ms' : bigint,
  'governance_canister' : [] | [Principal],
}
export type Result = { 'Ok' : null } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : SignedDelegation } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : string } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : StateInfo } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : Uint8Array | number[] } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : SignInResponse } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : Principal } |
  { 'Err' : string };
export interface SignInResponse {
  'user_key' : Uint8Array | number[],
  'seed' : Uint8Array | number[],
  'expiration' : bigint,
}
export interface SignedDelegation {
  'signature' : Uint8Array | number[],
  'delegation' : Delegation,
}
export interface StateInfo {
  'session_expires_in_ms' : bigint,
  'governance_canister' : [] | [Principal],
  'statement' : string,
  'domains' : Array<[string, string]>,
}
export interface UpgradeArgs {
  'session_expires_in_ms' : [] | [bigint],
  'governance_canister' : [] | [Principal],
}
export interface _SERVICE {
  'admin_remove_domain' : ActorMethod<[string], Result>,
  'admin_update_domain' : ActorMethod<[string, string], Result>,
  'admin_update_statement' : ActorMethod<[string], Result>,
  'get_delegation' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[], bigint],
    Result_1
  >,
  'get_sign_in_with_ethereum_message' : ActorMethod<
    [string, string, number, bigint],
    Result_2
  >,
  'get_sign_in_with_solana_message' : ActorMethod<
    [string, string, bigint],
    Result_2
  >,
  'info' : ActorMethod<[], Result_3>,
  'my_iv' : ActorMethod<[], Result_4>,
  'sign_in_with_ethereum' : ActorMethod<
    [
      string,
      string,
      number,
      bigint,
      string,
      Uint8Array | number[],
      Uint8Array | number[],
      Uint8Array | number[],
    ],
    Result_5
  >,
  'sign_in_with_solana' : ActorMethod<
    [
      string,
      string,
      bigint,
      string,
      Uint8Array | number[],
      Uint8Array | number[],
      Uint8Array | number[],
    ],
    Result_5
  >,
  'validate_admin_remove_domain' : ActorMethod<[string], Result_2>,
  'validate_admin_update_domain' : ActorMethod<[string, string], Result_2>,
  'validate_admin_update_statement' : ActorMethod<[string], Result_2>,
  'verify_envelope' : ActorMethod<
    [Uint8Array | number[], [] | [Principal], [] | [Uint8Array | number[]]],
    Result_6
  >,
  'whoami' : ActorMethod<[], Result_6>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
