// Generated from the public dmsg_commerce.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const StorageProduct = IDL.Record({
    'product_id' : IDL.Vec(IDL.Nat8),
    'price_cents' : IDL.Nat64,
    'storage_bytes' : IDL.Nat64,
  });
  const ExecutionWeights = IDL.Record({
    'ecdsa_secp256k1' : IDL.Nat64,
    'ed25519' : IDL.Nat64,
    'version' : IDL.Nat64,
  });
  const PlanId = IDL.Variant({
    'Max' : IDL.Null,
    'Pro' : IDL.Null,
    'Free' : IDL.Null,
    'Plus' : IDL.Null,
  });
  const ResourceLimits = IDL.Record({
    'active_channels' : IDL.Nat64,
    'monthly_execution_units' : IDL.Nat64,
    'storage_bytes' : IDL.Nat64,
  });
  const PlanVersion = IDL.Record({
    'catalog_version' : IDL.Nat64,
    'terms_version' : IDL.Nat64,
    'price_cents' : IDL.Nat64,
    'membership_policy_version' : IDL.Opt(IDL.Nat64),
    'weights' : ExecutionWeights,
    'plan_id' : PlanId,
    'limits' : ResourceLimits,
  });
  const Catalog = IDL.Record({
    'decimals' : IDL.Nat8,
    'storage_products' : IDL.Vec(StorageProduct),
    'schema' : IDL.Nat16,
    'effective_at_ms' : IDL.Nat64,
    'version' : IDL.Nat64,
    'ledger' : IDL.Principal,
    'ledger_fee' : IDL.Nat,
    'plans' : IDL.Vec(PlanVersion),
    'terms_digest' : IDL.Vec(IDL.Nat8),
  });
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const CommerceInit = IDL.Record({
    'daily_orders' : IDL.Nat32,
    'max_subjects' : IDL.Nat64,
    'environment' : Environment,
    'governance' : IDL.Principal,
    'membership_canister' : IDL.Principal,
    'user_homes' : IDL.Vec(IDL.Principal),
    'catalog' : Catalog,
    'treasury' : Account,
  });
  const TermRule = IDL.Variant({
    'Fixed' : IDL.Record({
      'starts_at_ms' : IDL.Nat64,
      'expires_at_ms' : IDL.Nat64,
    }),
    'CalendarYear' : IDL.Null,
  });
  const ClaimChange = IDL.Variant({
    'Start' : IDL.Null,
    'Upgrade' : IDL.Record({ 'previous_claim' : IDL.Vec(IDL.Nat8) }),
    'Replace' : IDL.Record({ 'previous_claim' : IDL.Vec(IDL.Nat8) }),
    'Renew' : IDL.Record({ 'previous_claim' : IDL.Vec(IDL.Nat8) }),
  });
  const Beneficiary = IDL.Record({
    'product_id' : IDL.Text,
    'authority_canister' : IDL.Principal,
    'subject_bytes' : IDL.Vec(IDL.Nat8),
    'subject_schema' : IDL.Text,
  });
  const MembershipIntent = IDL.Record({
    'actor' : IDL.Principal,
    'beneficiary' : Beneficiary,
    'valid_until_ms' : IDL.Nat64,
    'action_digest' : IDL.Vec(IDL.Nat8),
    'application_id' : IDL.Vec(IDL.Nat8),
    'nonce' : IDL.Vec(IDL.Nat8),
    'service_canister' : IDL.Principal,
    'environment' : Environment,
  });
  const ClaimRequest = IDL.Record({
    'term' : TermRule,
    'benefit_id' : IDL.Vec(IDL.Nat8),
    'change' : ClaimChange,
    'policy_version' : IDL.Nat64,
    'expected_business_revision' : IDL.Nat64,
    'authorization' : MembershipIntent,
    'neuron_id' : IDL.Vec(IDL.Nat8),
  });
  const DecisionKind = IDL.Variant({ 'Apply' : IDL.Null, 'Close' : IDL.Null });
  const Threshold = IDL.Variant({
    'AnnualPrice' : IDL.Record({
      'price_cents' : IDL.Nat64,
      'r_den' : IDL.Nat,
      'r_num' : IDL.Nat,
    }),
    'FixedPanda' : IDL.Record({ 'atomic' : IDL.Nat }),
  });
  const MembershipPolicy = IDL.Record({
    'product_id' : IDL.Text,
    'threshold' : Threshold,
    'subsidy_units' : IDL.Nat64,
    'benefit_id' : IDL.Vec(IDL.Nat8),
    'effective_at_ms' : IDL.Nat64,
    'version' : IDL.Nat64,
  });
  const MembershipDecision = IDL.Record({
    'qualification_until_ms' : IDL.Nat64,
    'decision_id' : IDL.Vec(IDL.Nat8),
    'claim_id' : IDL.Vec(IDL.Nat8),
    'request' : ClaimRequest,
    'kind' : DecisionKind,
    'observed_at_ms' : IDL.Nat64,
    'required_atomic' : IDL.Nat,
    'starts_at_ms' : IDL.Nat64,
    'apply_by_ms' : IDL.Nat64,
    'expires_at_ms' : IDL.Nat64,
    'policy' : MembershipPolicy,
  });
  const DecisionOutcome = IDL.Variant({
    'Applied' : IDL.Null,
    'Rejected' : IDL.Null,
  });
  const MembershipDecisionReceipt = IDL.Record({
    'decision_id' : IDL.Vec(IDL.Nat8),
    'business_revision' : IDL.Nat64,
    'commitment_until_ms' : IDL.Nat64,
    'contract_id' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'starts_at_ms' : IDL.Nat64,
    'outcome' : DecisionOutcome,
    'decision_digest' : IDL.Vec(IDL.Nat8),
    'expires_at_ms' : IDL.Nat64,
  });
  const Error = IDL.Variant({
    'MigrationKeyUnavailable' : IDL.Null,
    'LegacyWriteDisabled' : IDL.Null,
    'InvalidInput' : IDL.Text,
    'RekeyRequired' : IDL.Null,
    'VersionConflict' : IDL.Null,
    'ExecutionUnknown' : IDL.Null,
    'IntegrityFailed' : IDL.Null,
    'IdTimestampOutOfRange' : IDL.Null,
    'NotFound' : IDL.Null,
    'FeeBlocked' : IDL.Null,
    'DeviceNotApproved' : IDL.Null,
    'Locked' : IDL.Null,
    'MembershipClosing' : IDL.Null,
    'RecoveryIncomplete' : IDL.Null,
    'IdCapacityExceeded' : IDL.Null,
    'PolicyStale' : IDL.Null,
    'IdGeneratorStateConflict' : IDL.Null,
    'IdempotencyConflict' : IDL.Null,
    'UnsupportedProtocol' : IDL.Null,
    'Unavailable' : IDL.Text,
    'MembershipStale' : IDL.Null,
    'Forbidden' : IDL.Null,
    'ResultExpired' : IDL.Null,
    'Expired' : IDL.Null,
    'MembershipIneligible' : IDL.Null,
    'QuotaExceeded' : IDL.Null,
    'AuthRequired' : IDL.Null,
    'Pending' : IDL.Null,
  });
  const Result = IDL.Variant({
    'Ok' : MembershipDecisionReceipt,
    'Err' : Error,
  });
  const MembershipAuthorization = IDL.Record({
    'valid_until_ms' : IDL.Nat64,
    'security_epoch' : IDL.Nat64,
    'verified_at_ms' : IDL.Nat64,
    'intent_digest' : IDL.Vec(IDL.Nat8),
  });
  const Result_1 = IDL.Variant({
    'Ok' : MembershipAuthorization,
    'Err' : Error,
  });
  const OrderStatus = IDL.Variant({
    'Closing' : IDL.Null,
    'Active' : IDL.Null,
    'AwaitingFunding' : IDL.Null,
    'Cancelled' : IDL.Null,
    'RefundCommitted' : IDL.Null,
  });
  const OrderProgress = IDL.Record({
    'status' : OrderStatus,
    'order_id' : IDL.Vec(IDL.Nat8),
  });
  const Result_2 = IDL.Variant({ 'Ok' : OrderProgress, 'Err' : Error });
  const MerchantTransferStatus = IDL.Variant({
    'Superseded' : IDL.Null,
    'Rejected' : IDL.Null,
    'Succeeded' : IDL.Null,
    'InFlight' : IDL.Null,
    'Unknown' : IDL.Null,
    'Pending' : IDL.Null,
  });
  const TransferProgress = IDL.Record({
    'status' : MerchantTransferStatus,
    'replaced_by' : IDL.Opt(IDL.Nat64),
    'transfer_id' : IDL.Nat64,
    'order_id' : IDL.Vec(IDL.Nat8),
  });
  const Result_3 = IDL.Variant({ 'Ok' : TransferProgress, 'Err' : Error });
  const TransferError = IDL.Variant({
    'GenericError' : IDL.Record({
      'message' : IDL.Text,
      'error_code' : IDL.Nat,
    }),
    'TemporarilyUnavailable' : IDL.Null,
    'BadBurn' : IDL.Record({ 'min_burn_amount' : IDL.Nat }),
    'Duplicate' : IDL.Record({ 'duplicate_of' : IDL.Nat }),
    'BadFee' : IDL.Record({ 'expected_fee' : IDL.Nat }),
    'CreatedInFuture' : IDL.Record({ 'ledger_time' : IDL.Nat64 }),
    'TooOld' : IDL.Null,
    'InsufficientFunds' : IDL.Record({ 'balance' : IDL.Nat }),
  });
  const MerchantTransfer = IDL.Record({
    'to' : Account,
    'fee' : IDL.Nat,
    'last_error' : IDL.Opt(TransferError),
    'status' : MerchantTransferStatus,
    'replaces' : IDL.Opt(IDL.Nat64),
    'created_at_time_ns' : IDL.Nat64,
    'memo' : IDL.Vec(IDL.Nat8),
    'replaced_by' : IDL.Opt(IDL.Nat64),
    'transfer_id' : IDL.Nat64,
    'block' : IDL.Opt(IDL.Nat64),
    'order_id' : IDL.Vec(IDL.Nat8),
    'amount' : IDL.Nat,
  });
  const Result_4 = IDL.Variant({ 'Ok' : MerchantTransfer, 'Err' : Error });
  const CertifiedEntry = IDL.Record({
    'key' : IDL.Vec(IDL.Nat8),
    'value' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'witness' : IDL.Vec(IDL.Nat8),
  });
  const CertifiedBatch = IDL.Record({
    'certificate' : IDL.Vec(IDL.Nat8),
    'schema' : IDL.Nat16,
    'entries' : IDL.Vec(CertifiedEntry),
    'canister' : IDL.Principal,
  });
  const Result_5 = IDL.Variant({ 'Ok' : CertifiedBatch, 'Err' : Error });
  const MerchantDeposit = IDL.Record({
    'from' : Account,
    'refundable' : IDL.Nat,
    'block' : IDL.Nat64,
    'order_id' : IDL.Vec(IDL.Nat8),
    'amount' : IDL.Nat,
  });
  const Result_6 = IDL.Variant({ 'Ok' : MerchantDeposit, 'Err' : Error });
  const MonthSegment = IDL.Record({
    'start_ms' : IDL.Nat64,
    'monthly_units' : IDL.Nat64,
    'end_ms' : IDL.Nat64,
    'source_contract_id' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const MonthEntitlement = IDL.Record({
    'business_revision' : IDL.Nat64,
    'calculation_version' : IDL.Nat16,
    'beneficiary' : Beneficiary,
    'segments' : IDL.Vec(MonthSegment),
    'allowed_units' : IDL.Nat64,
    'weights' : ExecutionWeights,
    'month_revision' : IDL.Nat64,
    'month_utc' : IDL.Nat32,
  });
  const StorageAddon = IDL.Record({
    'contract_id' : IDL.Vec(IDL.Nat8),
    'starts_at_ms' : IDL.Nat64,
    'storage_bytes' : IDL.Nat64,
    'order_id' : IDL.Vec(IDL.Nat8),
    'expires_at_ms' : IDL.Nat64,
    'last_issued_until_ms' : IDL.Nat64,
  });
  const SourceStatus = IDL.Variant({
    'Unverifiable' : IDL.Null,
    'Free' : IDL.Null,
    'Closing' : IDL.Null,
    'Active' : IDL.Null,
    'Suspended' : IDL.Null,
    'RepairRequired' : IDL.Null,
    'Expired' : IDL.Null,
  });
  const Eligibility = IDL.Variant({
    'Unverifiable' : IDL.Null,
    'Ineligible' : IDL.Null,
    'Eligible' : IDL.Null,
  });
  const EntitlementView = IDL.Record({
    'lease_source_contract_ids' : IDL.Vec(IDL.Vec(IDL.Nat8)),
    'plan_snapshot' : PlanVersion,
    'business_revision' : IDL.Nat64,
    'service_terminated_at_ms' : IDL.Opt(IDL.Nat64),
    'beneficiary' : Beneficiary,
    'schema' : IDL.Nat16,
    'valid_until_ms' : IDL.Nat64,
    'observed_at_ms' : IDL.Nat64,
    'repair_deadline_ms' : IDL.Opt(IDL.Nat64),
    'lease_revision' : IDL.Nat64,
    'issued_at_ms' : IDL.Nat64,
    'home_commerce' : IDL.Principal,
    'addons' : IDL.Vec(StorageAddon),
    'effective_stop_at_ms' : IDL.Opt(IDL.Nat64),
    'effective_limits' : ResourceLimits,
    'next_limit_change_at_ms' : IDL.Opt(IDL.Nat64),
    'source_status' : SourceStatus,
    'eligibility_status' : Eligibility,
    'active_contract_id' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const ExecutionEntitlement = IDL.Record({
    'month' : MonthEntitlement,
    'view' : EntitlementView,
  });
  const Result_7 = IDL.Variant({ 'Ok' : ExecutionEntitlement, 'Err' : Error });
  const Result_8 = IDL.Variant({
    'Ok' : IDL.Opt(MembershipDecisionReceipt),
    'Err' : Error,
  });
  const OrderAction = IDL.Variant({
    'Buyout' : IDL.Record({ 'contract_id' : IDL.Vec(IDL.Nat8) }),
    'Storage' : IDL.Record({ 'product_id' : IDL.Vec(IDL.Nat8) }),
    'Upgrade' : IDL.Record({ 'plan' : PlanId }),
    'Renew' : IDL.Record({ 'plan' : PlanId }),
    'Subscribe' : IDL.Record({ 'plan' : PlanId }),
  });
  const QuoteOrder = IDL.Record({
    'action' : OrderAction,
    'op_id' : IDL.Vec(IDL.Nat8),
    'beneficiary' : Beneficiary,
    'payer' : Account,
    'expected_business_revision' : IDL.Nat64,
  });
  const OrderQuote = IDL.Record({
    'amount_atomic' : IDL.Nat,
    'fee_reserve' : IDL.Nat,
    'request' : QuoteOrder,
    'term' : TermRule,
    'created_at_ms' : IDL.Nat64,
    'fund_by_ms' : IDL.Nat64,
    'home_commerce' : IDL.Principal,
    'catalog' : Catalog,
    'activate_by_ms' : IDL.Nat64,
  });
  const OpenOrder = IDL.Record({
    'quote' : OrderQuote,
    'authorization' : MembershipIntent,
  });
  const BillingOrder = IDL.Record({
    'status' : OrderStatus,
    'fee_reserve' : IDL.Nat,
    'service_reserve' : IDL.Nat,
    'generation' : IDL.Nat64,
    'network_fees' : IDL.Nat,
    'activated_contract_id' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'transferred' : IDL.Nat,
    'refundable' : IDL.Nat,
    'earned' : IDL.Nat,
    'next_transfer' : IDL.Nat64,
    'receive_subaccount' : IDL.Vec(IDL.Nat8),
    'input' : OpenOrder,
    'order_id' : IDL.Vec(IDL.Nat8),
    'outgoing' : IDL.Nat,
    'funding_block' : IDL.Opt(IDL.Nat64),
    'close_effective_at_ms' : IDL.Opt(IDL.Nat64),
    'refunded_principal' : IDL.Nat,
    'busy_until_ms' : IDL.Nat64,
    'confirmed_in' : IDL.Nat,
  });
  const Result_9 = IDL.Variant({ 'Ok' : BillingOrder, 'Err' : Error });
  const Result_10 = IDL.Variant({ 'Ok' : OrderQuote, 'Err' : Error });
  const Result_11 = IDL.Variant({ 'Ok' : EntitlementView, 'Err' : Error });
  const ClaimStatus = IDL.Variant({
    'CoolingDown' : IDL.Null,
    'Closing' : IDL.Null,
    'Active' : IDL.Null,
    'Released' : IDL.Null,
    'Rejected' : IDL.Null,
    'Checking' : IDL.Null,
    'Applying' : IDL.Null,
  });
  const ClaimView = IDL.Record({
    'status' : ClaimStatus,
    'home_membership' : IDL.Principal,
    'decision_id' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'release_after_ms' : IDL.Nat64,
    'claim_id' : IDL.Vec(IDL.Nat8),
    'beneficiary' : Beneficiary,
    'schema' : IDL.Nat16,
    'valid_until_ms' : IDL.Nat64,
    'observed_at_ms' : IDL.Nat64,
    'benefit_id' : IDL.Vec(IDL.Nat8),
    'lease_revision' : IDL.Nat64,
    'starts_at_ms' : IDL.Nat64,
    'eligibility' : Eligibility,
    'policy_version' : IDL.Nat64,
    'expires_at_ms' : IDL.Nat64,
  });
  const Result_12 = IDL.Variant({ 'Ok' : ClaimView, 'Err' : Error });
  const Result_13 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  return IDL.Service({
    'apply_membership_decision' : IDL.Func([MembershipDecision], [Result], []),
    'authorize_membership_close' : IDL.Func(
        [IDL.Vec(IDL.Nat8), MembershipIntent],
        [Result_1],
        [],
      ),
    'authorize_membership_intent' : IDL.Func([ClaimRequest], [Result_1], []),
    'check_order_funding' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_2],
        [],
      ),
    'claim_deposit_refund' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_3],
        [],
      ),
    'claim_fee_reserve' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_3], []),
    'collect_revenue' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_4], []),
    'get_catalog' : IDL.Func([], [Result_5], ['query']),
    'get_deposit' : IDL.Func([IDL.Nat64], [Result_6], ['query']),
    'get_entitlement_batch' : IDL.Func(
        [IDL.Vec(Beneficiary)],
        [Result_5],
        ['query'],
      ),
    'get_execution_entitlement' : IDL.Func(
        [Beneficiary, IDL.Nat32, IDL.Nat64],
        [Result_7],
        [],
      ),
    'get_membership_decision' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_8], []),
    'get_operation' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_9], ['query']),
    'get_order_certified' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_5],
        ['query'],
      ),
    'get_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_4],
        ['query'],
      ),
    'list_catalogs' : IDL.Func(
        [IDL.Opt(IDL.Nat64)],
        [IDL.Vec(Catalog)],
        ['query'],
      ),
    'open_order' : IDL.Func([OpenOrder], [Result_9], []),
    'process_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result_3],
        [],
      ),
    'quote_order' : IDL.Func([QuoteOrder], [Result_10], ['query']),
    'reconcile_order' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_2], []),
    'reconcile_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64, IDL.Nat64],
        [Result_3],
        [],
      ),
    'refresh_catalog' : IDL.Func([], [Catalog], []),
    'refresh_entitlement' : IDL.Func([Beneficiary], [Result_11], []),
    'release_replaced_claim' : IDL.Func(
        [Beneficiary, IDL.Vec(IDL.Nat8)],
        [Result_12],
        [],
      ),
    'request_refund' : IDL.Func(
        [IDL.Vec(IDL.Nat8), MembershipIntent],
        [Result_9],
        [],
      ),
    'revise_rejected_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat64, IDL.Nat],
        [Result_4],
        [],
      ),
    'schedule_policy' : IDL.Func([Catalog], [Result_13], []),
    'set_admission_pause' : IDL.Func([IDL.Bool], [Result_13], []),
    'verify_ledger_configuration' : IDL.Func([], [Result_13], []),
  });
};
export const init = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const StorageProduct = IDL.Record({
    'product_id' : IDL.Vec(IDL.Nat8),
    'price_cents' : IDL.Nat64,
    'storage_bytes' : IDL.Nat64,
  });
  const ExecutionWeights = IDL.Record({
    'ecdsa_secp256k1' : IDL.Nat64,
    'ed25519' : IDL.Nat64,
    'version' : IDL.Nat64,
  });
  const PlanId = IDL.Variant({
    'Max' : IDL.Null,
    'Pro' : IDL.Null,
    'Free' : IDL.Null,
    'Plus' : IDL.Null,
  });
  const ResourceLimits = IDL.Record({
    'active_channels' : IDL.Nat64,
    'monthly_execution_units' : IDL.Nat64,
    'storage_bytes' : IDL.Nat64,
  });
  const PlanVersion = IDL.Record({
    'catalog_version' : IDL.Nat64,
    'terms_version' : IDL.Nat64,
    'price_cents' : IDL.Nat64,
    'membership_policy_version' : IDL.Opt(IDL.Nat64),
    'weights' : ExecutionWeights,
    'plan_id' : PlanId,
    'limits' : ResourceLimits,
  });
  const Catalog = IDL.Record({
    'decimals' : IDL.Nat8,
    'storage_products' : IDL.Vec(StorageProduct),
    'schema' : IDL.Nat16,
    'effective_at_ms' : IDL.Nat64,
    'version' : IDL.Nat64,
    'ledger' : IDL.Principal,
    'ledger_fee' : IDL.Nat,
    'plans' : IDL.Vec(PlanVersion),
    'terms_digest' : IDL.Vec(IDL.Nat8),
  });
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const CommerceInit = IDL.Record({
    'daily_orders' : IDL.Nat32,
    'max_subjects' : IDL.Nat64,
    'environment' : Environment,
    'governance' : IDL.Principal,
    'membership_canister' : IDL.Principal,
    'user_homes' : IDL.Vec(IDL.Principal),
    'catalog' : Catalog,
    'treasury' : Account,
  });
  return [CommerceInit];
};
