// Generated from the public dmsg_cose.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export interface CoseInit {
  'executing_canister' : Principal,
  'derivation_version' : number,
  'daily_cycles' : bigint,
  'issuer_namespace' : string,
  'daily_executions' : number,
  'environment' : Environment,
  'master' : MasterKey,
  'governance' : Principal,
  'user_homes' : Array<Principal>,
}
export interface CoseStats {
  'executions_today' : number,
  'results' : bigint,
  'cycles' : bigint,
  'budget_day' : bigint,
  'accounts' : bigint,
  'in_flight' : bigint,
  'max_accounts' : bigint,
  'stable_pages' : bigint,
  'unknown' : bigint,
  'cycles_today' : bigint,
}
export interface EncryptedRootKey {
  'key' : KeyDescriptor,
  'encrypted_key' : Uint8Array | number[],
}
export type Environment = { 'Local' : null } |
  { 'Production' : null } |
  { 'Staging' : null };
export type Error = { 'MigrationKeyUnavailable' : null } |
  { 'LegacyWriteDisabled' : null } |
  { 'InvalidInput' : string } |
  { 'IntervalReserved' : null } |
  { 'NeuronOccupied' : null } |
  { 'RekeyRequired' : null } |
  { 'VersionConflict' : null } |
  { 'ExecutionUnknown' : null } |
  { 'IntegrityFailed' : null } |
  { 'IdTimestampOutOfRange' : null } |
  { 'NotFound' : null } |
  { 'FeeBlocked' : null } |
  { 'DeviceNotApproved' : null } |
  { 'Locked' : null } |
  { 'MembershipClosing' : null } |
  { 'RecoveryIncomplete' : null } |
  { 'IdCapacityExceeded' : null } |
  { 'PolicyStale' : null } |
  { 'IdGeneratorStateConflict' : null } |
  { 'IdempotencyConflict' : null } |
  { 'UnsupportedProtocol' : null } |
  { 'Unavailable' : string } |
  { 'MembershipStale' : null } |
  { 'Forbidden' : null } |
  { 'ResultExpired' : null } |
  { 'Expired' : null } |
  { 'MembershipIneligible' : null } |
  { 'QuotaExceeded' : null } |
  { 'AuthRequired' : null } |
  { 'Pending' : null };
export interface ExecutionCleanup {
  'next_after' : [] | [Uint8Array | number[]],
  'homes_scanned' : number,
  'results_removed' : number,
}
export interface ExecutionGrant {
  'account_id' : Uint8Array | number[],
  'request_id' : Uint8Array | number[],
  'device_sequence' : bigint,
  'execution_sequence' : bigint,
  'generation' : bigint,
  'approved_at' : bigint,
  'device_id' : Uint8Array | number[],
  'security_epoch' : bigint,
  'home_cose' : Principal,
  'max_cycles' : bigint,
  'home_user' : Principal,
  'transport_key' : Uint8Array | number[],
  'expires_at' : bigint,
}
export type ExecutionOutcome = { 'Failed' : Error } |
  { 'Executing' : null } |
  { 'Authorized' : null } |
  { 'Unknown' : Error } |
  { 'ResultExpired' : null } |
  { 'Completed' : EncryptedRootKey };
export interface ExecutionResult {
  'request_id' : Uint8Array | number[],
  'cycles_cost_upper_bound' : bigint,
  'cycles_charged' : bigint,
  'outcome' : ExecutionOutcome,
}
export type Initialization = { 'Ready' : null } |
  { 'Uninitialized' : null } |
  { 'Initializing' : null };
export interface KeyDescriptor {
  'account_id' : Uint8Array | number[],
  'key_generation' : bigint,
  'public_key_fingerprint' : Uint8Array | number[],
  'derivation_version' : number,
  'public_key' : Uint8Array | number[],
  'home_cose' : Principal,
  'environment' : Environment,
  'master_key_name' : string,
}
export interface KeyState {
  'initialization' : Initialization,
  'error' : [] | [string],
  'fingerprint' : [] | [Uint8Array | number[]],
  'config' : CoseInit,
}
export interface MasterKey {
  'expected_fingerprint' : Uint8Array | number[],
  'key_name' : string,
}
export type Result = { 'Ok' : null } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : ExecutionResult } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : KeyState } |
  { 'Err' : Error };
export type Result_3 = { 'Ok' : KeyDescriptor } |
  { 'Err' : Error };
export type Result_4 = { 'Ok' : string } |
  { 'Err' : string };
export interface _SERVICE {
  'admin_add_user_home' : ActorMethod<[Principal], Result>,
  'admin_set_daily_budget' : ActorMethod<[number, bigint], Result>,
  'cose_stats' : ActorMethod<[], CoseStats>,
  'execute' : ActorMethod<[ExecutionGrant], Result_1>,
  'get_execution' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_1
  >,
  'initialize_keys' : ActorMethod<[], Result_2>,
  'key_state' : ActorMethod<[], KeyState>,
  'prune_executions' : ActorMethod<
    [[] | [Uint8Array | number[]]],
    ExecutionCleanup
  >,
  'root_public_key' : ActorMethod<[Uint8Array | number[], bigint], Result_3>,
  'validate_admin_add_user_home' : ActorMethod<[Principal], Result_4>,
  'validate_admin_set_daily_budget' : ActorMethod<[number, bigint], Result_4>,
  'validate_initialize_keys' : ActorMethod<[], Result_4>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
