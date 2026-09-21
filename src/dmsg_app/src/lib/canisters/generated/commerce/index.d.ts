// Generated from the public dmsg_commerce.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export interface Account {
  'owner' : Principal,
  'subaccount' : [] | [Uint8Array | number[]],
}
export interface Beneficiary {
  'product_id' : string,
  'authority_canister' : Principal,
  'subject_bytes' : Uint8Array | number[],
  'subject_schema' : string,
}
export interface BillingOrder {
  'status' : OrderStatus,
  'fee_reserve' : bigint,
  'service_reserve' : bigint,
  'generation' : bigint,
  'network_fees' : bigint,
  'activated_contract_id' : [] | [Uint8Array | number[]],
  'transferred' : bigint,
  'refundable' : bigint,
  'earned' : bigint,
  'next_transfer' : bigint,
  'receive_subaccount' : Uint8Array | number[],
  'input' : OpenOrder,
  'order_id' : Uint8Array | number[],
  'outgoing' : bigint,
  'funding_block' : [] | [bigint],
  'close_effective_at_ms' : [] | [bigint],
  'refunded_principal' : bigint,
  'busy_until_ms' : bigint,
  'confirmed_in' : bigint,
}
export interface Catalog {
  'decimals' : number,
  'storage_products' : Array<StorageProduct>,
  'schema' : number,
  'effective_at_ms' : bigint,
  'version' : bigint,
  'ledger' : Principal,
  'ledger_fee' : bigint,
  'plans' : Array<PlanVersion>,
  'terms_digest' : Uint8Array | number[],
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
export interface CommerceInit {
  'daily_orders' : number,
  'max_subjects' : bigint,
  'environment' : Environment,
  'governance' : Principal,
  'membership_canister' : Principal,
  'user_homes' : Array<Principal>,
  'catalog' : Catalog,
  'treasury' : Account,
}
export type DecisionKind = { 'Apply' : null } |
  { 'Close' : null };
export type DecisionOutcome = { 'Applied' : null } |
  { 'Rejected' : null };
export type Eligibility = { 'Unverifiable' : null } |
  { 'Ineligible' : null } |
  { 'Eligible' : null };
export interface EntitlementView {
  'lease_source_contract_ids' : Array<Uint8Array | number[]>,
  'plan_snapshot' : PlanVersion,
  'business_revision' : bigint,
  'service_terminated_at_ms' : [] | [bigint],
  'beneficiary' : Beneficiary,
  'schema' : number,
  'valid_until_ms' : bigint,
  'observed_at_ms' : bigint,
  'repair_deadline_ms' : [] | [bigint],
  'lease_revision' : bigint,
  'issued_at_ms' : bigint,
  'home_commerce' : Principal,
  'addons' : Array<StorageAddon>,
  'effective_stop_at_ms' : [] | [bigint],
  'effective_limits' : ResourceLimits,
  'next_limit_change_at_ms' : [] | [bigint],
  'source_status' : SourceStatus,
  'eligibility_status' : Eligibility,
  'active_contract_id' : [] | [Uint8Array | number[]],
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
export interface ExecutionEntitlement {
  'month' : MonthEntitlement,
  'view' : EntitlementView,
}
export interface ExecutionWeights {
  'ecdsa_secp256k1' : bigint,
  'ed25519' : bigint,
  'version' : bigint,
}
export interface MembershipAuthorization {
  'valid_until_ms' : bigint,
  'security_epoch' : bigint,
  'verified_at_ms' : bigint,
  'intent_digest' : Uint8Array | number[],
}
export interface MembershipDecision {
  'qualification_until_ms' : bigint,
  'decision_id' : Uint8Array | number[],
  'claim_id' : Uint8Array | number[],
  'request' : ClaimRequest,
  'kind' : DecisionKind,
  'observed_at_ms' : bigint,
  'required_atomic' : bigint,
  'starts_at_ms' : bigint,
  'apply_by_ms' : bigint,
  'expires_at_ms' : bigint,
  'policy' : MembershipPolicy,
}
export interface MembershipDecisionReceipt {
  'decision_id' : Uint8Array | number[],
  'business_revision' : bigint,
  'commitment_until_ms' : bigint,
  'contract_id' : [] | [Uint8Array | number[]],
  'starts_at_ms' : bigint,
  'outcome' : DecisionOutcome,
  'decision_digest' : Uint8Array | number[],
  'expires_at_ms' : bigint,
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
export interface MerchantDeposit {
  'from' : Account,
  'refundable' : bigint,
  'block' : bigint,
  'order_id' : Uint8Array | number[],
  'amount' : bigint,
}
export interface MerchantTransfer {
  'to' : Account,
  'fee' : bigint,
  'status' : MerchantTransferStatus,
  'replaces' : [] | [bigint],
  'created_at_time_ns' : bigint,
  'memo' : Uint8Array | number[],
  'replaced_by' : [] | [bigint],
  'transfer_id' : bigint,
  'block' : [] | [bigint],
  'order_id' : Uint8Array | number[],
  'amount' : bigint,
}
export type MerchantTransferStatus = { 'Superseded' : null } |
  { 'Rejected' : null } |
  { 'Succeeded' : null } |
  { 'InFlight' : null } |
  { 'Unknown' : null } |
  { 'Pending' : null };
export interface MonthEntitlement {
  'business_revision' : bigint,
  'calculation_version' : number,
  'beneficiary' : Beneficiary,
  'segments' : Array<MonthSegment>,
  'allowed_units' : bigint,
  'weights' : ExecutionWeights,
  'month_revision' : bigint,
  'month_utc' : number,
}
export interface MonthSegment {
  'start_ms' : bigint,
  'monthly_units' : bigint,
  'end_ms' : bigint,
  'source_contract_id' : [] | [Uint8Array | number[]],
}
export interface OpenOrder {
  'quote' : OrderQuote,
  'authorization' : MembershipIntent,
}
export type OrderAction = {
    'Buyout' : { 'contract_id' : Uint8Array | number[] }
  } |
  { 'Storage' : { 'product_id' : Uint8Array | number[] } } |
  { 'Upgrade' : { 'plan' : PlanId } } |
  { 'Renew' : { 'plan' : PlanId } } |
  { 'Subscribe' : { 'plan' : PlanId } };
export interface OrderQuote {
  'amount_atomic' : bigint,
  'fee_reserve' : bigint,
  'request' : QuoteOrder,
  'term' : TermRule,
  'created_at_ms' : bigint,
  'fund_by_ms' : bigint,
  'home_commerce' : Principal,
  'catalog' : Catalog,
  'activate_by_ms' : bigint,
}
export type OrderStatus = { 'Closing' : null } |
  { 'Active' : null } |
  { 'AwaitingFunding' : null } |
  { 'Cancelled' : null } |
  { 'Authorizing' : null } |
  { 'RefundCommitted' : null };
export type PlanId = { 'Max' : null } |
  { 'Pro' : null } |
  { 'Free' : null } |
  { 'Plus' : null };
export interface PlanVersion {
  'catalog_version' : bigint,
  'terms_version' : bigint,
  'price_cents' : bigint,
  'membership_policy_version' : [] | [bigint],
  'weights' : ExecutionWeights,
  'plan_id' : PlanId,
  'limits' : ResourceLimits,
}
export interface QuoteOrder {
  'action' : OrderAction,
  'op_id' : Uint8Array | number[],
  'beneficiary' : Beneficiary,
  'payer' : Account,
  'expected_business_revision' : bigint,
}
export interface ResourceLimits {
  'active_channels' : bigint,
  'monthly_execution_units' : bigint,
  'storage_bytes' : bigint,
}
export type Result = { 'Ok' : MembershipDecisionReceipt } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : MembershipAuthorization } |
  { 'Err' : Error };
export type Result_10 = { 'Ok' : ClaimView } |
  { 'Err' : Error };
export type Result_11 = { 'Ok' : null } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : BillingOrder } |
  { 'Err' : Error };
export type Result_3 = { 'Ok' : MerchantTransfer } |
  { 'Err' : Error };
export type Result_4 = { 'Ok' : CertifiedBatch } |
  { 'Err' : Error };
export type Result_5 = { 'Ok' : MerchantDeposit } |
  { 'Err' : Error };
export type Result_6 = { 'Ok' : ExecutionEntitlement } |
  { 'Err' : Error };
export type Result_7 = { 'Ok' : [] | [MembershipDecisionReceipt] } |
  { 'Err' : Error };
export type Result_8 = { 'Ok' : OrderQuote } |
  { 'Err' : Error };
export type Result_9 = { 'Ok' : EntitlementView } |
  { 'Err' : Error };
export type SourceStatus = { 'Unverifiable' : null } |
  { 'Free' : null } |
  { 'Closing' : null } |
  { 'Active' : null } |
  { 'Suspended' : null } |
  { 'RepairRequired' : null } |
  { 'Expired' : null };
export interface StorageAddon {
  'contract_id' : Uint8Array | number[],
  'starts_at_ms' : bigint,
  'storage_bytes' : bigint,
  'order_id' : Uint8Array | number[],
  'expires_at_ms' : bigint,
  'last_issued_until_ms' : bigint,
}
export interface StorageProduct {
  'product_id' : Uint8Array | number[],
  'price_cents' : bigint,
  'storage_bytes' : bigint,
  'duration_ms' : bigint,
}
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
  'apply_membership_decision' : ActorMethod<[MembershipDecision], Result>,
  'authorize_membership_close' : ActorMethod<
    [Uint8Array | number[], MembershipIntent],
    Result_1
  >,
  'authorize_membership_intent' : ActorMethod<[ClaimRequest], Result_1>,
  'check_order_funding' : ActorMethod<
    [Uint8Array | number[], bigint],
    Result_2
  >,
  'claim_deposit_refund' : ActorMethod<
    [Uint8Array | number[], bigint],
    Result_3
  >,
  'claim_fee_reserve' : ActorMethod<[Uint8Array | number[]], Result_3>,
  'collect_revenue' : ActorMethod<[Uint8Array | number[]], Result_3>,
  'get_catalog' : ActorMethod<[], Result_4>,
  'get_deposit' : ActorMethod<[bigint], Result_5>,
  'get_entitlement_batch' : ActorMethod<[Array<Beneficiary>], Result_4>,
  'get_execution_entitlement' : ActorMethod<
    [Beneficiary, number, bigint],
    Result_6
  >,
  'get_membership_decision' : ActorMethod<[Uint8Array | number[]], Result_7>,
  'get_operation' : ActorMethod<[Uint8Array | number[]], Result_2>,
  'get_order_certified' : ActorMethod<[Uint8Array | number[]], Result_4>,
  'get_transfer' : ActorMethod<[Uint8Array | number[], bigint], Result_3>,
  'list_catalogs' : ActorMethod<[], Array<Catalog>>,
  'open_order' : ActorMethod<[OpenOrder], Result_2>,
  'process_transfer' : ActorMethod<[Uint8Array | number[], bigint], Result_3>,
  'quote_order' : ActorMethod<[QuoteOrder], Result_8>,
  'reconcile_order' : ActorMethod<[Uint8Array | number[]], Result_2>,
  'reconcile_transfer' : ActorMethod<
    [Uint8Array | number[], bigint, bigint],
    Result_3
  >,
  'refresh_catalog' : ActorMethod<[], Catalog>,
  'refresh_entitlement' : ActorMethod<[Beneficiary], Result_9>,
  'release_replaced_claim' : ActorMethod<
    [Beneficiary, Uint8Array | number[]],
    Result_10
  >,
  'request_refund' : ActorMethod<
    [Uint8Array | number[], MembershipIntent],
    Result_2
  >,
  'revise_rejected_transfer' : ActorMethod<
    [Uint8Array | number[], bigint, bigint],
    Result_3
  >,
  'schedule_policy' : ActorMethod<[Catalog], Result_11>,
  'set_admission_pause' : ActorMethod<[boolean], Result_11>,
  'verify_ledger_configuration' : ActorMethod<[], Result_11>,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
