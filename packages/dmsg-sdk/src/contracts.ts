// Generated from dmsg_types integration.rs and app_action.rs by generate-integration-sdk.py.
// CBOR values use raw principal/hash/account bytes and bigint integers.

export type Eligibility = 'Eligible' | 'Ineligible' | 'Unverifiable'

export type Environment = 'Local' | 'Staging' | 'Production'

export interface Account { owner: Uint8Array; subaccount: Uint8Array | null }

export interface Beneficiary { product_id: string; authority_canister: Uint8Array; subject_schema: string; subject_bytes: Uint8Array }

export type AppCapability =
  | 'Authenticate'
  | 'SignDocument'
  | 'SignAction'
  | 'Checkout'

export type SigningProfile =
  | 'TextStatementV1'
  | 'DigestStatementV1'
  | 'FileStatementV1'
  | 'AppActionV1'

export interface AppRegistration {
  version: bigint
  environment: Environment
  app_id: string
  config_version: bigint
  origins: string[]
  user_homes: Uint8Array[]
  cose_homes: Uint8Array[]
  product_ids: string[]
  capabilities: AppCapability[]
  profiles: SigningProfile[]
  authentication_receiver: Uint8Array
  action_authority: Uint8Array
  paused: boolean
}

export interface ProductRegistration {
  version: bigint
  environment: Environment
  product_id: string
  config_version: bigint
  quote_authority: Uint8Array
  beneficiary_authority: Uint8Array
  adapter: Uint8Array
  subject_schema: string
  subject_size: bigint
  merchant: Account
  ledgers: Uint8Array[]
  terms_hash: Uint8Array
  paused: boolean
}

export type AuthenticationPurpose =
  | 'Login'
  | 'Link'
  | 'Reauthenticate'

export interface AuthenticationRequest {
  version: bigint
  environment: Environment
  app_id: string
  app_config_version: bigint
  origin: string
  receiver: Uint8Array
  challenge_hash: Uint8Array
  session_key_hash: Uint8Array
  purpose: AuthenticationPurpose
  nonce: Uint8Array
  operation_id: Uint8Array
  issued_at_ms: bigint
  expires_at_ms: bigint
}

export interface AuthenticationResult {
  version: bigint
  request: AuthenticationRequest
  account_id: Uint8Array
  home_user: Uint8Array
  security_epoch: bigint
  device_id: Uint8Array
  approved_at_ms: bigint
  expires_at_ms: bigint
}

export type SettlementMethod =
  | 'Cash'
  | 'Panda'

export interface BillingOffer {
  version: bigint
  environment: Environment
  app_id: string
  product_id: string
  offer_id: Uint8Array
  beneficiary: Beneficiary
  quote_authority: Uint8Array
  adapter: Uint8Array
  sku: string
  product_terms_hash: Uint8Array
  expected_business_revision: bigint
  amount_usd_micros: bigint
  starts_at_ms: bigint
  expires_at_ms: bigint
  issued_at_ms: bigint
  accept_by_ms: bigint
  operation_id: Uint8Array
  allowed_settlement_methods: SettlementMethod[]
}

export interface PandaRatePolicy {
  version: bigint
  policy_version: bigint
  environment: Environment
  product_ids: string[]
  r_num: bigint
  r_den: bigint
  published_at_ms: bigint
  effective_at_ms: bigint
}

export interface PandaQuote {
  quoted_at_ms: bigint
  version: bigint
  offer_hash: Uint8Array
  policy: PandaRatePolicy
  required_stake_e8s: bigint
  application_deadline_ms: bigint
  committed_until_ms: bigint
}

export interface CashQuote {
  version: bigint
  offer_hash: Uint8Array
  ledger: Uint8Array
  amount_atomic: bigint
  conversion_hash: Uint8Array
  payer: Account
  deposit: Account
  max_network_fee_atomic: bigint
  fee_reserve_atomic: bigint
  funding_deadline_ms: bigint
  activation_deadline_ms: bigint
}

export type ApprovalPurpose =
  | 'CashCheckout'
  | 'PandaSubscription'

export interface ApplicationApproval {
  version: bigint
  environment: Environment
  app_id: string
  app_config_version: bigint
  origin: string
  approving_account: Uint8Array
  service: Uint8Array
  beneficiary: Beneficiary
  actor: Uint8Array
  purpose: ApprovalPurpose
  action_digest: Uint8Array
  operation_id: Uint8Array
  nonce: Uint8Array
  expires_at_ms: bigint
}

export type SettlementSource =
  | { Cash: { order_id: Uint8Array; ledger: Uint8Array; block_index: bigint; amount_atomic: bigint } }
  | { Panda: { claim_id: Uint8Array; quote_hash: Uint8Array; committed_until_ms: bigint; lease_until_ms: bigint } }

export interface ProductDecision {
  version: bigint
  offer: BillingOffer
  decision_id: Uint8Array
  source: SettlementSource
  decided_at_ms: bigint
  apply_by_ms: bigint
}

export type ProductOutcome =
  | { Applied: { business_revision: bigint; contract_id: Uint8Array; committed_until_ms: bigint } }
  | { Rejected: { reason: ProductRejection } }

export type ProductRejection =
  | 'RevisionConflict'
  | 'Unauthorized'
  | 'Expired'
  | 'IntervalReserved'
  | 'OfferMismatch'

export interface ProductReceipt {
  version: bigint
  decision_id: Uint8Array
  decision_hash: Uint8Array
  adapter: Uint8Array
  outcome: ProductOutcome
  applied_at_ms: bigint
}

export interface ApplicationAuthorization {
  approval_id: Uint8Array
  approval_hash: Uint8Array
  security_epoch: bigint
  verified_at_ms: bigint
  valid_until_ms: bigint
}

export interface AppAction {
  version: bigint
  environment: Environment
  app_id: string
  app_config_version: bigint
  origin: string
  receiver: Uint8Array
  actor_id: Uint8Array
  signing_account: Uint8Array
  operation_id: Uint8Array
  intent_hash: Uint8Array
  input_hash: Uint8Array
  subject_hash: Uint8Array
  precondition_hash: Uint8Array
  role_snapshot_hash: Uint8Array
  signing_policy_hash: Uint8Array
  rule_set_hash: Uint8Array
  issued_at_ms: bigint
  expires_at_ms: bigint
  command: AppActionCommand
  files: ActionFile[]
}

export type AppActionCommand =
  | { TokenListCertifyDisclosure: { project_id: bigint; contract_id: bigint; revision: bigint } }
  | { TokenListDecideReview: { project_id: bigint; case_id: bigint; round: bigint; outcome: ActionReviewOutcome; changes: ActionRequestedChange[]; rationale: string } }
  | { TokenListCertifyTransition: { project_id: bigint; transition_id: bigint; statement_hash: Uint8Array; rationale: string; analysis: (ActionArtifact | null) } }
  | { TokenListApproveTransition: { project_id: bigint; transition_id: bigint; approve: boolean; statement_hash: Uint8Array; rationale: string } }

export type ActionReviewOutcome =
  | 'Approved'
  | 'Rejected'
  | 'ChangesRequested'

export interface ActionRequestedChange {
  locator: string
  detail: string
  blocking: boolean
}

export interface ActionArtifact {
  uri: string
  sha256: Uint8Array
  content_type: string
  size: bigint
}

export interface ActionFile {
  file_id: string
  revision: bigint
  sha256: Uint8Array
  byte_length: bigint
  media_type: string
  display_name: (string | null)
  representation: ActionFileRepresentation
}

export type ActionFileRepresentation =
  | 'Original'
  | 'Encrypted'

export type SettlementAssetKind =
  | 'CkUsdt'
  | 'CkUsdc'

export interface SettlementAsset {
  version: bigint
  policy_version: bigint
  environment: Environment
  ledger: Uint8Array
  asset: SettlementAssetKind
  decimals: bigint
  price_usd_micros: bigint
  price_observed_at_ms: bigint
  price_valid_until_ms: bigint
  network_fee_atomic: bigint
  max_network_fee_atomic: bigint
  enabled: boolean
}

export interface ProductApproval {
  version: bigint
  approval_id: Uint8Array
  offer_hash: Uint8Array
  operator: Uint8Array
  method: SettlementMethod
  approved_at_ms: bigint
  expires_at_ms: bigint
}

export interface ProductAuthorizationRequest {
  offer: BillingOffer
  account_approval: ApplicationApproval
  user_home: Uint8Array
  approval_id: Uint8Array
  product_approval: (ProductApproval | null)
}

export interface ProductAuthorization {
  request_hash: Uint8Array
  operator: Uint8Array
  verified_at_ms: bigint
  valid_until_ms: bigint
}

export interface CheckoutQuote {
  product: ProductRegistration
  offer: BillingOffer
  cash: CashQuote
  asset: SettlementAsset
  quoted_at_ms: bigint
}

export interface OpenCheckout {
  quote: CheckoutQuote
  authorization: ProductAuthorizationRequest
}

export type CheckoutStatus =
  | 'Reserving'
  | 'AwaitingFunding'
  | 'Applying'
  | 'Applied'
  | 'RefundCommitted'
  | 'Rejected'

export interface CheckoutProgress {
  order_id: Uint8Array
  status: CheckoutStatus
  decision_id: (Uint8Array | null)
}

export interface CheckoutView {
  progress: CheckoutProgress
  quote: CheckoutQuote
  receipt: (ProductReceipt | null)
  outgoing_atomic: bigint
  service_reserve_atomic: bigint
  fee_reserve_atomic: bigint
}

export interface CashBlock {
  ledger: Uint8Array
  block_index: bigint
}

export interface CheckoutDeposit {
  order_id: Uint8Array
  block: CashBlock
  from: Account
  amount_atomic: bigint
  refundable_atomic: bigint
}

export type CashTransferStatus =
  | 'Pending'
  | 'InFlight'
  | 'Unknown'
  | 'Rejected'
  | 'Succeeded'
  | 'Superseded'

export interface CashTransfer {
  transfer_id: Uint8Array
  order_id: Uint8Array
  ledger: Uint8Array
  source_subaccount: Uint8Array
  to: Account
  amount_atomic: bigint
  fee_atomic: bigint
  max_fee_atomic: bigint
  memo: Uint8Array
  created_at_time_ns: bigint
  status: CashTransferStatus
  block_index: (bigint | null)
  expected_fee_atomic: (bigint | null)
  error_code: (string | null)
  replaces: (Uint8Array | null)
  replaced_by: (Uint8Array | null)
}

export type SubscriptionSource =
  | 'Included'
  | { Cash: { order_id: Uint8Array; ledger: Uint8Array; block_index: bigint; amount_atomic: bigint } }
  | { PandaClaim: { claim_id: Uint8Array; quote: PandaQuote } }

export type SubscriptionStatus =
  | 'Active'
  | 'Repairing'
  | 'Unverifiable'
  | 'Terminated'
  | 'Expired'
  | 'Cancelled'

export interface SubscriptionContract {
  contract_id: Uint8Array
  offer: BillingOffer
  source: SubscriptionSource
  decision_id: (Uint8Array | null)
  applied_at_ms: bigint
  business_revision: bigint
  lease_revision: bigint
  status: SubscriptionStatus
  lease_until_ms: bigint
  max_issued_until_ms: bigint
  qualification: Eligibility
  observed_at_ms: bigint
  repair_elapsed_ms: bigint
}

export interface CashCancellationReceipt {
  order_id: Uint8Array
  contract_id: Uint8Array
  decision_hash: Uint8Array
  cancelled_at_ms: bigint
  cancelled: boolean
  business_revision: bigint
}

export interface CashTransferProgress {
  transfer_id: Uint8Array
  status: CashTransferStatus
  block_index: (bigint | null)
}

export interface CheckoutRequest {
  offer: BillingOffer
  approving_account: Uint8Array
  product_approval: (ProductApproval | null)
  method: SettlementMethod
}

export interface SettlementAssetView {
  policy: SettlementAsset
  ledger_verified: boolean
}

export interface CheckoutLedgerBalance {
  ledger: Uint8Array
  incoming_atomic: bigint
  refundable_atomic: bigint
  service_reserve_atomic: bigint
  fee_reserve_atomic: bigint
  outgoing_atomic: bigint
}

export interface CheckoutOperationAudit {
  order: CheckoutView
  balances: CheckoutLedgerBalance[]
}

export interface CheckoutOperationsPage {
  orders: CheckoutOperationAudit[]
  next: (Uint8Array | null)
}

export interface CashTransfersPage {
  transfers: CashTransfer[]
  next: (Uint8Array | null)
}

export interface PandaServiceConfig {
  commerce_canister: Uint8Array
  max_claims: bigint
  hourly_applications: bigint
  cooling_ms: bigint
}

export interface PandaApplicationTerms {
  home_membership: Uint8Array
  user_home: Uint8Array
  approving_account: Uint8Array
  actor: Uint8Array
  sns_governance: Uint8Array
  neuron_id: Uint8Array
  offer: BillingOffer
  quote: PandaQuote
}

export interface PandaClaimRequest {
  terms: PandaApplicationTerms
  authorization: ProductAuthorizationRequest
}

export type PandaClaimStatus =
  | 'Checking'
  | 'CoolingDown'
  | 'Applying'
  | 'Active'
  | 'Terminated'
  | 'Cancelled'
  | 'Rejected'
  | 'Released'

export interface PandaClaimView {
  version: bigint
  claim_id: Uint8Array
  terms: PandaApplicationTerms
  status: PandaClaimStatus
  eligibility: Eligibility
  observed_at_ms: bigint
  valid_until_ms: bigint
  lease_revision: bigint
  cooling_until_ms: (bigint | null)
  committed_until_ms: bigint
  repair_elapsed_ms: bigint
  receipt: (ProductReceipt | null)
}

export interface PandaOperationsPage {
  claims: PandaClaimView[]
  next: (Uint8Array | null)
}

export type KeyPurpose =
  | 'AppAction'
  | 'FileAttestation'
  | 'Statement'
  | 'ContentRoot'

export type Algorithm =
  | 'Ed25519'
  | 'EcdsaSecp256k1'
  | 'VetKdBls12381'

export interface KeyDescriptor {
  key_id: Uint8Array
  account_id: Uint8Array
  purpose: KeyPurpose
  algorithm: Algorithm
  home_cose: Uint8Array
  master_key_name: string
  environment: Environment
  derivation_version: bigint
  key_generation: bigint
  public_key: Uint8Array
  public_key_fingerprint: Uint8Array
}

export type ExecutionStatus =
  | 'Authorized'
  | 'Executing'
  | 'Completed'
  | 'Failed'
  | 'Unknown'
  | 'ResultExpired'

export interface ExecutionReceipt {
  schema: bigint
  account_id: Uint8Array
  issuer: string
  request_id: Uint8Array
  device_id: Uint8Array
  security_epoch: bigint
  approved_at: bigint
  expires_at: bigint
  origin: string
  max_cycles: bigint
  to_be_signed_digest: Uint8Array
  public_key_fingerprint: Uint8Array
  status: ExecutionStatus
  signature_digest: (Uint8Array | null)
}
