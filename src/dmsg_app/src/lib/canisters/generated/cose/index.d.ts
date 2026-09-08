// Generated from the public dmsg_cose.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export type Algorithm = { 'VetKdBls12381' : null } |
  { 'Ed25519' : null } |
  { 'Bip340' : null } |
  { 'EcdsaSecp256k1' : null };
export interface CoseInit {
  'masters' : Array<MasterKey>,
  'executing_canister' : Principal,
  'derivation_version' : number,
  'daily_cycles' : bigint,
  'initial_home_user' : Principal,
  'daily_executions' : number,
  'environment' : Environment,
}
export type Environment = { 'Local' : null } |
  { 'Production' : null } |
  { 'Staging' : null };
export type Error = { 'MigrationKeyUnavailable' : null } |
  { 'LegacyWriteDisabled' : null } |
  { 'InvalidInput' : string } |
  { 'RekeyRequired' : null } |
  { 'VersionConflict' : null } |
  { 'ExecutionUnknown' : null } |
  { 'IntegrityFailed' : null } |
  { 'NotFound' : null } |
  { 'FeeBlocked' : null } |
  { 'DeviceNotApproved' : null } |
  { 'Locked' : null } |
  { 'RecoveryIncomplete' : null } |
  { 'PolicyStale' : null } |
  { 'IdempotencyConflict' : null } |
  { 'UnsupportedProtocol' : null } |
  { 'Unavailable' : string } |
  { 'Forbidden' : null } |
  { 'ResultExpired' : null } |
  { 'Expired' : null } |
  { 'QuotaExceeded' : null } |
  { 'AuthRequired' : null } |
  { 'Pending' : null };
export interface ExecutionGrant {
  'request_id' : Uint8Array | number[],
  'device_sequence' : bigint,
  'execution_sequence' : bigint,
  'subject' : Uint8Array | number[],
  'kind' : ExecutionKind,
  'approved_at' : bigint,
  'device_id' : Uint8Array | number[],
  'security_epoch' : bigint,
  'home_cose' : Principal,
  'max_cycles' : bigint,
  'home_user' : Principal,
  'expires_at' : bigint,
}
export type ExecutionKind = {
    'Sign' : { 'key' : KeyRequest, 'canonical_payload' : Uint8Array | number[] }
  } |
  {
    'Derive' : {
      'generation' : bigint,
      'transport_key' : Uint8Array | number[],
      'root_op_id' : [] | [Uint8Array | number[]],
    }
  };
export interface ExecutionResult {
  'key' : [] | [KeyDescriptor],
  'status' : ExecutionStatus,
  'result' : [] | [Uint8Array | number[]],
  'charged_cycles' : bigint,
}
export type ExecutionStatus = { 'Failed' : null } |
  { 'Executing' : null } |
  { 'Authorized' : null } |
  { 'Unknown' : null } |
  { 'ResultExpired' : null } |
  { 'Completed' : null };
export type Initialization = { 'Ready' : null } |
  { 'Uninitialized' : null } |
  { 'Initializing' : null };
export interface KeyDescriptor {
  'algorithm' : Algorithm,
  'key_generation' : bigint,
  'public_key_fingerprint' : Uint8Array | number[],
  'provider' : [] | [string],
  'subject' : Uint8Array | number[],
  'derivation_version' : number,
  'public_key' : Uint8Array | number[],
  'key_id' : Uint8Array | number[],
  'home_cose' : Principal,
  'environment' : Environment,
  'master_key_name' : string,
  'purpose' : KeyPurpose,
}
export type KeyPurpose = { 'ContentRoot' : null } |
  { 'FileAttestation' : null } |
  { 'ProviderController' : null } |
  { 'Identity' : null } |
  { 'Statement' : null };
export interface KeyRequest {
  'algorithm' : Algorithm,
  'provider' : [] | [string],
  'generation' : bigint,
  'purpose' : KeyPurpose,
}
export interface KeyState {
  'initialization' : Initialization,
  'fingerprints' : Array<Uint8Array | number[]>,
  'error' : [] | [string],
  'config' : CoseInit,
}
export interface MasterKey {
  'algorithm' : Algorithm,
  'expected_fingerprint' : Uint8Array | number[],
  'key_name' : string,
}
export type Result = { 'Ok' : KeyDescriptor } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : ExecutionResult } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : KeyState } |
  { 'Err' : Error };
export type Result_3 = { 'Ok' : null } |
  { 'Err' : Error };
export interface _SERVICE {
  'describe_key' : ActorMethod<[Uint8Array | number[]], Result>,
  'execute' : ActorMethod<[ExecutionGrant], Result_1>,
  'get_execution' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_1
  >,
  'initialize_keys' : ActorMethod<[], Result_2>,
  'key_state' : ActorMethod<[], KeyState>,
  'register_subject' : ActorMethod<[Uint8Array | number[]], Result_3>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
