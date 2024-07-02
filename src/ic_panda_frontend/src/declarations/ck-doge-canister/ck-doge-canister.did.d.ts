import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface BlockRef { 'height' : bigint, 'hash' : Uint8Array | number[] }
export type ChainArgs = { 'Upgrade' : UpgradeArgs } |
  { 'Init' : InitArgs };
export interface CreateTxInput {
  'from_subaccount' : [] | [Uint8Array | number[]],
  'fee_rate' : bigint,
  'address' : string,
  'utxos' : Array<Utxo>,
  'amount' : bigint,
}
export interface CreateTxOutput {
  'tx' : Uint8Array | number[],
  'fee' : bigint,
  'tip_height' : bigint,
  'instructions' : bigint,
}
export interface InitArgs {
  'ecdsa_key_name' : string,
  'chain' : number,
  'prev_start_height' : bigint,
  'prev_start_blockhash' : string,
  'min_confirmations' : number,
}
export interface RPCAgent {
  'proxy_token' : [] | [string],
  'api_token' : [] | [string],
  'endpoint' : string,
  'name' : string,
  'max_cycles' : bigint,
}
export type Result = { 'Ok' : null } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : CreateTxOutput } |
  { 'Err' : string };
export type Result_2 = { 'Ok' : string } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : bigint } |
  { 'Err' : string };
export type Result_4 = { 'Ok' : State } |
  { 'Err' : null };
export type Result_5 = { 'Ok' : BlockRef } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : UnspentTx } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : UtxosOutput } |
  { 'Err' : string };
export type Result_8 = { 'Ok' : SendTxOutput } |
  { 'Err' : string };
export interface SendTxInput {
  'tx' : Uint8Array | number[],
  'from_subaccount' : [] | [Uint8Array | number[]],
}
export interface SendTxOutput {
  'tip_height' : bigint,
  'txid' : Uint8Array | number[],
  'instructions' : bigint,
}
export interface State {
  'start_height' : bigint,
  'processed_height' : bigint,
  'tip_height' : bigint,
  'ecdsa_key_name' : [] | [string],
  'managers' : Array<Principal>,
  'rpc_proxy_public_key' : [] | [string],
  'chain' : string,
  'confirmed_height' : bigint,
  'unconfirmed_utxos' : bigint,
  'unprocessed_blocks' : bigint,
  'syncing_status' : [] | [number],
  'last_errors' : Array<string>,
  'confirmed_utxs' : bigint,
  'tip_blockhash' : string,
  'unconfirmed_utxs' : bigint,
  'min_confirmations' : number,
  'processed_blockhash' : string,
  'rpc_agents' : Array<RPCAgent>,
  'confirmed_utxos' : bigint,
  'start_blockhash' : string,
}
export interface TxStatus {
  'height' : bigint,
  'tip_height' : bigint,
  'confirmed_height' : bigint,
}
export interface UnspentTx {
  'height' : bigint,
  'output' : Array<Uint8Array | number[]>,
  'spent' : Array<[] | [Uint8Array | number[]]>,
}
export interface UpgradeArgs { 'min_confirmations' : [] | [number] }
export interface Utxo {
  'height' : bigint,
  'value' : bigint,
  'txid' : Uint8Array | number[],
  'vout' : number,
}
export interface UtxosOutput {
  'tip_height' : bigint,
  'confirmed_height' : bigint,
  'utxos' : Array<Utxo>,
  'tip_blockhash' : Uint8Array | number[],
}
export interface _SERVICE {
  'admin_restart_syncing' : ActorMethod<[[] | [number]], Result>,
  'admin_set_agent' : ActorMethod<[Array<RPCAgent>], Result>,
  'admin_set_managers' : ActorMethod<[Array<Principal>], Result>,
  'api_version' : ActorMethod<[], number>,
  'create_tx' : ActorMethod<[CreateTxInput], Result_1>,
  'get_address' : ActorMethod<[], Result_2>,
  'get_balance' : ActorMethod<[string], Result_3>,
  'get_balance_b' : ActorMethod<[Uint8Array | number[]], bigint>,
  'get_state' : ActorMethod<[], Result_4>,
  'get_tip' : ActorMethod<[], Result_5>,
  'get_tx_status' : ActorMethod<[Uint8Array | number[]], [] | [TxStatus]>,
  'get_utx' : ActorMethod<[string], Result_6>,
  'get_utx_b' : ActorMethod<[Uint8Array | number[]], [] | [UnspentTx]>,
  'list_utxos' : ActorMethod<[string, number, boolean], Result_7>,
  'list_utxos_b' : ActorMethod<
    [Uint8Array | number[], number, boolean],
    Result_7
  >,
  'send_tx' : ActorMethod<[SendTxInput], Result_8>,
  'sign_and_send_tx' : ActorMethod<[SendTxInput], Result_8>,
  'validate_admin_set_managers' : ActorMethod<[Array<Principal>], Result>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
