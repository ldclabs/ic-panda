// Generated from the public dmsg_cose.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export type Algorithm = { 'VetKdBls12381' : null } |
  { 'Ed25519' : null } |
  { 'EcdsaSecp256k1' : null };
export interface CommercialReservation {
  'reservation_id' : Uint8Array | number[],
  'business_revision' : bigint,
  'valid_until_ms' : bigint,
  'lease_revision' : bigint,
  'weight_policy_version' : bigint,
  'units' : bigint,
  'month_utc' : number,
}
export interface CoseInit {
  'masters' : Array<MasterKey>,
  'executing_canister' : Principal,
  'derivation_version' : number,
  'daily_cycles' : bigint,
  'issuer_namespace' : string,
  'daily_executions' : number,
  'environment' : Environment,
  'governance' : Principal,
  'user_homes' : Array<Principal>,
}
export interface CoseStats {
  'executions_today' : number,
  'results' : bigint,
  'cycles' : bigint,
  'budget_day' : bigint,
  'accounts' : bigint,
  'formal_cycles_today' : bigint,
  'in_flight' : bigint,
  'max_accounts' : bigint,
  'stable_pages' : bigint,
  'unknown' : bigint,
  'formal_executions_today' : number,
  'cycles_today' : bigint,
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
  'kind' : ExecutionKind,
  'approved_at' : bigint,
  'device_id' : Uint8Array | number[],
  'security_epoch' : bigint,
  'home_cose' : Principal,
  'max_cycles' : bigint,
  'home_user' : Principal,
  'commerce' : [] | [CommercialReservation],
  'expires_at' : bigint,
}
export type ExecutionKind = {
    'Sign' : {
      'key' : KeyRequest,
      'public_key_fingerprint' : Uint8Array | number[],
      'origin' : string,
      'to_be_signed' : Uint8Array | number[],
    }
  } |
  {
    'AgentEvent' : {
      'key' : KeyRequest,
      'origin' : string,
      'event' : Uint8Array | number[],
      'principal_id' : string,
    }
  } |
  {
    'Derive' : {
      'generation' : bigint,
      'transport_key' : Uint8Array | number[],
      'root_op_id' : [] | [Uint8Array | number[]],
    }
  };
export type ExecutionOutcome = { 'Failed' : Error } |
  { 'Executing' : null } |
  { 'Authorized' : null } |
  { 'Unknown' : Error } |
  { 'ResultExpired' : null } |
  { 'Completed' : ExecutionOutput };
export type ExecutionOutput = {
    'EncryptedRootKey' : {
      'key' : KeyDescriptor,
      'encrypted_key' : Uint8Array | number[],
    }
  } |
  {
    'AgentSignature' : {
      'key' : KeyDescriptor,
      'signature' : Uint8Array | number[],
      'event_hash' : Uint8Array | number[],
    }
  } |
  { 'Signature' : { 'key' : KeyDescriptor, 'artifact' : SignedArtifact } };
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
  'algorithm' : Algorithm,
  'key_generation' : bigint,
  'public_key_fingerprint' : Uint8Array | number[],
  'derivation_version' : number,
  'public_key' : Uint8Array | number[],
  'key_id' : Uint8Array | number[],
  'home_cose' : Principal,
  'environment' : Environment,
  'master_key_name' : string,
  'purpose' : KeyPurpose,
}
export type KeyPurpose = { 'ContentRoot' : null } |
  { 'AppAction' : null } |
  { 'FileAttestation' : null } |
  { 'AgentController' : null } |
  { 'Statement' : null };
export interface KeyRequest {
  'algorithm' : Algorithm,
  'generation' : bigint,
  'purpose' : KeyPurpose,
}
export type KeySelector = { 'ContentRoot' : { 'generation' : bigint } } |
  { 'Signing' : SigningKey } |
  { 'AgentController' : { 'generation' : number } };
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
export interface SignedArtifact {
  'cose_sign1' : Uint8Array | number[],
  'cose_key' : Uint8Array | number[],
}
export type SigningAlgorithm = { 'Ed25519' : null } |
  { 'EcdsaSecp256k1' : null };
export interface SigningKey {
  'algorithm' : SigningAlgorithm,
  'purpose' : SigningPurpose,
}
export type SigningPurpose = { 'AppAction' : null } |
  { 'FileAttestation' : null } |
  { 'Statement' : null };
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
  'public_key' : ActorMethod<[Uint8Array | number[], KeySelector], Result_3>,
  'validate_admin_add_user_home' : ActorMethod<[Principal], Result_4>,
  'validate_admin_set_daily_budget' : ActorMethod<[number, bigint], Result_4>,
  'validate_initialize_keys' : ActorMethod<[], Result_4>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
