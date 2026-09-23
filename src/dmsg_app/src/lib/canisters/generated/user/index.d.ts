// Generated from the public dmsg_user.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export interface Account {
  'owner' : Principal,
  'subaccount' : [] | [Uint8Array | number[]],
}
export type AccountCommand = {
    'SetDeviceCapabilities' : {
      'capabilities' : Array<Capability>,
      'device_id' : Uint8Array | number[],
    }
  } |
  {
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
export interface AccountInfo {
  'account_id' : Uint8Array | number[],
  'status' : AccountStatus,
  'account_version' : bigint,
  'auth_bindings' : Array<Principal>,
  'recovery_nonce' : bigint,
  'root_slot' : [] | [RootReservation],
  'security_epoch' : bigint,
  'created_at_ms' : bigint,
  'pending_recovery' : [] | [PendingRecovery],
  'issuer' : string,
  'home_cose' : Principal,
  'home_user' : Principal,
  'sensitive_policy' : SensitivePolicy,
  'recovery' : [] | [RecoveryPolicy],
  'current_root' : [] | [ContentRootRef],
  'recovery_checked' : boolean,
  'devices' : Array<[Uint8Array | number[], Device]>,
  'vault_write_state' : VaultWriteState,
}
export interface AccountMutation {
  'account_id' : Uint8Array | number[],
  'command' : AccountCommand,
  'approval' : Approval,
  'expected_version' : bigint,
}
export type AccountStatus = { 'Active' : null } |
  { 'RecoveryDisputed' : null };
export interface ActionArtifact {
  'uri' : string,
  'sha256' : Uint8Array | number[],
  'size' : bigint,
  'content_type' : string,
}
export interface ActionFile {
  'sha256' : Uint8Array | number[],
  'media_type' : string,
  'byte_length' : bigint,
  'display_name' : [] | [string],
  'revision' : bigint,
  'representation' : ActionFileRepresentation,
  'file_id' : string,
}
export type ActionFileRepresentation = { 'Encrypted' : null } |
  { 'Original' : null };
export interface ActionRequestedChange {
  'blocking' : boolean,
  'locator' : string,
  'detail' : string,
}
export type ActionReviewOutcome = { 'Approved' : null } |
  { 'Rejected' : null } |
  { 'ChangesRequested' : null };
export type Algorithm = { 'VetKdBls12381' : null } |
  { 'Ed25519' : null } |
  { 'EcdsaSecp256k1' : null };
export interface AppAction {
  'files' : Array<ActionFile>,
  'rule_set_hash' : Uint8Array | number[],
  'app_config_version' : bigint,
  'origin' : string,
  'signing_account' : Uint8Array | number[],
  'actor_id' : Uint8Array | number[],
  'operation_id' : Uint8Array | number[],
  'subject_hash' : Uint8Array | number[],
  'version' : number,
  'command' : AppActionCommand,
  'app_id' : string,
  'issued_at_ms' : bigint,
  'environment' : Environment,
  'precondition_hash' : Uint8Array | number[],
  'intent_hash' : Uint8Array | number[],
  'receiver' : Principal,
  'input_hash' : Uint8Array | number[],
  'expires_at_ms' : bigint,
  'role_snapshot_hash' : Uint8Array | number[],
  'signing_policy_hash' : Uint8Array | number[],
}
export type AppActionCommand = {
    'TokenListCertifyTransition' : {
      'transition_id' : bigint,
      'rationale' : string,
      'project_id' : bigint,
      'analysis' : [] | [ActionArtifact],
      'statement_hash' : Uint8Array | number[],
    }
  } |
  {
    'TokenListCertifyDisclosure' : {
      'contract_id' : bigint,
      'project_id' : bigint,
      'revision' : bigint,
    }
  } |
  {
    'TokenListApproveTransition' : {
      'approve' : boolean,
      'transition_id' : bigint,
      'rationale' : string,
      'project_id' : bigint,
      'statement_hash' : Uint8Array | number[],
    }
  } |
  {
    'TokenListDecideReview' : {
      'case_id' : bigint,
      'rationale' : string,
      'project_id' : bigint,
      'changes' : Array<ActionRequestedChange>,
      'outcome' : ActionReviewOutcome,
      'round' : bigint,
    }
  };
export interface AppActionSignRequest {
  'key' : SigningKeyRef,
  'account_id' : Uint8Array | number[],
  'action' : AppAction,
  'issuer' : string,
  'max_cycles' : bigint,
  'approval' : Approval,
}
export interface ApplicationApproval {
  'service' : Principal,
  'actor' : Principal,
  'beneficiary' : Beneficiary,
  'app_config_version' : bigint,
  'origin' : string,
  'action_digest' : Uint8Array | number[],
  'operation_id' : Uint8Array | number[],
  'approving_account' : Uint8Array | number[],
  'version' : number,
  'app_id' : string,
  'nonce' : Uint8Array | number[],
  'environment' : Environment,
  'purpose' : ApprovalPurpose,
  'expires_at_ms' : bigint,
}
export interface ApplicationAuthorization {
  'approval_id' : Uint8Array | number[],
  'valid_until_ms' : bigint,
  'security_epoch' : bigint,
  'verified_at_ms' : bigint,
  'approval_hash' : Uint8Array | number[],
}
export interface Approval {
  'request_id' : Uint8Array | number[],
  'signature' : Uint8Array | number[],
  'device_id' : Uint8Array | number[],
  'security_epoch' : bigint,
  'expires_at' : bigint,
  'sequence' : bigint,
}
export type ApprovalPurpose = { 'AppAction' : null } |
  { 'CashCheckout' : null } |
  { 'PandaSubscription' : null };
export type AuthenticationPurpose = { 'Reauthenticate' : null } |
  { 'Login' : null } |
  { 'Link' : null };
export interface AuthenticationRequest {
  'app_config_version' : bigint,
  'origin' : string,
  'operation_id' : Uint8Array | number[],
  'session_key_hash' : Uint8Array | number[],
  'version' : number,
  'app_id' : string,
  'issued_at_ms' : bigint,
  'nonce' : Uint8Array | number[],
  'environment' : Environment,
  'receiver' : Principal,
  'purpose' : AuthenticationPurpose,
  'expires_at_ms' : bigint,
  'challenge_hash' : Uint8Array | number[],
}
export interface AuthenticationResult {
  'account_id' : Uint8Array | number[],
  'request' : AuthenticationRequest,
  'device_id' : Uint8Array | number[],
  'security_epoch' : bigint,
  'version' : number,
  'home_user' : Principal,
  'expires_at_ms' : bigint,
  'approved_at_ms' : bigint,
}
export interface Beneficiary {
  'product_id' : string,
  'authority_canister' : Principal,
  'subject_bytes' : Uint8Array | number[],
  'subject_schema' : string,
}
export interface BillingOffer {
  'sku' : string,
  'product_id' : string,
  'beneficiary' : Beneficiary,
  'amount_usd_micros' : bigint,
  'operation_id' : Uint8Array | number[],
  'starts_at_ms' : bigint,
  'version' : number,
  'app_id' : string,
  'issued_at_ms' : bigint,
  'offer_id' : Uint8Array | number[],
  'quote_authority' : Principal,
  'environment' : Environment,
  'expected_business_revision' : bigint,
  'adapter' : Principal,
  'allowed_settlement_methods' : Array<SettlementMethod>,
  'expires_at_ms' : bigint,
  'accept_by_ms' : bigint,
  'product_terms_hash' : Uint8Array | number[],
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
export interface CreateAccount {
  'op_id' : Uint8Array | number[],
  'device' : DeviceInput,
  'proof' : Uint8Array | number[],
  'expires_at' : bigint,
}
export interface DeriveRootRequest {
  'account_id' : Uint8Array | number[],
  'target' : RootTarget,
  'max_cycles' : bigint,
  'approval' : Approval,
  'transport_public_key' : Uint8Array | number[],
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
  'cycles_cost_upper_bound' : bigint,
  'outcome' : ExecutionOutcome,
}
export interface ExecutionUsage {
  'account_id' : Uint8Array | number[],
  'business_revision' : bigint,
  'valid_until_ms' : bigint,
  'held_units' : bigint,
  'lease_revision' : bigint,
  'allowed_units' : bigint,
  'charged_units' : bigint,
  'weight_policy_version' : bigint,
  'month_revision' : bigint,
  'month_utc' : number,
}
export type HandleAction = { 'AcceptTransfer' : null } |
  { 'Register' : null } |
  { 'Transfer' : null } |
  { 'ClaimLegacy' : null };
export interface HandleIntent {
  'account_id' : Uint8Array | number[],
  'handle_canister' : Principal,
  'target_account' : [] | [Uint8Array | number[]],
  'action' : HandleAction,
  'op_id' : Uint8Array | number[],
  'handle' : string,
  'expected_version' : bigint,
  'terms_digest' : Uint8Array | number[],
}
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
  { 'Statement' : null };
export interface OperationReceipt {
  'id' : Uint8Array | number[],
  'account_version' : bigint,
  'digest' : Uint8Array | number[],
}
export interface PaymentOffer {
  'account_id' : Uint8Array | number[],
  'quote_scope' : Uint8Array | number[],
  'issued_at' : bigint,
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
export interface ProductApproval {
  'method' : SettlementMethod,
  'approval_id' : Uint8Array | number[],
  'offer_hash' : Uint8Array | number[],
  'operator' : Principal,
  'version' : number,
  'expires_at_ms' : bigint,
  'approved_at_ms' : bigint,
}
export interface ProductAuthorization {
  'request_hash' : Uint8Array | number[],
  'operator' : Principal,
  'valid_until_ms' : bigint,
  'verified_at_ms' : bigint,
}
export interface ProductAuthorizationRequest {
  'product_approval' : [] | [ProductApproval],
  'account_approval' : ApplicationApproval,
  'approval_id' : Uint8Array | number[],
  'user_home' : Principal,
  'offer' : BillingOffer,
}
export interface RecoveryConfirmation {
  'request_id' : Uint8Array | number[],
  'dispute' : Uint8Array | number[],
  'expires_at' : bigint,
}
export interface RecoveryPolicy {
  'delay_ms' : bigint,
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
export type Result = { 'Ok' : Uint8Array | number[] } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : AuthenticationResult } |
  { 'Err' : Error };
export type Result_10 = { 'Ok' : OperationReceipt } |
  { 'Err' : Error };
export type Result_11 = { 'Ok' : [] | [PendingRecovery] } |
  { 'Err' : Error };
export type Result_12 = { 'Ok' : [] | [ContentRootRef] } |
  { 'Err' : Error };
export type Result_13 = { 'Ok' : ApplicationAuthorization } |
  { 'Err' : Error };
export type Result_14 = { 'Ok' : bigint } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : CertifiedBatch } |
  { 'Err' : Error };
export type Result_3 = { 'Ok' : ProductAuthorization } |
  { 'Err' : Error };
export type Result_4 = { 'Ok' : null } |
  { 'Err' : Error };
export type Result_5 = { 'Ok' : Uint8Array | number[] } |
  { 'Err' : Error };
export type Result_6 = { 'Ok' : ExecutionResult } |
  { 'Err' : Error };
export type Result_7 = { 'Ok' : AccountInfo } |
  { 'Err' : Error };
export type Result_8 = {
    'Ok' : [SecuritySnapshot, Array<[Uint8Array | number[], Device]>]
  } |
  { 'Err' : Error };
export type Result_9 = { 'Ok' : ExecutionUsage } |
  { 'Err' : Error };
export interface RootReservation {
  'op_id' : Uint8Array | number[],
  'generation' : bigint,
  'security_epoch' : bigint,
  'expected_generation' : bigint,
  'expires_at' : bigint,
}
export type RootTarget = {
    'Candidate' : { 'op_id' : Uint8Array | number[], 'generation' : bigint }
  } |
  { 'Current' : { 'generation' : bigint } };
export interface SecuritySnapshot {
  'content_root_generation' : bigint,
  'account_id' : Uint8Array | number[],
  'account_version' : bigint,
  'recovery_nonce' : bigint,
  'content_root_digest' : [] | [Uint8Array | number[]],
  'devices_root' : Uint8Array | number[],
  'recovery_hpke_pub' : [] | [Uint8Array | number[]],
  'schema' : number,
  'security_epoch' : bigint,
  'recovery_root_version' : bigint,
  'issuer' : string,
  'home_cose' : Principal,
  'home_user' : Principal,
  'recovery_delay_ms' : [] | [bigint],
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
export type SettlementMethod = { 'Cash' : null } |
  { 'Panda' : null };
export interface SignRequest {
  'key' : SigningKeyRef,
  'account_id' : Uint8Array | number[],
  'statement' : Statement,
  'origin' : string,
  'max_cycles' : bigint,
  'approval' : Approval,
}
export interface SignedArtifact {
  'cose_sign1' : Uint8Array | number[],
  'cose_key' : Uint8Array | number[],
}
export interface SignedOffer {
  'signature' : Uint8Array | number[],
  'offer' : PaymentOffer,
}
export type SigningAlgorithm = { 'Ed25519' : null } |
  { 'EcdsaSecp256k1' : null };
export interface SigningKeyRef {
  'kid' : Uint8Array | number[],
  'algorithm' : SigningAlgorithm,
  'public_key_fingerprint' : Uint8Array | number[],
}
export interface Statement {
  'issued_at' : [] | [bigint],
  'content' : StatementContent,
  'subject' : [] | [string],
  'issuer' : string,
}
export type StatementContent = { 'AppAction' : AppAction } |
  {
    'FileStatement' : {
      'sha256' : Uint8Array | number[],
      'text' : string,
      'content_type' : [] | [string],
      'location' : [] | [string],
    }
  } |
  { 'Text' : string } |
  {
    'Digest' : {
      'sha256' : Uint8Array | number[],
      'content_type' : [] | [string],
      'location' : [] | [string],
    }
  };
export interface UserInit {
  'handle_canister' : Principal,
  'home_cose' : Principal,
  'issuer_namespace' : string,
  'daily_new_accounts' : number,
  'payment_canister' : Principal,
  'environment' : Environment,
  'commerce_canister' : Principal,
  'max_accounts' : bigint,
  'membership_canister' : Principal,
}
export type VaultWriteState = { 'RekeyRequired' : null } |
  { 'Ready' : null } |
  { 'Uninitialized' : null };
export interface _SERVICE {
  'approve_application' : ActorMethod<[ApplicationApproval, Approval], Result>,
  'approve_authentication' : ActorMethod<
    [Uint8Array | number[], AuthenticationRequest, Approval],
    Result_1
  >,
  'authentication_certificate' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_2
  >,
  'authorize_product_billing' : ActorMethod<
    [ProductAuthorizationRequest],
    Result_3
  >,
  'begin_auth_binding' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[], bigint],
    Result_4
  >,
  'complete_recovery' : ActorMethod<[Uint8Array | number[]], Result_4>,
  'consume_handle_authorization' : ActorMethod<[HandleIntent], Result_4>,
  'consume_handle_transfer_authorizations' : ActorMethod<
    [HandleIntent, HandleIntent],
    Result_4
  >,
  'create_account' : ActorMethod<[CreateAccount], Result_5>,
  'derive_root' : ActorMethod<[DeriveRootRequest], Result_6>,
  'get_account' : ActorMethod<[Uint8Array | number[]], Result_7>,
  'get_device_bundle' : ActorMethod<[Uint8Array | number[]], Result_8>,
  'get_execution' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_6
  >,
  'get_execution_receipt' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_2
  >,
  'get_execution_usage' : ActorMethod<
    [Uint8Array | number[], number],
    Result_9
  >,
  'get_execution_usage_certified' : ActorMethod<
    [Uint8Array | number[], number],
    Result_2
  >,
  'get_operation' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_10
  >,
  'get_recovery_request' : ActorMethod<[Uint8Array | number[]], Result_11>,
  'get_root_ref' : ActorMethod<[Uint8Array | number[]], Result_12>,
  'inspect_app_action' : ActorMethod<
    [Uint8Array | number[], AppAction],
    Result_4
  >,
  'mutate_account' : ActorMethod<[AccountMutation], Result_10>,
  'my_account' : ActorMethod<[], [] | [Uint8Array | number[]]>,
  'prune_auth_bindings' : ActorMethod<
    [Uint8Array | number[]],
    [] | [Uint8Array | number[]]
  >,
  'prune_external_approvals' : ActorMethod<
    [Uint8Array | number[]],
    [] | [Uint8Array | number[]]
  >,
  'reconcile_execution' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[]],
    Result_6
  >,
  'reconfirm_recovery' : ActorMethod<
    [Uint8Array | number[], RecoveryConfirmation, Uint8Array | number[]],
    Result_4
  >,
  'refresh_execution_entitlement' : ActorMethod<
    [Uint8Array | number[]],
    Result_9
  >,
  'request_recovery' : ActorMethod<
    [
      Uint8Array | number[],
      RecoveryRequest,
      Uint8Array | number[],
      Uint8Array | number[],
    ],
    Result_4
  >,
  'security_snapshot_batch' : ActorMethod<
    [Array<Uint8Array | number[]>],
    Result_2
  >,
  'sign' : ActorMethod<[SignRequest], Result_6>,
  'sign_app_action' : ActorMethod<[AppActionSignRequest], Result_6>,
  'verify_application_authorization' : ActorMethod<
    [Uint8Array | number[], ApplicationApproval],
    Result_13
  >,
  'verify_payment_offer' : ActorMethod<[SignedOffer], Result_14>,
  'verify_product_account' : ActorMethod<[string, Beneficiary], Result_4>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
