// Generated from the public membership.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export interface Beneficiary {
  'product_id' : string,
  'authority_canister' : Principal,
  'subject_bytes' : Uint8Array | number[],
  'subject_schema' : string,
}
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
export type ClaimChange = { 'Start' : null } |
  { 'Upgrade' : { 'previous_claim' : Uint8Array | number[] } } |
  { 'Replace' : { 'previous_claim' : Uint8Array | number[] } } |
  { 'Renew' : { 'previous_claim' : Uint8Array | number[] } };
export interface ClaimRequest {
  'term' : TermRule,
  'benefit_id' : Uint8Array | number[],
  'change' : ClaimChange,
  'policy_version' : bigint,
  'expected_business_revision' : bigint,
  'authorization' : MembershipIntent,
  'neuron_id' : Uint8Array | number[],
}
export type ClaimStatus = { 'CoolingDown' : null } |
  { 'Closing' : null } |
  { 'Active' : null } |
  { 'Released' : null } |
  { 'Rejected' : null } |
  { 'Checking' : null } |
  { 'Applying' : null };
export interface ClaimView {
  'status' : ClaimStatus,
  'home_membership' : Principal,
  'decision_id' : [] | [Uint8Array | number[]],
  'release_after_ms' : bigint,
  'claim_id' : Uint8Array | number[],
  'beneficiary' : Beneficiary,
  'schema' : number,
  'valid_until_ms' : bigint,
  'observed_at_ms' : bigint,
  'benefit_id' : Uint8Array | number[],
  'lease_revision' : bigint,
  'starts_at_ms' : bigint,
  'eligibility' : Eligibility,
  'policy_version' : bigint,
  'expires_at_ms' : bigint,
}
export type Eligibility = { 'Unverifiable' : null } |
  { 'Ineligible' : null } |
  { 'Eligible' : null };
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
export interface MembershipInit {
  'max_claims' : bigint,
  'hourly_applications' : number,
  'panda_ledger' : Principal,
  'sns_root' : Principal,
  'environment' : Environment,
  'cooling_ms' : bigint,
  'governance' : Principal,
  'products' : Array<ProductConfig>,
  'subsidy_budget' : bigint,
  'policies' : Array<MembershipPolicy>,
}
export interface MembershipIntent {
  'actor' : Principal,
  'beneficiary' : Beneficiary,
  'valid_until_ms' : bigint,
  'action_digest' : Uint8Array | number[],
  'application_id' : Uint8Array | number[],
  'nonce' : Uint8Array | number[],
  'service_canister' : Principal,
  'environment' : Environment,
}
export interface MembershipPolicy {
  'product_id' : string,
  'threshold' : Threshold,
  'subsidy_units' : bigint,
  'benefit_id' : Uint8Array | number[],
  'effective_at_ms' : bigint,
  'version' : bigint,
}
export interface ProductConfig {
  'product_id' : string,
  'subject_size' : number,
  'adapter' : Principal,
  'authorities' : Array<Principal>,
  'subject_schema' : string,
}
export type Result = { 'Ok' : ClaimView } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : CertifiedBatch } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : MembershipPolicy } |
  { 'Err' : Error };
export type Result_3 = { 'Ok' : null } |
  { 'Err' : Error };
export type TermRule = {
    'Fixed' : { 'starts_at_ms' : bigint, 'expires_at_ms' : bigint }
  } |
  { 'CalendarYear' : null };
export type Threshold = {
    'AnnualPrice' : {
      'price_cents' : bigint,
      'r_den' : bigint,
      'r_num' : bigint,
    }
  } |
  { 'FixedPanda' : { 'atomic' : bigint } };
export interface _SERVICE {
  'advance_application' : ActorMethod<[Uint8Array | number[]], Result>,
  'cancel_application' : ActorMethod<[Uint8Array | number[]], Result>,
  'close_for_consumer' : ActorMethod<[Uint8Array | number[]], Result>,
  'get_claim_certified' : ActorMethod<[Array<Uint8Array | number[]>], Result_1>,
  'get_claim_for_consumer' : ActorMethod<[Uint8Array | number[]], Result>,
  'get_operation' : ActorMethod<[Uint8Array | number[]], Result>,
  'get_policy' : ActorMethod<[bigint], Result_2>,
  'get_policy_certified' : ActorMethod<[BigUint64Array | bigint[]], Result_1>,
  'increase_subsidy_budget' : ActorMethod<[bigint], Result_3>,
  'reconcile_claim' : ActorMethod<[Uint8Array | number[]], Result>,
  'refresh_claim' : ActorMethod<[Uint8Array | number[]], Result>,
  'register_product' : ActorMethod<[ProductConfig], Result_3>,
  'request_change' : ActorMethod<
    [Uint8Array | number[], MembershipIntent],
    Result
  >,
  'request_claim' : ActorMethod<[ClaimRequest], Result>,
  'schedule_policy' : ActorMethod<[MembershipPolicy], Result_3>,
  'set_admission_pause' : ActorMethod<[boolean], Result_3>,
  'verify_sns_configuration' : ActorMethod<[], Result_3>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
