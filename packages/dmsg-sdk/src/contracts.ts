// Generated from dmsg_types/src/integration.rs by generate-integration-sdk.py.
// CBOR values use raw principal/hash/account bytes and bigint integers.

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
  subsidy_budget_id: Uint8Array
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
  subsidy_budget_id: Uint8Array
}

export interface PandaQuote {
  version: bigint
  offer_hash: Uint8Array
  policy: PandaRatePolicy
  required_stake_e8s: bigint
  subsidy_usd_micros: bigint
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
  | 'AppAction'

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
