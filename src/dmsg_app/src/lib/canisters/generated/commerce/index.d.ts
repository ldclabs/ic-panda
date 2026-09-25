// Generated from the public dmsg_commerce.did. Run npm run bindings.
import type { Principal } from '@icp-sdk/core/principal';
import type { ActorMethod } from '@icp-sdk/core/agent';
import type { IDL } from '@icp-sdk/core/candid';

export interface Account {
  'owner' : Principal,
  'subaccount' : [] | [Uint8Array | number[]],
}
export type AppCapability = { 'SignAction' : null } |
  { 'Checkout' : null } |
  { 'SignDocument' : null } |
  { 'Authenticate' : null };
export interface AppRegistration {
  'cose_homes' : Array<Principal>,
  'capabilities' : Array<AppCapability>,
  'origins' : Array<string>,
  'product_ids' : Array<string>,
  'authentication_receiver' : Principal,
  'version' : number,
  'app_id' : string,
  'config_version' : bigint,
  'environment' : Environment,
  'action_authority' : Principal,
  'user_homes' : Array<Principal>,
  'profiles' : Array<SigningProfile>,
  'paused' : boolean,
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
export interface CashBlock { 'block_index' : bigint, 'ledger' : Principal }
export interface CashCancellationReceipt {
  'decision_hash' : Uint8Array | number[],
  'cancelled' : boolean,
  'business_revision' : bigint,
  'contract_id' : Uint8Array | number[],
  'cancelled_at_ms' : bigint,
  'order_id' : Uint8Array | number[],
}
export interface CashQuote {
  'amount_atomic' : bigint,
  'offer_hash' : Uint8Array | number[],
  'fee_reserve_atomic' : bigint,
  'funding_deadline_ms' : bigint,
  'max_network_fee_atomic' : bigint,
  'activation_deadline_ms' : bigint,
  'deposit' : Account,
  'version' : number,
  'ledger' : Principal,
  'payer' : Account,
  'conversion_hash' : Uint8Array | number[],
}
export interface CashTransfer {
  'to' : Account,
  'status' : CashTransferStatus,
  'source_subaccount' : Uint8Array | number[],
  'amount_atomic' : bigint,
  'expected_fee_atomic' : [] | [bigint],
  'replaces' : [] | [Uint8Array | number[]],
  'block_index' : [] | [bigint],
  'created_at_time_ns' : bigint,
  'memo' : Uint8Array | number[],
  'replaced_by' : [] | [Uint8Array | number[]],
  'transfer_id' : Uint8Array | number[],
  'ledger' : Principal,
  'max_fee_atomic' : bigint,
  'fee_atomic' : bigint,
  'order_id' : Uint8Array | number[],
  'error_code' : [] | [string],
}
export interface CashTransferProgress {
  'status' : CashTransferStatus,
  'block_index' : [] | [bigint],
  'transfer_id' : Uint8Array | number[],
}
export type CashTransferStatus = { 'Superseded' : null } |
  { 'Rejected' : null } |
  { 'Succeeded' : null } |
  { 'InFlight' : null } |
  { 'Unknown' : null } |
  { 'Pending' : null };
export interface CashTransfersPage {
  'transfers' : Array<CashTransfer>,
  'next' : [] | [Uint8Array | number[]],
}
export interface Catalog {
  'storage_products' : Array<StorageProduct>,
  'schema' : number,
  'effective_at_ms' : bigint,
  'version' : bigint,
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
export interface CheckoutDeposit {
  'amount_atomic' : bigint,
  'refundable_atomic' : bigint,
  'from' : Account,
  'block' : CashBlock,
  'order_id' : Uint8Array | number[],
}
export interface CheckoutLedgerBalance {
  'fee_reserve_atomic' : bigint,
  'refundable_atomic' : bigint,
  'outgoing_atomic' : bigint,
  'service_reserve_atomic' : bigint,
  'ledger' : Principal,
  'incoming_atomic' : bigint,
}
export interface CheckoutOperationAudit {
  'order' : CheckoutView,
  'balances' : Array<CheckoutLedgerBalance>,
}
export interface CheckoutOperationsPage {
  'orders' : Array<CheckoutOperationAudit>,
  'next' : [] | [Uint8Array | number[]],
}
export interface CheckoutProgress {
  'status' : CheckoutStatus,
  'decision_id' : [] | [Uint8Array | number[]],
  'order_id' : Uint8Array | number[],
}
export interface CheckoutQuote {
  'asset' : SettlementAsset,
  'offer' : BillingOffer,
  'cash' : CashQuote,
  'quoted_at_ms' : bigint,
  'product' : ProductRegistration,
}
export type CheckoutStatus = { 'Applied' : null } |
  { 'Reserving' : null } |
  { 'AwaitingFunding' : null } |
  { 'Rejected' : null } |
  { 'RefundCommitted' : null } |
  { 'Applying' : null };
export interface CheckoutView {
  'receipt' : [] | [ProductReceipt],
  'fee_reserve_atomic' : bigint,
  'outgoing_atomic' : bigint,
  'quote' : CheckoutQuote,
  'progress' : CheckoutProgress,
  'service_reserve_atomic' : bigint,
}
export interface CommerceInit {
  'daily_orders' : number,
  'max_subjects' : bigint,
  'environment' : Environment,
  'governance' : Principal,
  'membership_canister' : Principal,
  'user_homes' : Array<Principal>,
  'catalog' : Catalog,
}
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
export interface ExecutionEntitlement {
  'month' : MonthEntitlement,
  'view' : EntitlementView,
}
export interface ExecutionWeights {
  'ecdsa_secp256k1' : bigint,
  'ed25519' : bigint,
  'version' : bigint,
}
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
export interface OpenCheckout {
  'quote' : CheckoutQuote,
  'authorization' : ProductAuthorizationRequest,
}
export type PlanId = { 'Max' : null } |
  { 'Pro' : null } |
  { 'Free' : null } |
  { 'Plus' : null };
export interface PlanVersion {
  'catalog_version' : bigint,
  'terms_version' : bigint,
  'price_cents' : bigint,
  'weights' : ExecutionWeights,
  'plan_id' : PlanId,
  'limits' : ResourceLimits,
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
export interface ProductDecision {
  'decision_id' : Uint8Array | number[],
  'offer' : BillingOffer,
  'source' : SettlementSource,
  'decided_at_ms' : bigint,
  'version' : number,
  'apply_by_ms' : bigint,
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
export interface ProductRegistration {
  'product_id' : string,
  'subject_size' : number,
  'terms_hash' : Uint8Array | number[],
  'version' : number,
  'config_version' : bigint,
  'merchant' : Account,
  'quote_authority' : Principal,
  'environment' : Environment,
  'ledgers' : Array<Principal>,
  'beneficiary_authority' : Principal,
  'adapter' : Principal,
  'subject_schema' : string,
  'paused' : boolean,
}
export type ProductRejection = { 'OfferMismatch' : null } |
  { 'IntervalReserved' : null } |
  { 'RevisionConflict' : null } |
  { 'Unauthorized' : null } |
  { 'Expired' : null };
export interface ResourceLimits {
  'active_channels' : bigint,
  'monthly_execution_units' : bigint,
  'storage_bytes' : bigint,
}
export type Result = { 'Ok' : ProductReceipt } |
  { 'Err' : Error };
export type Result_1 = { 'Ok' : CashCancellationReceipt } |
  { 'Err' : Error };
export type Result_10 = { 'Ok' : ExecutionEntitlement } |
  { 'Err' : Error };
export type Result_11 = { 'Ok' : [] | [ProductReceipt] } |
  { 'Err' : Error };
export type Result_12 = { 'Ok' : BillingOffer } |
  { 'Err' : Error };
export type Result_13 = { 'Ok' : CashTransferProgress } |
  { 'Err' : Error };
export type Result_14 = { 'Ok' : SettlementAsset } |
  { 'Err' : Error };
export type Result_15 = { 'Ok' : CheckoutQuote } |
  { 'Err' : Error };
export type Result_16 = {
    'Ok' : [AppRegistration, [] | [ProductRegistration]]
  } |
  { 'Err' : Error };
export type Result_17 = { 'Ok' : EntitlementView } |
  { 'Err' : Error };
export type Result_18 = { 'Ok' : null } |
  { 'Err' : Error };
export type Result_2 = { 'Ok' : CheckoutProgress } |
  { 'Err' : Error };
export type Result_3 = { 'Ok' : CertifiedBatch } |
  { 'Err' : Error };
export type Result_4 = { 'Ok' : Array<CheckoutDeposit> } |
  { 'Err' : Error };
export type Result_5 = { 'Ok' : CheckoutOperationsPage } |
  { 'Err' : Error };
export type Result_6 = { 'Ok' : CashTransfersPage } |
  { 'Err' : Error };
export type Result_7 = { 'Ok' : CashTransfer } |
  { 'Err' : Error };
export type Result_8 = { 'Ok' : [] | [CashCancellationReceipt] } |
  { 'Err' : Error };
export type Result_9 = { 'Ok' : CheckoutView } |
  { 'Err' : Error };
export interface SettlementAsset {
  'decimals' : number,
  'asset' : SettlementAssetKind,
  'max_network_fee_atomic' : bigint,
  'price_valid_until_ms' : bigint,
  'price_observed_at_ms' : bigint,
  'version' : number,
  'enabled' : boolean,
  'network_fee_atomic' : bigint,
  'ledger' : Principal,
  'environment' : Environment,
  'price_usd_micros' : bigint,
  'policy_version' : bigint,
}
export type SettlementAssetKind = { 'CkUsdc' : null } |
  { 'CkUsdt' : null };
export interface SettlementAssetView {
  'ledger_verified' : boolean,
  'policy' : SettlementAsset,
}
export type SettlementMethod = { 'Cash' : null } |
  { 'Panda' : null };
export type SettlementSource = {
    'Cash' : {
      'amount_atomic' : bigint,
      'block_index' : bigint,
      'ledger' : Principal,
      'order_id' : Uint8Array | number[],
    }
  } |
  {
    'Panda' : {
      'claim_id' : Uint8Array | number[],
      'quote_hash' : Uint8Array | number[],
      'lease_until_ms' : bigint,
      'committed_until_ms' : bigint,
    }
  };
export type SigningProfile = { 'FileStatementV1' : null } |
  { 'TextStatementV1' : null } |
  { 'DigestStatementV1' : null } |
  { 'AppActionV1' : null };
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
}
export interface _SERVICE {
  'apply_product_decision' : ActorMethod<[ProductDecision], Result>,
  'cancel_cash_contract' : ActorMethod<
    [Uint8Array | number[], Uint8Array | number[], Uint8Array | number[]],
    Result_1
  >,
  'cancel_checkout' : ActorMethod<[Uint8Array | number[]], Result_2>,
  'check_checkout_funding' : ActorMethod<
    [Uint8Array | number[], CashBlock],
    Result_2
  >,
  'checkout_certificate' : ActorMethod<[Uint8Array | number[]], Result_3>,
  'checkout_deposits' : ActorMethod<[Uint8Array | number[]], Result_4>,
  'checkout_operations' : ActorMethod<
    [[] | [Uint8Array | number[]], number],
    Result_5
  >,
  'checkout_progress' : ActorMethod<[Uint8Array | number[]], Result_2>,
  'checkout_transfer_certificate' : ActorMethod<
    [Uint8Array | number[]],
    Result_3
  >,
  'checkout_transfers' : ActorMethod<
    [[] | [Uint8Array | number[]], number],
    Result_6
  >,
  'claim_checkout_fee_reserve' : ActorMethod<[Uint8Array | number[]], Result_7>,
  'claim_checkout_refund' : ActorMethod<
    [Uint8Array | number[], Principal, Array<bigint>, Uint8Array | number[]],
    Result_7
  >,
  'collect_checkout_revenue' : ActorMethod<[Uint8Array | number[]], Result_7>,
  'get_cash_cancellation' : ActorMethod<[Uint8Array | number[]], Result_8>,
  'get_catalog' : ActorMethod<[], Result_3>,
  'get_checkout' : ActorMethod<[Uint8Array | number[]], Result_9>,
  'get_checkout_for_product' : ActorMethod<[Uint8Array | number[]], Result_9>,
  'get_checkout_transfer' : ActorMethod<[Uint8Array | number[]], Result_7>,
  'get_entitlement_batch' : ActorMethod<[Array<Beneficiary>], Result_3>,
  'get_execution_entitlement' : ActorMethod<
    [Beneficiary, number, bigint],
    Result_10
  >,
  'get_product_decision' : ActorMethod<[Uint8Array | number[]], Result_11>,
  'integration_configuration_certificate' : ActorMethod<
    [string, [] | [string]],
    Result_3
  >,
  'list_catalogs' : ActorMethod<[[] | [bigint]], Array<Catalog>>,
  'open_checkout' : ActorMethod<[OpenCheckout], Result_9>,
  'prepare_account_subscription' : ActorMethod<
    [string, Beneficiary, string, Uint8Array | number[]],
    Result_12
  >,
  'process_checkout_transfer' : ActorMethod<[Uint8Array | number[]], Result_13>,
  'publish_settlement_price' : ActorMethod<
    [Principal, bigint, bigint],
    Result_14
  >,
  'quote_checkout' : ActorMethod<[BillingOffer, Principal, Account], Result_15>,
  'read_integration_configuration' : ActorMethod<
    [string, [] | [string]],
    Result_16
  >,
  'reconcile_checkout' : ActorMethod<[Uint8Array | number[]], Result_2>,
  'reconcile_checkout_transfer' : ActorMethod<
    [Uint8Array | number[], CashBlock],
    Result_13
  >,
  'refresh_catalog' : ActorMethod<[], Catalog>,
  'refresh_entitlement' : ActorMethod<[Beneficiary], Result_17>,
  'register_integration_app' : ActorMethod<[AppRegistration], Result_18>,
  'register_integration_product' : ActorMethod<
    [ProductRegistration],
    Result_18
  >,
  'register_settlement_asset' : ActorMethod<[SettlementAsset], Result_18>,
  'release_product_billing' : ActorMethod<
    [ProductAuthorizationRequest],
    Result_18
  >,
  'reserve_product_billing' : ActorMethod<
    [ProductAuthorizationRequest, bigint],
    Result_18
  >,
  'revise_checkout_transfer_fee' : ActorMethod<
    [Uint8Array | number[], bigint],
    Result_7
  >,
  'schedule_policy' : ActorMethod<[Catalog], Result_18>,
  'set_admission_pause' : ActorMethod<[boolean], Result_18>,
  'set_settlement_price_authority' : ActorMethod<[Principal], Result_18>,
  'settlement_assets' : ActorMethod<[], Array<SettlementAssetView>>,
  'settlement_assets_certificate' : ActorMethod<[], Result_3>,
  'verify_billing_offer' : ActorMethod<[BillingOffer], Result_18>,
  'verify_settlement_asset' : ActorMethod<
    [Principal, [] | [bigint]],
    Result_18
  >,
}
export declare const idlFactory: IDL.InterfaceFactory;
export declare const init: (args: { IDL: typeof IDL }) => IDL.Type[];
