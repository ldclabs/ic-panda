import { schemas } from './schemas.ts'
import type {
  AppRegistration,
  ProductRegistration,
  BillingOffer,
  PandaRatePolicy,
  AuthenticationRequest,
  ApplicationApproval,
  Environment
} from './contracts.ts'
import { digest, equalBytes } from './encoding.ts'

export const INTEGRATION_VERSION = 1n
export const COMMERCE_VERSION = 2n
export const EXTENSION_PROTOCOL = 'dmsg-extension/4'
export const AUTH_TTL_MS = 300_000n
export const MAX_SESSION_MS = 86_400_000n
export const OFFER_TTL_MS = 900_000n
export const PANDA_COOLING_MS = 3_900_000n
export const PANDA_LEASE_MS = 3_600_000n
export const CASH_FUNDING_MS = 1_800_000n
export const CASH_ACTIVATION_MS = 86_400_000n
export const APPLICATION_TTL_MS = 86_400_000n
export const POLICY_NOTICE_MS = 2_592_000_000n
export const CKUSDT_LEDGER = 'cngnf-vqaaa-aaaar-qag4q-cai'
export const CKUSDC_LEDGER = 'xevnm-gaaaa-aaaar-qafnq-cai'
const U128 = (1n << 128n) - 1n

export function requireValid(value: unknown, reason: string): asserts value {
  if (!value) throw new Error(reason)
}

function fields(shape: Record<string, unknown>, value: unknown): void {
  requireValid(
    value &&
      typeof value === 'object' &&
      Object.getPrototypeOf(value) === Object.prototype,
    'record'
  )
  const record = value as Record<string, unknown>
  requireValid(
    Object.keys(record).length === Object.keys(shape).length &&
      Object.keys(shape).every((k) => Object.hasOwn(record, k)),
    'exact fields'
  )
  for (const [name, type] of Object.entries(shape))
    validateShape(type as string, record[name])
}

/** Enforces the exact Rust wire shape, integer ranges, lengths and closed variants. */
export function validateShape(type: string, value: unknown): void {
  if (type.startsWith('Vec<')) {
    requireValid(Array.isArray(value) && value.length <= 64, 'bounded list')
    for (const item of value) validateShape(type.slice(4, -1), item)
    return
  }
  if (type.startsWith('Option<')) {
    if (value !== null) validateShape(type.slice(7, -1), value)
    return
  }
  if (/^u(16|64|128)$/.test(type)) {
    requireValid(
      typeof value === 'bigint' &&
        value >= 0n &&
        value < 1n << BigInt(type.slice(1)),
      type
    )
    return
  }
  if (type === 'bool') {
    requireValid(typeof value === 'boolean', type)
    return
  }
  if (type === 'String') {
    requireValid(
      typeof value === 'string' &&
        new TextDecoder().decode(new TextEncoder().encode(value)) === value &&
        new TextEncoder().encode(value).length <= 4096,
      type
    )
    return
  }
  if (['Hash', 'AccountId', 'Principal', 'Bytes'].includes(type)) {
    requireValid(value instanceof Uint8Array, type)
    requireValid(
      type === 'Hash'
        ? value.length === 32
        : type === 'AccountId'
          ? value.length === 12
          : type === 'Principal'
            ? value.length <= 29
            : value.length <= 64,
      type
    )
    return
  }
  const shape = (schemas as Record<string, unknown>)[type]
  requireValid(shape, 'unknown schema')
  if (Array.isArray(shape)) {
    if (typeof value === 'string') {
      requireValid(shape.includes(value), 'variant')
      return
    }
    requireValid(
      value && typeof value === 'object' && Object.keys(value).length === 1,
      'variant'
    )
    const [tag, fieldsValue] = Object.entries(value)[0]!
    const variant = shape.find(
      (v) => typeof v === 'object' && Object.hasOwn(v, tag)
    )
    requireValid(variant, 'variant')
    fields(variant[tag], fieldsValue)
    return
  }
  fields(shape as Record<string, unknown>, value)
}

export function validateIdentifier(value: string): void {
  requireValid(/^[a-z0-9-]{1,64}$/.test(value), 'identifier')
}

export function validateOrigin(value: string, environment: Environment): void {
  const url = new URL(value)
  const local =
    environment === 'Local' &&
    url.protocol === 'http:' &&
    ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname)
  requireValid(
    (url.protocol === 'https:' || local) &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash &&
      url.origin === value,
    'exact origin'
  )
}

function principal(value: Uint8Array): void {
  requireValid(
    value.length > 0 && !(value.length === 1 && value[0] === 4),
    'authenticated principal'
  )
}

function unique(values: unknown[], max: number, nonempty = true): void {
  requireValid(
    values.length <= max && (!nonempty || values.length > 0),
    'list size'
  )
  const keys = values.map((v) =>
    v instanceof Uint8Array ? [...v].join(',') : String(v)
  )
  requireValid(new Set(keys).size === keys.length, 'duplicate')
}

function nonzero(value: Uint8Array): void {
  requireValid(
    value.some((v) => v !== 0),
    'zero identifier'
  )
}

export function validateApp(app: AppRegistration): void {
  validateShape('AppRegistration', app)
  requireValid(app.version === 1n && app.config_version > 0n, 'app version')
  validateIdentifier(app.app_id)
  unique(app.origins, 16)
  unique(app.user_homes, 16)
  unique(app.cose_homes, 16)
  unique(app.product_ids, 16, false)
  unique(app.capabilities, 4)
  unique(app.profiles, 4, false)
  requireValid(
    app.capabilities.includes('SignAction') ===
      app.profiles.includes('AppActionV1') &&
      app.capabilities.includes('SignDocument') ===
        app.profiles.some((p) => p !== 'AppActionV1'),
    'profile capabilities'
  )
  app.origins.forEach((v) => validateOrigin(v, app.environment))
  app.user_homes.forEach(principal)
  app.cose_homes.forEach(principal)
  principal(app.authentication_receiver)
  app.product_ids.forEach(validateIdentifier)
}

// Raw IC principal bytes for the two explicitly selected mainnet ledgers.
const PRODUCTION_LEDGERS = [
  Uint8Array.from([0, 0, 0, 0, 2, 48, 1, 185, 1, 1]),
  Uint8Array.from([0, 0, 0, 0, 2, 48, 1, 91, 1, 1])
]

export function validateProduct(product: ProductRegistration): void {
  validateShape('ProductRegistration', product)
  requireValid(
    product.version === 2n &&
      product.config_version > 0n &&
      product.subject_size >= 1n &&
      product.subject_size <= 64n,
    'product version/size'
  )
  validateIdentifier(product.product_id)
  validateIdentifier(product.subject_schema)
  unique(product.ledgers, 2)
  for (const p of [
    product.quote_authority,
    product.beneficiary_authority,
    product.adapter,
    product.merchant.owner,
    ...product.ledgers
  ])
    principal(p)
  if (product.environment === 'Production')
    for (const ledger of product.ledgers)
      requireValid(
        PRODUCTION_LEDGERS.some((v) => equalBytes(v, ledger)),
        'unsupported ledger'
      )
  nonzero(product.terms_hash)
  nonzero(product.subsidy_budget_id)
}

export function validateBillingOffer(
  offer: BillingOffer,
  app: AppRegistration,
  product: ProductRegistration,
  now: bigint
): void {
  validateShape('BillingOffer', offer)
  validateApp(app)
  validateProduct(product)
  requireValid(
    offer.version === 2n && !app.paused && !product.paused,
    'version/paused'
  )
  requireValid(
    offer.environment === app.environment &&
      offer.environment === product.environment &&
      offer.app_id === app.app_id &&
      offer.product_id === product.product_id &&
      app.product_ids.includes(offer.product_id) &&
      app.capabilities.includes('Checkout') &&
      equalBytes(offer.quote_authority, product.quote_authority) &&
      equalBytes(offer.adapter, product.adapter) &&
      equalBytes(offer.product_terms_hash, product.terms_hash),
    'offer binding'
  )
  requireValid(
    offer.beneficiary.product_id === product.product_id &&
      equalBytes(
        offer.beneficiary.authority_canister,
        product.beneficiary_authority
      ) &&
      offer.beneficiary.subject_schema === product.subject_schema &&
      BigInt(offer.beneficiary.subject_bytes.length) === product.subject_size,
    'subject binding'
  )
  validateIdentifier(offer.sku)
  requireValid(
    offer.amount_usd_micros > 0n && offer.starts_at_ms < offer.expires_at_ms,
    'paid interval'
  )
  requireValid(
    offer.issued_at_ms <= now &&
      now < offer.accept_by_ms &&
      offer.accept_by_ms <= offer.expires_at_ms &&
      offer.accept_by_ms - offer.issued_at_ms <= OFFER_TTL_MS,
    'offer expiry'
  )
  unique(offer.allowed_settlement_methods, 2)
  nonzero(offer.offer_id)
  nonzero(offer.operation_id)
}

export function validateAuthentication(
  request: AuthenticationRequest,
  app: AppRegistration,
  now: bigint
): void {
  validateShape('AuthenticationRequest', request)
  validateApp(app)
  requireValid(request.version === 1n && !app.paused, 'version/paused')
  requireValid(
    request.environment === app.environment &&
      request.app_id === app.app_id &&
      request.app_config_version === app.config_version &&
      equalBytes(request.receiver, app.authentication_receiver) &&
      app.origins.includes(request.origin) &&
      app.capabilities.includes('Authenticate'),
    'authentication binding'
  )
  requireValid(
    request.issued_at_ms <= now &&
      now < request.expires_at_ms &&
      request.expires_at_ms - request.issued_at_ms <= AUTH_TTL_MS,
    'authentication expiry'
  )
  for (const v of [
    request.challenge_hash,
    request.session_key_hash,
    request.nonce,
    request.operation_id
  ])
    nonzero(v)
}

export function requiredPandaStake(
  amount: bigint,
  numerator: bigint,
  denominator: bigint
): bigint {
  for (const value of [amount, numerator, denominator])
    requireValid(
      typeof value === 'bigint' && value > 0n && value <= U128,
      'amount/rate'
    )
  const n = amount * numerator * 100_000_000n,
    d = 1_000_000n * denominator
  const stake = (n + d - 1n) / d
  requireValid(stake <= U128, 'stake overflow')
  return stake
}

export function validateRatePolicy(policy: PandaRatePolicy): void {
  validateShape('PandaRatePolicy', policy)
  requireValid(
    policy.version === 2n &&
      policy.policy_version > 0n &&
      policy.r_num > 0n &&
      policy.r_den > 0n &&
      policy.effective_at_ms - policy.published_at_ms >= POLICY_NOTICE_MS,
    'rate policy'
  )
  unique(policy.product_ids, 16)
  policy.product_ids.forEach(validateIdentifier)
  nonzero(policy.subsidy_budget_id)
}

export async function billingOfferHash(
  value: BillingOffer
): Promise<Uint8Array> {
  validateShape('BillingOffer', value)
  return digest('dmsg/commerce/offer/v2', value)
}
export async function authenticationRequestHash(
  value: AuthenticationRequest
): Promise<Uint8Array> {
  validateShape('AuthenticationRequest', value)
  return digest('dmsg/authentication/request/v1', value)
}
export async function applicationApprovalHash(
  value: ApplicationApproval
): Promise<Uint8Array> {
  validateShape('ApplicationApproval', value)
  return digest('dmsg/application/approval/v1', value)
}

/** Pure quotation after the caller has authenticated the product's authoritative offer. */
export async function quotePanda(
  offer: BillingOffer,
  app: AppRegistration,
  product: ProductRegistration,
  policy: PandaRatePolicy,
  now: bigint
): Promise<import('./contracts.ts').PandaQuote> {
  offer = structuredClone(offer)
  policy = structuredClone(policy)
  validateBillingOffer(offer, app, product, now)
  validateRatePolicy(policy)
  requireValid(
    offer.allowed_settlement_methods.includes('Panda') &&
      policy.environment === offer.environment &&
      policy.product_ids.includes(offer.product_id) &&
      equalBytes(policy.subsidy_budget_id, product.subsidy_budget_id),
    'PANDA policy binding'
  )
  requireValid(policy.effective_at_ms <= now, 'policy not effective')
  const deadline =
    now + APPLICATION_TTL_MS < offer.expires_at_ms
      ? now + APPLICATION_TTL_MS
      : offer.expires_at_ms
  requireValid(deadline - now > PANDA_COOLING_MS, 'cooling deadline')
  return {
    version: 2n,
    offer_hash: await billingOfferHash(offer),
    policy: structuredClone(policy),
    required_stake_e8s: requiredPandaStake(
      offer.amount_usd_micros,
      policy.r_num,
      policy.r_den
    ),
    subsidy_usd_micros: offer.amount_usd_micros,
    application_deadline_ms: deadline,
    committed_until_ms: offer.expires_at_ms
  }
}

export function validateApplicationApproval(
  value: ApplicationApproval,
  app: AppRegistration,
  product: ProductRegistration,
  expectedService: Uint8Array,
  now: bigint
): void {
  validateShape('ApplicationApproval', value)
  validateApp(app)
  validateProduct(product)
  requireValid(
    value.version === 1n && !app.paused && !product.paused,
    'version/paused'
  )
  requireValid(
    value.app_id === app.app_id &&
      value.app_config_version === app.config_version &&
      value.environment === app.environment &&
      value.environment === product.environment &&
      app.origins.includes(value.origin) &&
      app.product_ids.includes(product.product_id) &&
      equalBytes(value.service, expectedService),
    'approval binding'
  )
  requireValid(
    app.capabilities.includes(
      value.purpose === 'AppAction' ? 'SignAction' : 'Checkout'
    ),
    'capability'
  )
  principal(value.actor)
  principal(value.service)
  requireValid(
    value.beneficiary.product_id === product.product_id &&
      value.beneficiary.subject_schema === product.subject_schema &&
      equalBytes(
        value.beneficiary.authority_canister,
        product.beneficiary_authority
      ) &&
      BigInt(value.beneficiary.subject_bytes.length) === product.subject_size,
    'subject binding'
  )
  requireValid(
    now < value.expires_at_ms && value.expires_at_ms - now <= AUTH_TTL_MS,
    'approval expiry'
  )
  for (const id of [
    value.action_digest,
    value.operation_id,
    value.nonce,
    value.approving_account
  ])
    nonzero(id)
}

export async function validateCashQuote(
  value: import('./contracts.ts').CashQuote,
  offer: BillingOffer,
  product: ProductRegistration,
  acceptedAt: bigint
): Promise<void> {
  value = structuredClone(value)
  offer = structuredClone(offer)
  product = structuredClone(product)
  validateShape('CashQuote', value)
  requireValid(
    value.version === 2n &&
      equalBytes(value.offer_hash, await billingOfferHash(offer)) &&
      offer.allowed_settlement_methods.includes('Cash') &&
      product.ledgers.some((l) => equalBytes(l, value.ledger)),
    'cash binding'
  )
  principal(value.payer.owner)
  principal(value.deposit.owner)
  nonzero(value.conversion_hash)
  requireValid(
    value.amount_atomic > 0n &&
      value.fee_reserve_atomic >= value.max_network_fee_atomic &&
      value.amount_atomic + value.fee_reserve_atomic <= U128,
    'cash amount / fee'
  )
  requireValid(
    value.funding_deadline_ms > acceptedAt &&
      value.funding_deadline_ms - acceptedAt <= CASH_FUNDING_MS &&
      value.activation_deadline_ms >= value.funding_deadline_ms &&
      value.activation_deadline_ms - acceptedAt <= CASH_ACTIVATION_MS &&
      value.activation_deadline_ms <= offer.expires_at_ms,
    'cash deadline'
  )
}

export async function matchProductReceipt(
  value: import('./contracts.ts').ProductReceipt,
  decision: import('./contracts.ts').ProductDecision
): Promise<void> {
  value = structuredClone(value)
  decision = structuredClone(decision)
  validateShape('ProductReceipt', value)
  validateShape('ProductDecision', decision)
  requireValid(
    value.version === 2n &&
      decision.version === 2n &&
      equalBytes(value.decision_id, decision.decision_id) &&
      equalBytes(
        value.decision_hash,
        await digest('dmsg/commerce/decision/v2', decision)
      ) &&
      equalBytes(value.adapter, decision.offer.adapter) &&
      value.applied_at_ms >= decision.decided_at_ms,
    'receipt binding'
  )
  if ('Applied' in value.outcome)
    requireValid(
      value.outcome.Applied.committed_until_ms === decision.offer.expires_at_ms,
      'immutable commitment end'
    )
}
