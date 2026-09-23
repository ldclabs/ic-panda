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
    'weights' : ExecutionWeights,
    'plan_id' : PlanId,
    'limits' : ResourceLimits,
  });
  const Catalog = IDL.Record({
    'storage_products' : IDL.Vec(StorageProduct),
    'schema' : IDL.Nat16,
    'effective_at_ms' : IDL.Nat64,
    'version' : IDL.Nat64,
    'plans' : IDL.Vec(PlanVersion),
    'terms_digest' : IDL.Vec(IDL.Nat8),
  });
  const CommerceInit = IDL.Record({
    'daily_orders' : IDL.Nat32,
    'max_subjects' : IDL.Nat64,
    'environment' : Environment,
    'governance' : IDL.Principal,
    'membership_canister' : IDL.Principal,
    'user_homes' : IDL.Vec(IDL.Principal),
    'catalog' : Catalog,
  });
  const Beneficiary = IDL.Record({
    'product_id' : IDL.Text,
    'authority_canister' : IDL.Principal,
    'subject_bytes' : IDL.Vec(IDL.Nat8),
    'subject_schema' : IDL.Text,
  });
  const SettlementMethod = IDL.Variant({
    'Cash' : IDL.Null,
    'Panda' : IDL.Null,
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
  const SettlementSource = IDL.Variant({
    'Cash' : IDL.Record({
      'amount_atomic' : IDL.Nat,
      'block_index' : IDL.Nat,
      'ledger' : IDL.Principal,
      'order_id' : IDL.Vec(IDL.Nat8),
    }),
    'Panda' : IDL.Record({
      'claim_id' : IDL.Vec(IDL.Nat8),
      'quote_hash' : IDL.Vec(IDL.Nat8),
      'lease_until_ms' : IDL.Nat64,
      'committed_until_ms' : IDL.Nat64,
    }),
  });
  const ProductDecision = IDL.Record({
    'decision_id' : IDL.Vec(IDL.Nat8),
    'offer' : BillingOffer,
    'source' : SettlementSource,
    'decided_at_ms' : IDL.Nat64,
    'version' : IDL.Nat16,
    'apply_by_ms' : IDL.Nat64,
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
  const Result = IDL.Variant({ 'Ok' : ProductReceipt, 'Err' : Error });
  const CashCancellationReceipt = IDL.Record({
    'decision_hash' : IDL.Vec(IDL.Nat8),
    'cancelled' : IDL.Bool,
    'business_revision' : IDL.Nat64,
    'contract_id' : IDL.Vec(IDL.Nat8),
    'cancelled_at_ms' : IDL.Nat64,
    'order_id' : IDL.Vec(IDL.Nat8),
  });
  const Result_1 = IDL.Variant({
    'Ok' : CashCancellationReceipt,
    'Err' : Error,
  });
  const CheckoutStatus = IDL.Variant({
    'Applied' : IDL.Null,
    'Reserving' : IDL.Null,
    'AwaitingFunding' : IDL.Null,
    'Rejected' : IDL.Null,
    'RefundCommitted' : IDL.Null,
    'Applying' : IDL.Null,
  });
  const CheckoutProgress = IDL.Record({
    'status' : CheckoutStatus,
    'decision_id' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'order_id' : IDL.Vec(IDL.Nat8),
  });
  const Result_2 = IDL.Variant({ 'Ok' : CheckoutProgress, 'Err' : Error });
  const CashBlock = IDL.Record({
    'block_index' : IDL.Nat,
    'ledger' : IDL.Principal,
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
  const Result_3 = IDL.Variant({ 'Ok' : CertifiedBatch, 'Err' : Error });
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const CheckoutDeposit = IDL.Record({
    'amount_atomic' : IDL.Nat,
    'refundable_atomic' : IDL.Nat,
    'from' : Account,
    'block' : CashBlock,
    'order_id' : IDL.Vec(IDL.Nat8),
  });
  const Result_4 = IDL.Variant({
    'Ok' : IDL.Vec(CheckoutDeposit),
    'Err' : Error,
  });
  const SettlementAssetKind = IDL.Variant({
    'CkUsdc' : IDL.Null,
    'CkUsdt' : IDL.Null,
  });
  const SettlementAsset = IDL.Record({
    'decimals' : IDL.Nat16,
    'asset' : SettlementAssetKind,
    'max_network_fee_atomic' : IDL.Nat,
    'price_valid_until_ms' : IDL.Nat64,
    'price_observed_at_ms' : IDL.Nat64,
    'version' : IDL.Nat16,
    'enabled' : IDL.Bool,
    'network_fee_atomic' : IDL.Nat,
    'ledger' : IDL.Principal,
    'environment' : Environment,
    'price_usd_micros' : IDL.Nat,
    'policy_version' : IDL.Nat64,
  });
  const CashQuote = IDL.Record({
    'amount_atomic' : IDL.Nat,
    'offer_hash' : IDL.Vec(IDL.Nat8),
    'fee_reserve_atomic' : IDL.Nat,
    'funding_deadline_ms' : IDL.Nat64,
    'max_network_fee_atomic' : IDL.Nat,
    'activation_deadline_ms' : IDL.Nat64,
    'deposit' : Account,
    'version' : IDL.Nat16,
    'ledger' : IDL.Principal,
    'payer' : Account,
    'conversion_hash' : IDL.Vec(IDL.Nat8),
  });
  const ProductRegistration = IDL.Record({
    'product_id' : IDL.Text,
    'subject_size' : IDL.Nat16,
    'terms_hash' : IDL.Vec(IDL.Nat8),
    'version' : IDL.Nat16,
    'config_version' : IDL.Nat64,
    'merchant' : Account,
    'quote_authority' : IDL.Principal,
    'environment' : Environment,
    'subsidy_budget_id' : IDL.Vec(IDL.Nat8),
    'ledgers' : IDL.Vec(IDL.Principal),
    'beneficiary_authority' : IDL.Principal,
    'adapter' : IDL.Principal,
    'subject_schema' : IDL.Text,
    'paused' : IDL.Bool,
  });
  const CheckoutQuote = IDL.Record({
    'asset' : SettlementAsset,
    'offer' : BillingOffer,
    'cash' : CashQuote,
    'quoted_at_ms' : IDL.Nat64,
    'product' : ProductRegistration,
  });
  const CheckoutView = IDL.Record({
    'receipt' : IDL.Opt(ProductReceipt),
    'fee_reserve_atomic' : IDL.Nat,
    'outgoing_atomic' : IDL.Nat,
    'quote' : CheckoutQuote,
    'progress' : CheckoutProgress,
    'service_reserve_atomic' : IDL.Nat,
  });
  const CheckoutLedgerBalance = IDL.Record({
    'fee_reserve_atomic' : IDL.Nat,
    'refundable_atomic' : IDL.Nat,
    'outgoing_atomic' : IDL.Nat,
    'service_reserve_atomic' : IDL.Nat,
    'ledger' : IDL.Principal,
    'incoming_atomic' : IDL.Nat,
  });
  const CheckoutOperationAudit = IDL.Record({
    'order' : CheckoutView,
    'balances' : IDL.Vec(CheckoutLedgerBalance),
  });
  const CheckoutOperationsPage = IDL.Record({
    'orders' : IDL.Vec(CheckoutOperationAudit),
    'next' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Result_5 = IDL.Variant({
    'Ok' : CheckoutOperationsPage,
    'Err' : Error,
  });
  const CashTransferStatus = IDL.Variant({
    'Superseded' : IDL.Null,
    'Rejected' : IDL.Null,
    'Succeeded' : IDL.Null,
    'InFlight' : IDL.Null,
    'Unknown' : IDL.Null,
    'Pending' : IDL.Null,
  });
  const CashTransfer = IDL.Record({
    'to' : Account,
    'status' : CashTransferStatus,
    'source_subaccount' : IDL.Vec(IDL.Nat8),
    'amount_atomic' : IDL.Nat,
    'expected_fee_atomic' : IDL.Opt(IDL.Nat),
    'replaces' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'block_index' : IDL.Opt(IDL.Nat),
    'created_at_time_ns' : IDL.Nat64,
    'memo' : IDL.Vec(IDL.Nat8),
    'replaced_by' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'transfer_id' : IDL.Vec(IDL.Nat8),
    'ledger' : IDL.Principal,
    'max_fee_atomic' : IDL.Nat,
    'fee_atomic' : IDL.Nat,
    'order_id' : IDL.Vec(IDL.Nat8),
    'error_code' : IDL.Opt(IDL.Text),
  });
  const CashTransfersPage = IDL.Record({
    'transfers' : IDL.Vec(CashTransfer),
    'next' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Result_6 = IDL.Variant({ 'Ok' : CashTransfersPage, 'Err' : Error });
  const Result_7 = IDL.Variant({ 'Ok' : CashTransfer, 'Err' : Error });
  const Result_8 = IDL.Variant({
    'Ok' : IDL.Opt(CashCancellationReceipt),
    'Err' : Error,
  });
  const Result_9 = IDL.Variant({ 'Ok' : CheckoutView, 'Err' : Error });
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
  const Result_10 = IDL.Variant({ 'Ok' : ExecutionEntitlement, 'Err' : Error });
  const Result_11 = IDL.Variant({
    'Ok' : IDL.Opt(ProductReceipt),
    'Err' : Error,
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
  const ApprovalPurpose = IDL.Variant({
    'AppAction' : IDL.Null,
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
  const ProductAuthorizationRequest = IDL.Record({
    'product_approval' : IDL.Opt(ProductApproval),
    'account_approval' : ApplicationApproval,
    'approval_id' : IDL.Vec(IDL.Nat8),
    'user_home' : IDL.Principal,
    'offer' : BillingOffer,
  });
  const OpenCheckout = IDL.Record({
    'quote' : CheckoutQuote,
    'authorization' : ProductAuthorizationRequest,
  });
  const Result_12 = IDL.Variant({ 'Ok' : BillingOffer, 'Err' : Error });
  const CashTransferProgress = IDL.Record({
    'status' : CashTransferStatus,
    'block_index' : IDL.Opt(IDL.Nat),
    'transfer_id' : IDL.Vec(IDL.Nat8),
  });
  const Result_13 = IDL.Variant({ 'Ok' : CashTransferProgress, 'Err' : Error });
  const Result_14 = IDL.Variant({ 'Ok' : SettlementAsset, 'Err' : Error });
  const Result_15 = IDL.Variant({ 'Ok' : CheckoutQuote, 'Err' : Error });
  const AppCapability = IDL.Variant({
    'SignAction' : IDL.Null,
    'Checkout' : IDL.Null,
    'SignDocument' : IDL.Null,
    'Authenticate' : IDL.Null,
  });
  const SigningProfile = IDL.Variant({
    'FileStatementV1' : IDL.Null,
    'TextStatementV1' : IDL.Null,
    'DigestStatementV1' : IDL.Null,
    'AppActionV1' : IDL.Null,
  });
  const AppRegistration = IDL.Record({
    'cose_homes' : IDL.Vec(IDL.Principal),
    'capabilities' : IDL.Vec(AppCapability),
    'origins' : IDL.Vec(IDL.Text),
    'product_ids' : IDL.Vec(IDL.Text),
    'authentication_receiver' : IDL.Principal,
    'version' : IDL.Nat16,
    'app_id' : IDL.Text,
    'config_version' : IDL.Nat64,
    'environment' : Environment,
    'action_authority' : IDL.Principal,
    'user_homes' : IDL.Vec(IDL.Principal),
    'profiles' : IDL.Vec(SigningProfile),
    'paused' : IDL.Bool,
  });
  const Result_16 = IDL.Variant({
    'Ok' : IDL.Tuple(AppRegistration, IDL.Opt(ProductRegistration)),
    'Err' : Error,
  });
  const Result_17 = IDL.Variant({ 'Ok' : EntitlementView, 'Err' : Error });
  const Result_18 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  const SettlementAssetView = IDL.Record({
    'ledger_verified' : IDL.Bool,
    'policy' : SettlementAsset,
  });
  return IDL.Service({
    'apply_product_decision' : IDL.Func([ProductDecision], [Result], []),
    'cancel_cash_contract' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_1],
        [],
      ),
    'cancel_checkout' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_2], []),
    'check_checkout_funding' : IDL.Func(
        [IDL.Vec(IDL.Nat8), CashBlock],
        [Result_2],
        [],
      ),
    'checkout_certificate' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_3],
        ['query'],
      ),
    'checkout_deposits' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_4], ['query']),
    'checkout_operations' : IDL.Func(
        [IDL.Opt(IDL.Vec(IDL.Nat8)), IDL.Nat16],
        [Result_5],
        ['query'],
      ),
    'checkout_progress' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_2], ['query']),
    'checkout_transfer_certificate' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_3],
        ['query'],
      ),
    'checkout_transfers' : IDL.Func(
        [IDL.Opt(IDL.Vec(IDL.Nat8)), IDL.Nat16],
        [Result_6],
        ['query'],
      ),
    'claim_checkout_fee_reserve' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_7],
        [],
      ),
    'claim_checkout_refund' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Principal, IDL.Vec(IDL.Nat), IDL.Vec(IDL.Nat8)],
        [Result_7],
        [],
      ),
    'collect_checkout_revenue' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_7], []),
    'get_cash_cancellation' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_8], []),
    'get_catalog' : IDL.Func([], [Result_3], ['query']),
    'get_checkout' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_9], ['query']),
    'get_checkout_for_product' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_9], []),
    'get_checkout_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_7],
        ['query'],
      ),
    'get_entitlement_batch' : IDL.Func(
        [IDL.Vec(Beneficiary)],
        [Result_3],
        ['query'],
      ),
    'get_execution_entitlement' : IDL.Func(
        [Beneficiary, IDL.Nat32, IDL.Nat64],
        [Result_10],
        [],
      ),
    'get_product_decision' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_11], []),
    'integration_configuration_certificate' : IDL.Func(
        [IDL.Text, IDL.Opt(IDL.Text)],
        [Result_3],
        ['query'],
      ),
    'list_catalogs' : IDL.Func(
        [IDL.Opt(IDL.Nat64)],
        [IDL.Vec(Catalog)],
        ['query'],
      ),
    'open_checkout' : IDL.Func([OpenCheckout], [Result_9], []),
    'prepare_account_subscription' : IDL.Func(
        [IDL.Text, Beneficiary, IDL.Text, IDL.Vec(IDL.Nat8)],
        [Result_12],
        [],
      ),
    'process_checkout_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_13],
        [],
      ),
    'publish_settlement_price' : IDL.Func(
        [IDL.Principal, IDL.Nat, IDL.Nat64],
        [Result_14],
        [],
      ),
    'quote_checkout' : IDL.Func(
        [BillingOffer, IDL.Principal, Account],
        [Result_15],
        [],
      ),
    'read_integration_configuration' : IDL.Func(
        [IDL.Text, IDL.Opt(IDL.Text)],
        [Result_16],
        [],
      ),
    'reconcile_checkout' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_2], []),
    'reconcile_checkout_transfer' : IDL.Func(
        [IDL.Vec(IDL.Nat8), CashBlock],
        [Result_13],
        [],
      ),
    'refresh_catalog' : IDL.Func([], [Catalog], []),
    'refresh_entitlement' : IDL.Func([Beneficiary], [Result_17], []),
    'register_integration_app' : IDL.Func([AppRegistration], [Result_18], []),
    'register_integration_product' : IDL.Func(
        [ProductRegistration],
        [Result_18],
        [],
      ),
    'register_settlement_asset' : IDL.Func([SettlementAsset], [Result_18], []),
    'release_product_billing' : IDL.Func(
        [ProductAuthorizationRequest],
        [Result_18],
        [],
      ),
    'reserve_product_billing' : IDL.Func(
        [ProductAuthorizationRequest, IDL.Nat64],
        [Result_18],
        [],
      ),
    'revise_checkout_transfer_fee' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat],
        [Result_7],
        [],
      ),
    'schedule_policy' : IDL.Func([Catalog], [Result_18], []),
    'set_admission_pause' : IDL.Func([IDL.Bool], [Result_18], []),
    'set_settlement_price_authority' : IDL.Func(
        [IDL.Principal],
        [Result_18],
        [],
      ),
    'settlement_assets' : IDL.Func(
        [],
        [IDL.Vec(SettlementAssetView)],
        ['query'],
      ),
    'settlement_assets_certificate' : IDL.Func([], [Result_3], ['query']),
    'verify_billing_offer' : IDL.Func([BillingOffer], [Result_18], []),
    'verify_settlement_asset' : IDL.Func(
        [IDL.Principal, IDL.Opt(IDL.Nat)],
        [Result_18],
        [],
      ),
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
    'weights' : ExecutionWeights,
    'plan_id' : PlanId,
    'limits' : ResourceLimits,
  });
  const Catalog = IDL.Record({
    'storage_products' : IDL.Vec(StorageProduct),
    'schema' : IDL.Nat16,
    'effective_at_ms' : IDL.Nat64,
    'version' : IDL.Nat64,
    'plans' : IDL.Vec(PlanVersion),
    'terms_digest' : IDL.Vec(IDL.Nat8),
  });
  const CommerceInit = IDL.Record({
    'daily_orders' : IDL.Nat32,
    'max_subjects' : IDL.Nat64,
    'environment' : Environment,
    'governance' : IDL.Principal,
    'membership_canister' : IDL.Principal,
    'user_homes' : IDL.Vec(IDL.Principal),
    'catalog' : Catalog,
  });
  return [CommerceInit];
};
