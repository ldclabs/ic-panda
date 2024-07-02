import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface BurnInput {
  'fee_rate' : bigint,
  'address' : string,
  'amount' : bigint,
}
export interface BurnOutput {
  'tip_height' : bigint,
  'block_index' : bigint,
  'txid' : Uint8Array | number[],
  'instructions' : bigint,
}
export interface BurnedUtxos {
  'height' : bigint,
  'block_index' : bigint,
  'txid' : Uint8Array | number[],
  'address' : Uint8Array | number[],
  'utxos' : Array<Utxo>,
}
export interface CollectedUtxo {
  'height' : bigint,
  'principal' : Principal,
  'block_index' : bigint,
  'utxo' : Utxo,
}
export interface InitArgs {
  'ecdsa_key_name' : string,
  'chain' : number,
  'ledger_canister' : [] | [Principal],
  'chain_canister' : [] | [Principal],
}
export interface MintOutput { 'instructions' : bigint, 'amount' : bigint }
export interface MintedUtxo {
  'block_index' : bigint,
  'utxo' : Utxo,
  'minted_at' : bigint,
}
export type MinterArgs = { 'Upgrade' : UpgradeArgs } |
  { 'Init' : InitArgs };
export type Result = { 'Ok' : null } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : BurnOutput } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : string } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : State } |
  { 'Err' : null };
export type Result_4 = { 'Ok' : Array<MintedUtxo> } |
  { 'Err' : string };
export type Result_5 = { 'Ok' : MintOutput } |
  { 'Err' : string };
export interface State {
  'tokens_minted_count' : bigint,
  'ecdsa_key_name' : [] | [string],
  'managers' : Array<Principal>,
  'burning_utxos' : Array<
    [bigint, [Principal, Uint8Array | number[], bigint, bigint, string]]
  >,
  'chain' : string,
  'tokens_burned_count' : bigint,
  'collected_utxos' : bigint,
  'tokens_burned' : bigint,
  'accounts' : bigint,
  'minter_subaddress' : [] | [string],
  'ledger_canister' : [] | [Principal],
  'minter_address' : [] | [string],
  'burned_utxos' : bigint,
  'chain_canister' : [] | [Principal],
  'tokens_minted' : bigint,
}
export interface UpgradeArgs {
  'ledger_canister' : [] | [Principal],
  'chain_canister' : [] | [Principal],
}
export interface Utxo {
  'height' : bigint,
  'value' : bigint,
  'txid' : Uint8Array | number[],
  'vout' : number,
}
export interface _SERVICE {
  'admin_set_managers' : ActorMethod<[Array<Principal>], Result>,
  'api_version' : ActorMethod<[], number>,
  'burn_ckdoge' : ActorMethod<[BurnInput], Result_1>,
  'get_address' : ActorMethod<[], Result_2>,
  'get_state' : ActorMethod<[], Result_3>,
  'list_burned_utxos' : ActorMethod<[bigint, number], Array<BurnedUtxos>>,
  'list_collected_utxos' : ActorMethod<[bigint, number], Array<CollectedUtxo>>,
  'list_minted_utxos' : ActorMethod<[[] | [Principal]], Result_4>,
  'mint_ckdoge' : ActorMethod<[], Result_5>,
  'retry_burn_ckdoge' : ActorMethod<[bigint, [] | [bigint]], Result_1>,
  'validate_admin_set_managers' : ActorMethod<[Array<Principal>], Result>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
