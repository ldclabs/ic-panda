// Generated from the public membership.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const ProductConfig = IDL.Record({
    'product_id' : IDL.Text,
    'subject_size' : IDL.Nat16,
    'adapter' : IDL.Principal,
    'authorities' : IDL.Vec(IDL.Principal),
    'subject_schema' : IDL.Text,
  });
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
  const MembershipInit = IDL.Record({
    'max_claims' : IDL.Nat64,
    'hourly_applications' : IDL.Nat32,
    'panda_ledger' : IDL.Principal,
    'sns_root' : IDL.Principal,
    'environment' : Environment,
    'cooling_ms' : IDL.Nat64,
    'governance' : IDL.Principal,
    'products' : IDL.Vec(ProductConfig),
    'subsidy_budget' : IDL.Nat64,
    'policies' : IDL.Vec(MembershipPolicy),
  });
  const ClaimStatus = IDL.Variant({
    'CoolingDown' : IDL.Null,
    'Closing' : IDL.Null,
    'Active' : IDL.Null,
    'Released' : IDL.Null,
    'Rejected' : IDL.Null,
    'Checking' : IDL.Null,
    'Applying' : IDL.Null,
  });
  const Beneficiary = IDL.Record({
    'product_id' : IDL.Text,
    'authority_canister' : IDL.Principal,
    'subject_bytes' : IDL.Vec(IDL.Nat8),
    'subject_schema' : IDL.Text,
  });
  const Eligibility = IDL.Variant({
    'Unverifiable' : IDL.Null,
    'Ineligible' : IDL.Null,
    'Eligible' : IDL.Null,
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
  const Result = IDL.Variant({ 'Ok' : ClaimView, 'Err' : Error });
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
  const Result_1 = IDL.Variant({ 'Ok' : CertifiedBatch, 'Err' : Error });
  const Result_2 = IDL.Variant({ 'Ok' : MembershipPolicy, 'Err' : Error });
  const Result_3 = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
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
  const ClaimRequest = IDL.Record({
    'term' : TermRule,
    'benefit_id' : IDL.Vec(IDL.Nat8),
    'change' : ClaimChange,
    'policy_version' : IDL.Nat64,
    'expected_business_revision' : IDL.Nat64,
    'authorization' : MembershipIntent,
    'neuron_id' : IDL.Vec(IDL.Nat8),
  });
  const Result_4 = IDL.Variant({ 'Ok' : IDL.Nat32, 'Err' : Error });
  return IDL.Service({
    'advance_application' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'cancel_application' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'close_for_consumer' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'get_claim_certified' : IDL.Func(
        [IDL.Vec(IDL.Vec(IDL.Nat8))],
        [Result_1],
        ['query'],
      ),
    'get_claim_for_consumer' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'get_operation' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], ['query']),
    'get_policy' : IDL.Func([IDL.Nat64], [Result_2], ['query']),
    'get_policy_certified' : IDL.Func(
        [IDL.Vec(IDL.Nat64)],
        [Result_1],
        ['query'],
      ),
    'increase_subsidy_budget' : IDL.Func([IDL.Nat64], [Result_3], []),
    'reconcile_claim' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'refresh_claim' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result], []),
    'register_product' : IDL.Func([ProductConfig], [Result_3], []),
    'request_change' : IDL.Func(
        [IDL.Vec(IDL.Nat8), MembershipIntent],
        [Result],
        [],
      ),
    'request_claim' : IDL.Func([ClaimRequest], [Result], []),
    'schedule_policy' : IDL.Func([MembershipPolicy], [Result_3], []),
    'set_admission_pause' : IDL.Func([IDL.Bool], [Result_3], []),
    'sweep_expired_claims' : IDL.Func([], [Result_4], []),
    'verify_sns_configuration' : IDL.Func([], [Result_3], []),
  });
};
export const init = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const ProductConfig = IDL.Record({
    'product_id' : IDL.Text,
    'subject_size' : IDL.Nat16,
    'adapter' : IDL.Principal,
    'authorities' : IDL.Vec(IDL.Principal),
    'subject_schema' : IDL.Text,
  });
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
  const MembershipInit = IDL.Record({
    'max_claims' : IDL.Nat64,
    'hourly_applications' : IDL.Nat32,
    'panda_ledger' : IDL.Principal,
    'sns_root' : IDL.Principal,
    'environment' : Environment,
    'cooling_ms' : IDL.Nat64,
    'governance' : IDL.Principal,
    'products' : IDL.Vec(ProductConfig),
    'subsidy_budget' : IDL.Nat64,
    'policies' : IDL.Vec(MembershipPolicy),
  });
  return [MembershipInit];
};
