import type { Principal } from '@dfinity/principal';
import type { ActorMethod } from '@dfinity/agent';
import type { IDL } from '@dfinity/candid';

export interface AddPrizeInput {
  'claimable' : number,
  'quantity' : number,
  'expire' : number,
}
export interface AddPrizeInputV2 {
  'total_amount' : number,
  'kind' : [] | [number],
  'memo' : [] | [Uint8Array | number[]],
  'recipient' : [] | [Principal],
  'quantity' : number,
  'expire' : number,
}
export interface Airdrop {
  'weight' : bigint,
  'subaccount' : [] | [string],
  'neuron_id' : [] | [string],
}
export interface AirdropClaimInput {
  'recaptcha' : [] | [string],
  'challenge' : string,
  'code' : string,
  'lucky_code' : [] | [string],
}
export interface AirdropCodeOutput {
  'issued_at' : bigint,
  'code' : [] | [string],
  'name' : [] | [string],
  'issuer' : string,
  'filled' : number,
  'quantity' : number,
  'expire' : bigint,
  'amount' : bigint,
}
export interface AirdropHarvestInput {
  'recaptcha' : [] | [string],
  'amount' : bigint,
}
export interface AirdropLog {
  'id' : bigint,
  'ts' : bigint,
  'lucky_code' : string,
  'caller' : Principal,
  'amount' : bigint,
}
export interface AirdropStateOutput {
  'lucky_code' : [] | [string],
  'claimed' : bigint,
  'claimable' : bigint,
}
export interface Airdrops108Output {
  'status' : number,
  'ledger_updated_at' : bigint,
  'airdrops' : Array<Airdrop>,
  'ledger_weight_total' : bigint,
  'tokens_per_weight' : number,
  'error' : [] | [string],
  'neurons_hash' : string,
  'neurons_airdropped' : boolean,
  'ledger_hash' : string,
  'tokens_distributed' : bigint,
  'neurons_weight_total' : bigint,
  'neurons_updated_at' : bigint,
  'ledger_airdropped' : boolean,
}
export interface CaptchaOutput { 'challenge' : string, 'img_base64' : string }
export interface ClaimPrizeInput {
  'challenge' : Uint8Array | number[],
  'code' : string,
}
export interface ClaimPrizeOutput {
  'average' : bigint,
  'claimed' : bigint,
  'state' : AirdropStateOutput,
}
export interface LuckyDrawInput { 'icp' : number, 'amount' : [] | [bigint] }
export interface LuckyDrawLog {
  'id' : bigint,
  'ts' : bigint,
  'caller' : Principal,
  'random' : bigint,
  'icp_amount' : bigint,
  'amount' : bigint,
}
export interface LuckyDrawOutput {
  'airdrop_cryptogram' : [] | [string],
  'prize_cryptogram' : [] | [string],
  'luckypool_empty' : boolean,
  'random' : bigint,
  'amount' : bigint,
}
export interface NameInput { 'name' : string, 'old_name' : [] | [string] }
export interface NameOutput {
  'code' : string,
  'name' : string,
  'annual_fee' : bigint,
  'deposit' : bigint,
  'created_at' : bigint,
}
export interface Notification {
  'id' : number,
  'level' : number,
  'message' : string,
  'dismiss' : boolean,
  'timeout' : number,
}
export interface PrizeClaimLog {
  'claimed_at' : bigint,
  'prize' : PrizeOutput,
  'amount' : bigint,
}
export interface PrizeOutput {
  'id' : Uint8Array | number[],
  'fee' : bigint,
  'issued_at' : bigint,
  'code' : [] | [string],
  'kind' : number,
  'memo' : [] | [Uint8Array | number[]],
  'name' : [] | [string],
  'refund_amount' : bigint,
  'issuer' : string,
  'filled' : number,
  'quantity' : number,
  'expire' : bigint,
  'ended_at' : bigint,
  'amount' : bigint,
  'sys_subsidy' : bigint,
}
export type Result = { 'Ok' : PrizeOutput } |
  { 'Err' : string };
export type Result_1 = { 'Ok' : null } |
  { 'Err' : string };
export type Result_10 = { 'Ok' : bigint } |
  { 'Err' : string };
export type Result_11 = { 'Ok' : [] | [NameOutput] } |
  { 'Err' : null };
export type Result_12 = { 'Ok' : Principal } |
  { 'Err' : string };
export type Result_13 = { 'Ok' : NameOutput } |
  { 'Err' : string };
export type Result_14 = { 'Ok' : State } |
  { 'Err' : null };
export type Result_15 = { 'Ok' : bigint } |
  { 'Err' : string };
export type Result_16 = { 'Ok' : Principal } |
  { 'Err' : null };
export type Result_2 = { 'Ok' : AirdropStateOutput } |
  { 'Err' : string };
export type Result_3 = { 'Ok' : AirdropStateOutput } |
  { 'Err' : null };
export type Result_4 = { 'Ok' : Airdrops108Output } |
  { 'Err' : null };
export type Result_5 = { 'Ok' : CaptchaOutput } |
  { 'Err' : string };
export type Result_6 = { 'Ok' : ClaimPrizeOutput } |
  { 'Err' : string };
export type Result_7 = { 'Ok' : LuckyDrawOutput } |
  { 'Err' : string };
export type Result_8 = { 'Ok' : string } |
  { 'Err' : string };
export type Result_9 = { 'Ok' : boolean } |
  { 'Err' : string };
export interface State {
  'latest_luckydraw_logs' : Array<LuckyDrawLog>,
  'total_luckydraw' : bigint,
  'latest_airdrop_logs' : Array<AirdropLog>,
  'managers' : [] | [Array<Principal>],
  'total_airdrop' : bigint,
  'total_prize_count' : [] | [bigint],
  'total_airdrop_count' : bigint,
  'total_prize' : [] | [bigint],
  'lucky_code' : [] | [number],
  'airdrop_amount' : [] | [bigint],
  'luckiest_luckydraw_logs' : Array<LuckyDrawLog>,
  'airdrop_balance' : bigint,
  'total_luckydraw_count' : bigint,
  'total_prizes_count' : [] | [bigint],
  'prize_subsidy' : [] | [[bigint, number, number, number, number, number]],
  'total_luckydraw_icp' : bigint,
}
export interface _SERVICE {
  'add_prize' : ActorMethod<[AddPrizeInputV2], Result>,
  'admin_collect_icp' : ActorMethod<[bigint], Result_1>,
  'admin_collect_tokens' : ActorMethod<[bigint], Result_1>,
  'admin_set_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'airdrop' : ActorMethod<[AirdropClaimInput], Result_2>,
  'airdrop_codes_of' : ActorMethod<[Principal], Array<AirdropCodeOutput>>,
  'airdrop_logs' : ActorMethod<
    [[] | [bigint], [] | [bigint]],
    Array<AirdropLog>
  >,
  'airdrop_state_of' : ActorMethod<[[] | [Principal]], Result_3>,
  'airdrops108_of' : ActorMethod<[[] | [Principal]], Result_4>,
  'api_version' : ActorMethod<[], number>,
  'captcha' : ActorMethod<[], Result_5>,
  'claim_prize' : ActorMethod<[ClaimPrizeInput], Result_6>,
  'harvest' : ActorMethod<[AirdropHarvestInput], Result_2>,
  'luckydraw' : ActorMethod<[LuckyDrawInput], Result_7>,
  'luckydraw_logs' : ActorMethod<
    [[] | [bigint], [] | [bigint]],
    Array<LuckyDrawLog>
  >,
  'manager_add_notification' : ActorMethod<[Notification], Result_1>,
  'manager_add_prize' : ActorMethod<[AddPrizeInput], Result_8>,
  'manager_add_prize_v2' : ActorMethod<[AddPrizeInputV2], Result_8>,
  'manager_ban_users' : ActorMethod<[Array<Principal>], Result_1>,
  'manager_get_airdrop_key' : ActorMethod<[], Result_8>,
  'manager_remove_notifications' : ActorMethod<
    [Uint8Array | number[]],
    Result_1
  >,
  'manager_set_challenge_pub_key' : ActorMethod<[string], Result_1>,
  'manager_start_airdrops108' : ActorMethod<[], Result_9>,
  'manager_update_airdrop_amount' : ActorMethod<[bigint], Result_1>,
  'manager_update_airdrop_balance' : ActorMethod<[bigint], Result_1>,
  'manager_update_airdrops108_ledger_list' : ActorMethod<
    [Uint8Array | number[]],
    Result_10
  >,
  'manager_update_airdrops108_neurons_list' : ActorMethod<
    [Uint8Array | number[]],
    Result_10
  >,
  'manager_update_prize_subsidy' : ActorMethod<
    [[] | [[bigint, number, number, number, number, number]]],
    Result_1
  >,
  'my_luckydraw_logs' : ActorMethod<
    [[] | [bigint], [] | [bigint]],
    Array<LuckyDrawLog>
  >,
  'name_lookup' : ActorMethod<[string], Result_11>,
  'name_of' : ActorMethod<[[] | [Principal]], Result_11>,
  'notifications' : ActorMethod<[], Array<Notification>>,
  'principal_by_luckycode' : ActorMethod<[string], Result_12>,
  'prize' : ActorMethod<[string], Result_2>,
  'prize_claim_logs' : ActorMethod<
    [Principal, [] | [bigint], [] | [bigint]],
    Array<PrizeClaimLog>
  >,
  'prize_info' : ActorMethod<[string, [] | [Principal]], Result>,
  'prize_issue_logs' : ActorMethod<
    [Principal, [] | [bigint]],
    Array<PrizeOutput>
  >,
  'prize_ongoing' : ActorMethod<[], Array<PrizeOutput>>,
  'prizes_of' : ActorMethod<
    [[] | [Principal]],
    Array<[number, number, number, number, number, number]>
  >,
  'register_name' : ActorMethod<[NameInput], Result_13>,
  'state' : ActorMethod<[], Result_14>,
  'unregister_name' : ActorMethod<[NameInput], Result_15>,
  'update_name' : ActorMethod<[NameInput], Result_13>,
  'validate2_admin_collect_icp' : ActorMethod<[bigint], Result_8>,
  'validate2_admin_set_managers' : ActorMethod<[Array<Principal>], Result_8>,
  'validate_admin_collect_icp' : ActorMethod<[bigint], Result_1>,
  'validate_admin_collect_tokens' : ActorMethod<[bigint], Result_8>,
  'validate_admin_set_managers' : ActorMethod<[Array<Principal>], Result_1>,
  'whoami' : ActorMethod<[], Result_16>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
