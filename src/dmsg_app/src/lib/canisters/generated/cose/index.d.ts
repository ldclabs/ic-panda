// Generated from the public dmsg_cose.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export type Algorithm = { 'VetKdBls12381' : null } |
  { 'Ed25519' : null } |
  { 'EcdsaSecp256k1' : null };
export interface CoseInit {
  'masters' : Array<MasterKey>,
  'executing_canister' : Principal,
  'derivation_version' : number,
  'daily_cycles' : bigint,
  'initial_home_user' : Principal,
  'issuer_namespace' : string,
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
  { 'IdTimestampOutOfRange' : null } |
  { 'NotFound' : null } |
  { 'FeeBlocked' : null } |
  { 'DeviceNotApproved' : null } |
  { 'Locked' : null } |
  { 'RecoveryIncomplete' : null } |
  { 'IdCapacityExceeded' : null } |
  { 'PolicyStale' : null } |
  { 'IdGeneratorStateConflict' : null } |
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
  { 'Signature' : { 'key' : KeyDescriptor, 'artifact' : SignedArtifact } };
export interface ExecutionResult {
  'request_id' : Uint8Array | number[],
  'charged_cycles' : bigint,
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
  { 'FileAttestation' : null } |
  { 'Statement' : null };
export interface KeyRequest {
  'algorithm' : Algorithm,
  'generation' : bigint,
  'purpose' : KeyPurpose,
}
export type KeySelector = { 'ContentRoot' : { 'generation' : bigint } } |
  { 'Signing' : SigningKey };
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
export type Result = { 'Ok' : ExecutionResult } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : KeyState } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : KeyDescriptor } |
  { 'Err' : Error };
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
export type SigningPurpose = { 'FileAttestation' : null } |
  { 'Statement' : null };
export interface _SERVICE {
  'execute' : ActorMethod<[ExecutionGrant], Result>,
  'get_execution' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result
  >,
  'initialize_keys' : ActorMethod<[], Result_1>,
  'key_state' : ActorMethod<[], KeyState>,
  'public_key' : ActorMethod<[Uint8Array | number[], KeySelector], Result_2>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
