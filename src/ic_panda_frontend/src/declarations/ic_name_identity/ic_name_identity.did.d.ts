import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export type ChainArgs = { 'Upgrade' : UpgradeArgs } |
  { 'Init' : InitArgs };
export interface Delegation {
  'pubkey' : Uint8Array | number[],
  'targets' : [] | [Array<Principal>],
  'expiration' : bigint,
}
export interface Delegator {
  'owner' : Principal,
  'role' : number,
  'sign_in_at' : bigint,
}
export interface InitArgs { 'session_expires_in_ms' : bigint, 'name' : string }
export interface NameAccount { 'name' : string, 'account' : Principal }
export type Result = { 'Ok' : Array<Delegator> } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : null } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : SignedDelegation } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : Array<NameAccount> } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : Principal } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : State } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : SignInResponse } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : string } |
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
export interface State {
  'session_expires_in_ms' : bigint,
  'name' : string,
  'sign_in_count' : bigint,
}
export interface UpgradeArgs {
  'session_expires_in_ms' : [] | [bigint],
  'name' : [] | [string],
}
export interface _SERVICE {
  'activate_name' : ActorMethod<[string], Result>,
  'add_delegator' : ActorMethod<[string, Principal, number], Result>,
  'admin_reset_name' : ActorMethod<[string, Array<Principal>], Result_1>,
  'get_delegation' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[], bigint],
    Result_2
  >,
  'get_delegators' : ActorMethod<[string], Result>,
  'get_my_accounts' : ActorMethod<[], Result_3>,
  'get_principal' : ActorMethod<[string], Result_4>,
  'get_state' : ActorMethod<[], Result_5>,
  'leave_delegation' : ActorMethod<[string], Result_1>,
  'remove_delegator' : ActorMethod<[string, Principal], Result_1>,
  'sign_in' : ActorMethod<
    [string, Uint8Array | number[], Uint8Array | number[]],
    Result_6
  >,
  'validate_admin_reset_name' : ActorMethod<
    [string, Array<Principal>],
    Result_7
  >,
  'whoami' : ActorMethod<[], Result_4>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
