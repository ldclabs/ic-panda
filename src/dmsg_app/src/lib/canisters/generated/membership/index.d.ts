// Generated from the public membership.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

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
export type ApprovalPurpose = { 'CashCheckout' : null } |
  { 'PandaSubscription' : null };
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
export type Eligibility = { 'Unverifiable' : null } |
  { 'Ineligible' : null } |
  { 'Eligible' : null };
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
export interface MembershipInit {
  'expected_governance_module_hash' : [] | [Uint8Array | number[]],
  'panda_ledger' : Principal,
  'sns_root' : Principal,
  'environment' : Environment,
  'governance' : Principal,
}
export interface PandaApplicationTerms {
  'user_home' : Principal,
  'home_membership' : Principal,
  'actor' : Principal,
  'offer' : BillingOffer,
  'quote' : PandaQuote,
  'approving_account' : Uint8Array | number[],
  'sns_governance' : Principal,
  'neuron_id' : Uint8Array | number[],
}
export interface PandaClaimRequest {
  'terms' : PandaApplicationTerms,
  'authorization' : ProductAuthorizationRequest,
}
export type PandaClaimStatus = { 'Terminated' : null } |
  { 'CoolingDown' : null } |
  { 'Active' : null } |
  { 'Released' : null } |
  { 'Rejected' : null } |
  { 'Checking' : null } |
  { 'Cancelled' : null } |
  { 'Applying' : null };
export interface PandaClaimView {
  'status' : PandaClaimStatus,
  'terms' : PandaApplicationTerms,
  'receipt' : [] | [ProductReceipt],
  'claim_id' : Uint8Array | number[],
  'valid_until_ms' : bigint,
  'observed_at_ms' : bigint,
  'lease_revision' : bigint,
  'eligibility' : Eligibility,
  'version' : number,
  'cooling_until_ms' : [] | [bigint],
  'repair_elapsed_ms' : bigint,
  'committed_until_ms' : bigint,
}
export interface PandaOperationsPage {
  'claims' : Array<PandaClaimView>,
  'next' : [] | [Uint8Array | number[]],
}
export interface PandaQuote {
  'required_stake_e8s' : bigint,
  'offer_hash' : Uint8Array | number[],
  'version' : number,
  'committed_until_ms' : bigint,
  'quoted_at_ms' : bigint,
  'application_deadline_ms' : bigint,
  'policy' : PandaRatePolicy,
  'subsidy_usd_micros' : bigint,
}
export interface PandaRatePolicy {
  'product_ids' : Array<string>,
  'effective_at_ms' : bigint,
  'published_at_ms' : bigint,
  'version' : number,
  'environment' : Environment,
  'subsidy_budget_id' : Uint8Array | number[],
  'policy_version' : bigint,
  'r_den' : bigint,
  'r_num' : bigint,
}
export interface PandaServiceConfig {
  'max_claims' : bigint,
  'hourly_applications' : bigint,
  'commerce_canister' : Principal,
  'cooling_ms' : bigint,
}
export interface PandaSubsidyBudget {
  'reserved_usd_micros' : bigint,
  'total_usd_micros' : bigint,
  'budget_id' : Uint8Array | number[],
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
export interface ProductAuthorizationRequest {
  'product_approval' : [] | [ProductApproval],
  'account_approval' : ApplicationApproval,
  'approval_id' : Uint8Array | number[],
  'user_home' : Principal,
  'offer' : BillingOffer,
}
export type ProductOutcome = {
    'Applied' : {
      'business_revision' : bigint,
      'contract_id' : Uint8Array | number[],
      'committed_until_ms' : bigint,
    }
  } |
  { 'Rejected' : { 'reason' : ProductRejection } };
export interface ProductReceipt {
  'decision_hash' : Uint8Array | number[],
  'decision_id' : Uint8Array | number[],
  'applied_at_ms' : bigint,
  'version' : number,
  'outcome' : ProductOutcome,
  'adapter' : Principal,
}
export type ProductRejection = { 'OfferMismatch' : null } |
  { 'IntervalReserved' : null } |
  { 'RevisionConflict' : null } |
  { 'Unauthorized' : null } |
  { 'Expired' : null };
export type Result = { 'Ok' : PandaClaimView } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : null } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : CertifiedBatch } |
  { 'Err' : Error };
export type Result_3 = { 'Ok' : PandaOperationsPage } |
  { 'Err' : Error };
export type Result_4 = { 'Ok' : PandaApplicationTerms } |
  { 'Err' : Error };
export type Result_5 = { 'Ok' : PandaRatePolicy } |
  { 'Err' : Error };
export type Result_6 = { 'Ok' : PandaSubsidyBudget } |
  { 'Err' : Error };
export type Result_7 = { 'Ok' : number } |
  { 'Err' : Error };
export type SettlementMethod = { 'Cash' : null } |
  { 'Panda' : null };
export interface _SERVICE {
  'advance_panda_claim' : ActorMethod<
    [Uint8Array | number[], ProductAuthorizationRequest],
    Result
  >,
  'cancel_panda_application' : ActorMethod<[Uint8Array | number[]], Result>,
  'configure_panda_service' : ActorMethod<[PandaServiceConfig], Result_1>,
  'get_panda_claim' : ActorMethod<[Uint8Array | number[]], Result>,
  'get_panda_claim_for_product' : ActorMethod<[Uint8Array | number[]], Result>,
  'panda_budgets' : ActorMethod<[], Array<PandaSubsidyBudget>>,
  'panda_claim_certificate' : ActorMethod<[Uint8Array | number[]], Result_2>,
  'panda_operations' : ActorMethod<
    [[] | [Uint8Array | number[]], number],
    Result_3
  >,
  'quote_panda_subscription' : ActorMethod<
    [BillingOffer, Principal, Uint8Array | number[], Uint8Array | number[]],
    Result_4
  >,
  'reconcile_panda_claim' : ActorMethod<[Uint8Array | number[]], Result>,
  'refresh_panda_claim' : ActorMethod<[Uint8Array | number[]], Result>,
  'request_panda_claim' : ActorMethod<[PandaClaimRequest], Result>,
  'schedule_panda_rate' : ActorMethod<[PandaRatePolicy], Result_5>,
  'set_admission_pause' : ActorMethod<[boolean], Result_1>,
  'set_panda_subsidy_budget' : ActorMethod<
    [Uint8Array | number[], bigint],
    Result_6
  >,
  'set_sns_governance_module_hash' : ActorMethod<
    [Uint8Array | number[]],
    Result_1
  >,
  'sweep_panda_commitments' : ActorMethod<[], Result_7>,
  'verify_sns_configuration' : ActorMethod<[], Result_1>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
