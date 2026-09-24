// Generated from the public membership.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const MembershipInit = IDL.Record({
    'expected_governance_module_hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'panda_ledger' : IDL.Principal,
    'sns_root' : IDL.Principal,
    'environment' : Environment,
    'governance' : IDL.Principal,
  });
  const SettlementMethod = IDL.Variant({
    'Cash' : IDL.Null,
    'Panda' : IDL.Null,
  });
  const ProductApproval = IDL.Record({
    'method' : SettlementMethod,
    'approval_id' : IDL.Vec(IDL.Nat8),
    'offer_hash' : IDL.Vec(IDL.Nat8),
    'operator' : IDL.Principal,
    'version' : IDL.Nat16,
    'expires_at_ms' : IDL.Nat64,
    'approved_at_ms' : IDL.Nat64,
  });
  const Beneficiary = IDL.Record({
    'product_id' : IDL.Text,
    'authority_canister' : IDL.Principal,
    'subject_bytes' : IDL.Vec(IDL.Nat8),
    'subject_schema' : IDL.Text,
  });
  const ApprovalPurpose = IDL.Variant({
    'CashCheckout' : IDL.Null,
    'PandaSubscription' : IDL.Null,
  });
  const ApplicationApproval = IDL.Record({
    'service' : IDL.Principal,
    'actor' : IDL.Principal,
    'beneficiary' : Beneficiary,
    'app_config_version' : IDL.Nat64,
    'origin' : IDL.Text,
    'action_digest' : IDL.Vec(IDL.Nat8),
    'operation_id' : IDL.Vec(IDL.Nat8),
    'approving_account' : IDL.Vec(IDL.Nat8),
    'version' : IDL.Nat16,
    'app_id' : IDL.Text,
    'nonce' : IDL.Vec(IDL.Nat8),
    'environment' : Environment,
    'purpose' : ApprovalPurpose,
    'expires_at_ms' : IDL.Nat64,
  });
  const BillingOffer = IDL.Record({
    'sku' : IDL.Text,
    'product_id' : IDL.Text,
    'beneficiary' : Beneficiary,
    'amount_usd_micros' : IDL.Nat,
    'operation_id' : IDL.Vec(IDL.Nat8),
    'starts_at_ms' : IDL.Nat64,
    'version' : IDL.Nat16,
    'app_id' : IDL.Text,
    'issued_at_ms' : IDL.Nat64,
    'offer_id' : IDL.Vec(IDL.Nat8),
    'quote_authority' : IDL.Principal,
    'environment' : Environment,
    'expected_business_revision' : IDL.Nat64,
    'adapter' : IDL.Principal,
    'allowed_settlement_methods' : IDL.Vec(SettlementMethod),
    'expires_at_ms' : IDL.Nat64,
    'accept_by_ms' : IDL.Nat64,
    'product_terms_hash' : IDL.Vec(IDL.Nat8),
  });
  const ProductAuthorizationRequest = IDL.Record({
    'product_approval' : IDL.Opt(ProductApproval),
    'account_approval' : ApplicationApproval,
    'approval_id' : IDL.Vec(IDL.Nat8),
    'user_home' : IDL.Principal,
    'offer' : BillingOffer,
  });
  const PandaClaimStatus = IDL.Variant({
    'Terminated' : IDL.Null,
    'CoolingDown' : IDL.Null,
    'Active' : IDL.Null,
    'Released' : IDL.Null,
    'Rejected' : IDL.Null,
    'Checking' : IDL.Null,
    'Cancelled' : IDL.Null,
    'Applying' : IDL.Null,
  });
  const PandaRatePolicy = IDL.Record({
    'product_ids' : IDL.Vec(IDL.Text),
    'effective_at_ms' : IDL.Nat64,
    'published_at_ms' : IDL.Nat64,
    'version' : IDL.Nat16,
    'environment' : Environment,
    'subsidy_budget_id' : IDL.Vec(IDL.Nat8),
    'policy_version' : IDL.Nat64,
    'r_den' : IDL.Nat,
    'r_num' : IDL.Nat,
  });
  const PandaQuote = IDL.Record({
    'required_stake_e8s' : IDL.Nat,
    'offer_hash' : IDL.Vec(IDL.Nat8),
    'version' : IDL.Nat16,
    'committed_until_ms' : IDL.Nat64,
    'quoted_at_ms' : IDL.Nat64,
    'application_deadline_ms' : IDL.Nat64,
    'policy' : PandaRatePolicy,
    'subsidy_usd_micros' : IDL.Nat,
  });
  const PandaApplicationTerms = IDL.Record({
    'user_home' : IDL.Principal,
    'home_membership' : IDL.Principal,
    'actor' : IDL.Principal,
    'offer' : BillingOffer,
    'quote' : PandaQuote,
    'approving_account' : IDL.Vec(IDL.Nat8),
    'sns_governance' : IDL.Principal,
    'neuron_id' : IDL.Vec(IDL.Nat8),
  });
  const ProductRejection = IDL.Variant({
    'OfferMismatch' : IDL.Null,
    'IntervalReserved' : IDL.Null,
    'RevisionConflict' : IDL.Null,
    'Unauthorized' : IDL.Null,
    'Expired' : IDL.Null,
  });
  const ProductOutcome = IDL.Variant({
    'Applied' : IDL.Record({
      'business_revision' : IDL.Nat64,
      'contract_id' : IDL.Vec(IDL.Nat8),
      'committed_until_ms' : IDL.Nat64,
    }),
    'Rejected' : IDL.Record({ 'reason' : ProductRejection }),
  });
  const ProductReceipt = IDL.Record({
    'decision_hash' : IDL.Vec(IDL.Nat8),
    'decision_id' : IDL.Vec(IDL.Nat8),
    'applied_at_ms' : IDL.Nat64,
    'version' : IDL.Nat16,
    'outcome' : ProductOutcome,
    'adapter' : IDL.Principal,
  });
  const Eligibility = IDL.Variant({
    'Unverifiable' : IDL.Null,
    'Ineligible' : IDL.Null,
    'Eligible' : IDL.Null,
  });
  const PandaClaimView = IDL.Record({
    'status' : PandaClaimStatus,
    'terms' : PandaApplicationTerms,
    'receipt' : IDL.Opt(ProductReceipt),
    'claim_id' : IDL.Vec(IDL.Nat8),
    'valid_until_ms' : IDL.Nat64,
    'observed_at_ms' : IDL.Nat64,
    'lease_revision' : IDL.Nat64,
    'eligibility' : Eligibility,
    'version' : IDL.Nat16,
    'cooling_until_ms' : IDL.Opt(IDL.Nat64),
    'repair_elapsed_ms' : IDL.Nat64,
    'committed_until_ms' : IDL.Nat64,
  });
  const Error = IDL.Variant({
    'MigrationKeyUnavailable' : IDL.Null,
    'LegacyWriteDisabled' : IDL.Null,
    'InvalidInput' : IDL.Text,
    'IntervalReserved' : IDL.Null,
    'NeuronOccupied' : IDL.Null,
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
  const Result = IDL.Variant({ 'Ok' : PandaClaimView, 'Err' : Error });
  const PandaServiceConfig = IDL.Record({
    'max_claims' : IDL.Nat64,
    'hourly_applications' : IDL.Nat64,
    'commerce_canister' : IDL.Principal,
    'cooling_ms' : IDL.Nat64,
  });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  const PandaSubsidyBudget = IDL.Record({
    'reserved_usd_micros' : IDL.Nat,
    'total_usd_micros' : IDL.Nat,
    'budget_id' : IDL.Vec(IDL.Nat8),
  });
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
  const Result_2 = IDL.Variant({ 'Ok' : CertifiedBatch, 'Err' : Error });
  const PandaOperationsPage = IDL.Record({
    'claims' : IDL.Vec(PandaClaimView),
    'next' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Result_3 = IDL.Variant({ 'Ok' : PandaOperationsPage, 'Err' : Error });
  const Result_4 = IDL.Variant({ 'Ok' : PandaApplicationTerms, 'Err' : Error });
  const PandaClaimRequest = IDL.Record({
    'terms' : PandaApplicationTerms,
    'authorization' : ProductAuthorizationRequest,
  });
  const Result_5 = IDL.Variant({ 'Ok' : PandaRatePolicy, 'Err' : Error });
  const Result_6 = IDL.Variant({ 'Ok' : PandaSubsidyBudget, 'Err' : Error });
  const Result_7 = IDL.Variant({ 'Ok' : IDL.Nat32, 'Err' : Error });
  return IDL.Service({
    'advance_panda_claim' : IDL.Func(
        [IDL.Vec(IDL.Nat8), ProductAuthorizationRequest],
        [Result],
        [],
      ),
    'cancel_panda_application' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'configure_panda_service' : IDL.Func([PandaServiceConfig], [Result_1], []),
    'get_panda_claim' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], ['query']),
    'get_panda_claim_for_product' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'panda_budgets' : IDL.Func([], [IDL.Vec(PandaSubsidyBudget)], ['query']),
    'panda_claim_certificate' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_2],
        ['query'],
      ),
    'panda_operations' : IDL.Func(
        [IDL.Opt(IDL.Vec(IDL.Nat8)), IDL.Nat16],
        [Result_3],
        ['query'],
      ),
    'quote_panda_subscription' : IDL.Func(
        [BillingOffer, IDL.Principal, IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_4],
        [],
      ),
    'reconcile_panda_claim' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'refresh_panda_claim' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'request_panda_claim' : IDL.Func([PandaClaimRequest], [Result], []),
    'schedule_panda_rate' : IDL.Func([PandaRatePolicy], [Result_5], []),
    'set_admission_pause' : IDL.Func([IDL.Bool], [Result_1], []),
    'set_panda_subsidy_budget' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat],
        [Result_6],
        [],
      ),
    'set_sns_governance_module_hash' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_1],
        [],
      ),
    'sweep_panda_commitments' : IDL.Func([], [Result_7], []),
    'verify_sns_configuration' : IDL.Func([], [Result_1], []),
  });
};
export const init = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const MembershipInit = IDL.Record({
    'expected_governance_module_hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'panda_ledger' : IDL.Principal,
    'sns_root' : IDL.Principal,
    'environment' : Environment,
    'governance' : IDL.Principal,
  });
  return [MembershipInit];
};
