// Generated from the public dmsg_user.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export interface Account {
  'owner' : Principal,
  'subaccount' : [] | [Uint8Array | number[]],
}
export type AccountCommand = {
    'DisputeRecovery' : {
      'op_id' : Uint8Array | number[],
      'dispute' : Uint8Array | number[],
    }
  } |
  { 'ConfirmRecovery' : { 'proof' : Uint8Array | number[] } } |
  {
    'BindAuth' : { 'principal' : Principal, 'nonce' : Uint8Array | number[] }
  } |
  {
    'ReserveRoot' : {
      'op_id' : Uint8Array | number[],
      'expected_generation' : bigint,
    }
  } |
  { 'RemoveAuth' : { 'principal' : Principal } } |
  { 'AuthorizeHandle' : { 'intent' : HandleIntent } } |
  {
    'SetRecovery' : {
      'proof' : Uint8Array | number[],
      'policy' : RecoveryPolicy,
    }
  } |
  {
    'AddDevice' : { 'device' : DeviceInput, 'proof' : Uint8Array | number[] }
  } |
  {
    'CommitRoot' : {
      'op_id' : Uint8Array | number[],
      'root' : ContentRootRef,
      'expected_generation' : bigint,
    }
  } |
  { 'RevokeDevice' : { 'device_id' : Uint8Array | number[] } } |
  { 'SetPolicy' : { 'policy' : SensitivePolicy } };
export interface AccountMutation {
  'subject' : Uint8Array | number[],
  'command' : AccountCommand,
  'approval' : Approval,
  'expected_version' : bigint,
}
export type AccountStatus = { 'Active' : null } |
  { 'RecoveryDisputed' : null };
export type Algorithm = { 'VetKdBls12381' : null } |
  { 'Ed25519' : null } |
  { 'Bip340' : null } |
  { 'EcdsaSecp256k1' : null };
export interface Approval {
  'request_id' : Uint8Array | number[],
  'signature' : Uint8Array | number[],
  'device_id' : Uint8Array | number[],
  'security_epoch' : bigint,
  'expires_at' : bigint,
  'sequence' : bigint,
}
export interface AuthorizedExecution {
  'result' : ExecutionResult,
  'command_digest' : Uint8Array | number[],
  'grant' : ExecutionGrant,
}
export interface Budget {
  'day' : bigint,
  'executions' : number,
  'cycles' : bigint,
}
export type Capability = { 'FormalApprove' : null } |
  { 'ContentSign' : null } |
  { 'VaultUnlock' : null } |
  { 'RootManage' : null } |
  { 'PaymentOffer' : null };
export interface CertifiedBatch {
  'certificate' : Uint8Array | number[],
  'schema' : number,
  'entries' : Array<CertifiedEntry>,
  'canister' : Principal,
}
export interface CertifiedEntry {
  'key' : Uint8Array | number[],
  'value' : [] | [Uint8Array | number[]],
  'witness' : Uint8Array | number[],
}
export interface ContentRootRef {
  'key_generation' : bigint,
  'derivation_version' : number,
  'recovery_generation' : bigint,
  'generation' : bigint,
  'suite' : string,
  'home_cose' : Principal,
  'bundle_digest' : Uint8Array | number[],
}
export type ControllerRole = { 'Administrator' : null } |
  { 'Member' : null };
export interface CreateSubject {
  'op_id' : Uint8Array | number[],
  'device' : DeviceInput,
  'proof' : Uint8Array | number[],
  'expires_at' : bigint,
}
export interface Device {
  'added_at' : bigint,
  'added_by' : [] | [Uint8Array | number[]],
  'revoked_at' : [] | [bigint],
  'next_sequence' : bigint,
  'input' : DeviceInput,
}
export interface DeviceInput {
  'capabilities' : Array<Capability>,
  'role' : ControllerRole,
  'device_id' : Uint8Array | number[],
  'hpke_pub' : Uint8Array | number[],
  'signing_pub' : Uint8Array | number[],
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
export interface ExecuteRequest {
  'subject' : Uint8Array | number[],
  'kind' : ExecutionKind,
  'max_cycles' : bigint,
  'approval' : Approval,
}
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
export type HandleAction = { 'AcceptTransfer' : null } |
  { 'Register' : null } |
  { 'Transfer' : null } |
  { 'ClaimLegacy' : null };
export interface HandleAuthorization {
  'intent' : HandleIntent,
  'consumed' : boolean,
  'expires_at' : bigint,
}
export interface HandleIntent {
  'handle_canister' : Principal,
  'action' : HandleAction,
  'subject' : Uint8Array | number[],
  'op_id' : Uint8Array | number[],
  'target_subject' : [] | [Uint8Array | number[]],
  'handle' : string,
  'expected_version' : bigint,
  'terms_digest' : Uint8Array | number[],
}
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
export interface OperationReceipt {
  'id' : Uint8Array | number[],
  'account_version' : bigint,
  'digest' : Uint8Array | number[],
}
export interface PaymentOffer {
  'quote_scope' : Uint8Array | number[],
  'issued_at' : bigint,
  'subject' : Uint8Array | number[],
  'recipient' : Account,
  'device_id' : Uint8Array | number[],
  'security_epoch' : bigint,
  'version' : bigint,
  'ledger' : Principal,
  'offer_id' : Uint8Array | number[],
  'home_payment' : Principal,
  'expires_at' : bigint,
  'recipient_net' : bigint,
}
export interface PendingRecovery {
  'reconfirmed' : boolean,
  'request' : RecoveryRequest,
  'execute_after' : bigint,
  'confirmation' : [] | [RecoveryConfirmation],
  'dispute' : [] | [Uint8Array | number[]],
}
export interface RecoveryConfirmation {
  'request_id' : Uint8Array | number[],
  'dispute' : Uint8Array | number[],
  'expires_at' : bigint,
}
export interface RecoveryPolicy {
  'delay_ns' : bigint,
  'generation' : bigint,
  'hpke_pub' : Uint8Array | number[],
  'signing_pub' : Uint8Array | number[],
}
export interface RecoveryRequest {
  'op_id' : Uint8Array | number[],
  'generation' : bigint,
  'device' : DeviceInput,
  'new_auth' : Principal,
  'expires_at' : bigint,
}
export type Result = { 'Ok' : ExecutionResult } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : null } |
  { 'Err' : Error };
export type Result_10 = { 'Ok' : bigint } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : Uint8Array | number[] } |
  { 'Err' : Error };
export type Result_3 = {
    'Ok' : [SecuritySnapshot, Array<[Uint8Array | number[], Device]>]
  } |
  { 'Err' : Error };
export type Result_4 = { 'Ok' : AuthorizedExecution } |
  { 'Err' : Error };
export type Result_5 = { 'Ok' : OperationReceipt } |
  { 'Err' : Error };
export type Result_6 = { 'Ok' : [] | [PendingRecovery] } |
  { 'Err' : Error };
export type Result_7 = { 'Ok' : [] | [ContentRootRef] } |
  { 'Err' : Error };
export type Result_8 = { 'Ok' : Subject } |
  { 'Err' : Error };
export type Result_9 = { 'Ok' : CertifiedBatch } |
  { 'Err' : Error };
export interface RootReservation {
  'op_id' : Uint8Array | number[],
  'generation' : bigint,
  'security_epoch' : bigint,
  'expected_generation' : bigint,
  'expires_at' : bigint,
}
export interface SecuritySnapshot {
  'content_root_generation' : bigint,
  'account_version' : bigint,
  'recovery_nonce' : bigint,
  'content_root_digest' : [] | [Uint8Array | number[]],
  'devices_root' : Uint8Array | number[],
  'recovery_hpke_pub' : [] | [Uint8Array | number[]],
  'schema' : number,
  'security_epoch' : bigint,
  'recovery_root_version' : bigint,
  'home_user' : Principal,
  'subject_id' : Uint8Array | number[],
  'recovery_delay_ns' : [] | [bigint],
  'recovery_signing_pub' : [] | [Uint8Array | number[]],
  'vault_write_state' : VaultWriteState,
  'account_status' : AccountStatus,
  'pending_recovery_digest' : [] | [Uint8Array | number[]],
}
export interface SensitivePolicy {
  'daily_cycles' : bigint,
  'daily_executions' : number,
  'frozen' : boolean,
  'allowed_purposes' : Array<KeyPurpose>,
}
export interface SignedOffer {
  'signature' : Uint8Array | number[],
  'offer' : PaymentOffer,
}
export interface Subject {
  'status' : AccountStatus,
  'account_version' : bigint,
  'auth_bindings' : Array<Principal>,
  'recovery_nonce' : bigint,
  'root_slot' : [] | [RootReservation],
  'executions' : Array<[Uint8Array | number[], AuthorizedExecution]>,
  'security_epoch' : bigint,
  'pending_recovery' : [] | [PendingRecovery],
  'home_cose' : Principal,
  'home_user' : Principal,
  'subject_id' : Uint8Array | number[],
  'operations' : Array<OperationReceipt>,
  'next_execution_sequence' : bigint,
  'handle_authorizations' : Array<[Uint8Array | number[], HandleAuthorization]>,
  'sensitive_policy' : SensitivePolicy,
  'recovery' : [] | [RecoveryPolicy],
  'current_root' : [] | [ContentRootRef],
  'next_root_generation' : bigint,
  'recovery_checked' : boolean,
  'budget' : Budget,
  'devices' : Array<[Uint8Array | number[], Device]>,
  'vault_write_state' : VaultWriteState,
}
export interface UserInit {
  'handle_canister' : Principal,
  'daily_new_subjects' : number,
  'max_subjects' : bigint,
  'home_cose' : Principal,
  'payment_canister' : Principal,
}
export type VaultWriteState = { 'RekeyRequired' : null } |
  { 'Ready' : null } |
  { 'Uninitialized' : null };
export interface _SERVICE {
  'authorize_and_execute' : ActorMethod<[ExecuteRequest], Result>,
  'begin_auth_binding' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[], bigint],
    Result_1
  >,
  'complete_recovery' : ActorMethod<[Uint8Array | number[]], Result_1>,
  'consume_handle_authorization' : ActorMethod<[HandleIntent], Result_1>,
  'create_subject' : ActorMethod<[CreateSubject], Result_2>,
  'get_device_bundle' : ActorMethod<[Uint8Array | number[]], Result_3>,
  'get_execution' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_4
  >,
  'get_operation' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_5
  >,
  'get_recovery_request' : ActorMethod<[Uint8Array | number[]], Result_6>,
  'get_root_ref' : ActorMethod<[Uint8Array | number[]], Result_7>,
  'get_subject' : ActorMethod<[Uint8Array | number[]], Result_8>,
  'mutate_account' : ActorMethod<[AccountMutation], Result_5>,
  'my_subject' : ActorMethod<[], [] | [Uint8Array | number[]]>,
  'prune_auth_bindings' : ActorMethod<
    [Uint8Array | number[]],
    [] | [Uint8Array | number[]]
  >,
  'reconcile_execution' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result
  >,
  'reconfirm_recovery' : ActorMethod<
    [Uint8Array | number[], RecoveryConfirmation, Uint8Array | number[]],
    Result_1
  >,
  'request_recovery' : ActorMethod<
    [
      Uint8Array | number[],
      RecoveryRequest,
      Uint8Array | number[],
      Uint8Array | number[],
    ],
    Result_1
  >,
  'security_snapshot_batch' : ActorMethod<
    [Array<Uint8Array | number[]>],
    Result_9
  >,
  'verify_payment_offer' : ActorMethod<[SignedOffer], Result_10>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
