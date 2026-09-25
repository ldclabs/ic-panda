import { Principal } from "@icp-sdk/core/principal";
import { schemas } from "./schemas.ts";
import type {
  AppRegistration,
  ProductRegistration,
  BillingOffer,
  PandaRatePolicy,
  PandaQuote,
  AuthenticationRequest,
  ApplicationApproval,
  CashQuote,
  Environment,
  ProductDecision,
  ProductReceipt,
} from "./contracts.ts";
import { ensure } from "./cose-errors.ts";
import { digest, equalBytes, isWellFormed, utf8 } from "./encoding.ts";

export const INTEGRATION_VERSION = 1n;
export const COMMERCE_VERSION = 2n;
export const EXTENSION_PROTOCOL = "dmsg-extension/4";
export const AUTH_TTL_MS = 300_000n;
export const MAX_SESSION_MS = 86_400_000n;
export const OFFER_TTL_MS = 900_000n;
export const PANDA_COOLING_MS = 3_900_000n;
export const PANDA_LEASE_MS = 3_600_000n;
export const CASH_FUNDING_MS = 1_800_000n;
export const CASH_ACTIVATION_MS = 86_400_000n;
export const APPLICATION_TTL_MS = 86_400_000n;
export const POLICY_NOTICE_MS = 2_592_000_000n;
export const CKUSDT_LEDGER = "cngnf-vqaaa-aaaar-qag4q-cai";
export const CKUSDC_LEDGER = "xevnm-gaaaa-aaaar-qafnq-cai";
const U128 = (1n << 128n) - 1n;

function fields(shape: Record<string, unknown>, value: unknown): void {
  ensure(
    value &&
      typeof value === "object" &&
      Object.getPrototypeOf(value) === Object.prototype,
    "INVALID_INPUT",
  );
  const record = value as Record<string, unknown>;
  const names = Object.keys(shape);
  ensure(
    Object.keys(record).length === names.length &&
      names.every((k) => Object.hasOwn(record, k)),
    "INVALID_INPUT",
  );
  for (const name of names) validateShape(shape[name] as string, record[name]);
}

/** Enforces the exact Rust wire shape, integer ranges, lengths and closed variants. */
export function validateShape(type: string, value: unknown): void {
  if (type.startsWith("Vec<")) {
    ensure(Array.isArray(value) && value.length <= 64, "INVALID_INPUT");
    for (const item of value) validateShape(type.slice(4, -1), item);
    return;
  }
  if (type.startsWith("Option<")) {
    if (value !== null) validateShape(type.slice(7, -1), value);
    return;
  }
  if (/^u(16|64|128)$/.test(type)) {
    ensure(
      typeof value === "bigint" &&
        value >= 0n &&
        value < 1n << BigInt(type.slice(1)),
      "INVALID_INPUT",
    );
    return;
  }
  if (type === "bool") {
    ensure(typeof value === "boolean", "INVALID_INPUT");
    return;
  }
  if (type === "String") {
    ensure(
      typeof value === "string" &&
        isWellFormed(value) &&
        utf8(value).length <= 4096,
      "INVALID_INPUT",
    );
    return;
  }
  if (
    ["Hash", "AccountId", "Principal", "Bytes", "ByteBuf", "OpId"].includes(
      type,
    )
  ) {
    ensure(
      value instanceof Uint8Array &&
        (type === "Hash" || type === "OpId"
          ? value.length === 32
          : type === "AccountId"
            ? value.length === 12
            : type === "Principal"
              ? value.length <= 29
              : type === "ByteBuf"
                ? value.length <= 2048
                : value.length <= 64),
      "INVALID_INPUT",
    );
    return;
  }
  const shape = (schemas as Record<string, unknown>)[type];
  ensure(shape, "UNSUPPORTED_PROTOCOL");
  if (Array.isArray(shape)) {
    if (typeof value === "string") {
      ensure(shape.includes(value), "UNSUPPORTED_PROTOCOL");
      return;
    }
    ensure(
      value && typeof value === "object" && Object.keys(value).length === 1,
      "INVALID_INPUT",
    );
    const [tag, fieldsValue] = Object.entries(value)[0]!;
    const variant = shape.find(
      (v) => typeof v === "object" && Object.hasOwn(v, tag),
    );
    ensure(variant, "UNSUPPORTED_PROTOCOL");
    fields(variant[tag], fieldsValue);
    return;
  }
  fields(shape as Record<string, unknown>, value);
}

export function validateIdentifier(value: string): void {
  ensure(
    typeof value === "string" && /^[a-z0-9-]{1,64}$/.test(value),
    "INVALID_INPUT",
  );
}

export function validateOrigin(value: string, environment: Environment): void {
  ensure(typeof value === "string", "INVALID_INPUT");
  if (value.startsWith("chrome-extension://")) {
    ensure(/^chrome-extension:\/\/[a-p]{32}$/.test(value), "INVALID_INPUT");
    return;
  }
  ensure(URL.canParse(value), "INVALID_INPUT");
  const url = new URL(value);
  const local =
    environment === "Local" &&
    url.protocol === "http:" &&
    ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  ensure(
    (url.protocol === "https:" || local) &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash &&
      url.origin === value,
    "INVALID_INPUT",
  );
}

/** Rejects the management canister and anonymous principals. */
export function validatePrincipal(value: Uint8Array): void {
  ensure(
    value.length > 0 && !(value.length === 1 && value[0] === 4),
    "AUTH_REQUIRED",
  );
}

export function validateNonzero(value: Uint8Array): void {
  ensure(
    value.some((v) => v !== 0),
    "INVALID_INPUT",
  );
}

function unique(values: unknown[], max: number, nonempty = true): void {
  ensure(
    values.length <= max && (!nonempty || values.length > 0),
    "INVALID_INPUT",
  );
  const keys = values.map((v) =>
    v instanceof Uint8Array ? [...v].join(",") : String(v),
  );
  ensure(new Set(keys).size === keys.length, "INVALID_INPUT");
}

export function validateApp(app: AppRegistration): void {
  validateShape("AppRegistration", app);
  ensure(app.version === INTEGRATION_VERSION, "UNSUPPORTED_PROTOCOL");
  validateIdentifier(app.app_id);
  ensure(app.config_version > 0n, "INVALID_INPUT");
  unique(app.origins, 16);
  unique(app.user_homes, 16);
  unique(app.cose_homes, 16);
  unique(app.product_ids, 16, false);
  unique(app.capabilities, 4);
  unique(app.profiles, 4, false);
  ensure(
    app.capabilities.includes("SignAction") ===
      app.profiles.includes("AppActionV1") &&
      app.capabilities.includes("SignDocument") ===
        app.profiles.some((p) => p !== "AppActionV1"),
    "INVALID_INPUT",
  );
  app.origins.forEach((v) => validateOrigin(v, app.environment));
  app.user_homes.forEach(validatePrincipal);
  app.cose_homes.forEach(validatePrincipal);
  app.product_ids.forEach(validateIdentifier);
  validatePrincipal(app.action_authority);
  validatePrincipal(app.authentication_receiver);
}

export function validateProduct(product: ProductRegistration): void {
  validateShape("ProductRegistration", product);
  ensure(product.version === COMMERCE_VERSION, "UNSUPPORTED_PROTOCOL");
  validateIdentifier(product.product_id);
  validateIdentifier(product.subject_schema);
  ensure(
    product.config_version > 0n &&
      product.subject_size >= 1n &&
      product.subject_size <= 64n,
    "INVALID_INPUT",
  );
  unique(product.ledgers, 2);
  for (const p of [
    product.quote_authority,
    product.beneficiary_authority,
    product.adapter,
    product.merchant.owner,
    ...product.ledgers,
  ])
    validatePrincipal(p);
  if (product.environment === "Production")
    for (const ledger of product.ledgers)
      ensure(
        [CKUSDT_LEDGER, CKUSDC_LEDGER].includes(
          Principal.fromUint8Array(ledger).toText(),
        ),
        "INVALID_INPUT",
      );
  validateNonzero(product.terms_hash);
}

function validateSubject(
  subject: BillingOffer["beneficiary"],
  product: ProductRegistration,
): void {
  ensure(
    subject.product_id === product.product_id &&
      equalBytes(subject.authority_canister, product.beneficiary_authority) &&
      subject.subject_schema === product.subject_schema &&
      BigInt(subject.subject_bytes.length) === product.subject_size,
    "INVALID_INPUT",
  );
}

export function validateBillingOffer(
  offer: BillingOffer,
  app: AppRegistration,
  product: ProductRegistration,
  now: bigint,
): void {
  validateShape("BillingOffer", offer);
  validateApp(app);
  validateProduct(product);
  ensure(offer.version === COMMERCE_VERSION, "UNSUPPORTED_PROTOCOL");
  ensure(!app.paused && !product.paused, "LOCKED");
  ensure(
    offer.environment === app.environment &&
      offer.environment === product.environment &&
      offer.app_id === app.app_id &&
      offer.product_id === product.product_id &&
      app.product_ids.includes(offer.product_id) &&
      app.capabilities.includes("Checkout") &&
      equalBytes(offer.quote_authority, product.quote_authority) &&
      equalBytes(offer.adapter, product.adapter) &&
      equalBytes(offer.product_terms_hash, product.terms_hash),
    "FORBIDDEN",
  );
  validateSubject(offer.beneficiary, product);
  validateIdentifier(offer.sku);
  ensure(
    offer.amount_usd_micros > 0n && offer.starts_at_ms < offer.expires_at_ms,
    "INVALID_INPUT",
  );
  ensure(
    offer.issued_at_ms <= now &&
      now < offer.accept_by_ms &&
      offer.accept_by_ms <= offer.expires_at_ms &&
      offer.accept_by_ms - offer.issued_at_ms <= OFFER_TTL_MS,
    "EXPIRED",
  );
  unique(offer.allowed_settlement_methods, 2);
  validateNonzero(offer.offer_id);
  validateNonzero(offer.operation_id);
}

export function validateAuthentication(
  request: AuthenticationRequest,
  app: AppRegistration,
  now: bigint,
): void {
  validateShape("AuthenticationRequest", request);
  validateApp(app);
  ensure(request.version === INTEGRATION_VERSION, "UNSUPPORTED_PROTOCOL");
  ensure(!app.paused, "LOCKED");
  ensure(
    request.environment === app.environment &&
      request.app_id === app.app_id &&
      request.app_config_version === app.config_version &&
      equalBytes(request.receiver, app.authentication_receiver) &&
      app.origins.includes(request.origin) &&
      app.capabilities.includes("Authenticate"),
    "FORBIDDEN",
  );
  ensure(
    request.issued_at_ms <= now &&
      now < request.expires_at_ms &&
      request.expires_at_ms - request.issued_at_ms <= AUTH_TTL_MS,
    "EXPIRED",
  );
  for (const v of [
    request.challenge_hash,
    request.session_key_hash,
    request.nonce,
    request.operation_id,
  ])
    validateNonzero(v);
}

export function requiredPandaStake(
  amount: bigint,
  numerator: bigint,
  denominator: bigint,
): bigint {
  for (const value of [amount, numerator, denominator])
    ensure(
      typeof value === "bigint" && value > 0n && value <= U128,
      "INVALID_INPUT",
    );
  const n = amount * numerator * 100_000_000n,
    d = 1_000_000n * denominator;
  const stake = (n + d - 1n) / d;
  ensure(stake <= U128, "QUOTA_EXCEEDED");
  return stake;
}

export function validateRatePolicy(policy: PandaRatePolicy): void {
  validateShape("PandaRatePolicy", policy);
  ensure(policy.version === COMMERCE_VERSION, "UNSUPPORTED_PROTOCOL");
  ensure(
    policy.policy_version > 0n &&
      policy.r_num > 0n &&
      policy.r_den > 0n &&
      policy.effective_at_ms - policy.published_at_ms >= POLICY_NOTICE_MS,
    "INVALID_INPUT",
  );
  unique(policy.product_ids, 16);
  policy.product_ids.forEach(validateIdentifier);
}

export function billingOfferHash(value: BillingOffer): Uint8Array {
  validateShape("BillingOffer", value);
  return digest("dmsg/commerce/offer/v2", value);
}

export function authenticationRequestHash(
  value: AuthenticationRequest,
): Uint8Array {
  validateShape("AuthenticationRequest", value);
  return digest("dmsg/authentication/request/v1", value);
}

export function applicationApprovalHash(
  value: ApplicationApproval,
): Uint8Array {
  validateShape("ApplicationApproval", value);
  return digest("dmsg/application/approval/v1", value);
}

/** Pure quotation after the caller has authenticated the product's authoritative offer. */
export function quotePanda(
  offer: BillingOffer,
  app: AppRegistration,
  product: ProductRegistration,
  policy: PandaRatePolicy,
  now: bigint,
): PandaQuote {
  validateBillingOffer(offer, app, product, now);
  validateRatePolicy(policy);
  ensure(
    offer.allowed_settlement_methods.includes("Panda") &&
      policy.environment === offer.environment &&
      policy.product_ids.includes(offer.product_id),
    "FORBIDDEN",
  );
  ensure(policy.effective_at_ms <= now, "POLICY_STALE");
  const deadline =
    now + APPLICATION_TTL_MS < offer.expires_at_ms
      ? now + APPLICATION_TTL_MS
      : offer.expires_at_ms;
  ensure(deadline - now > PANDA_COOLING_MS, "EXPIRED");
  return {
    quoted_at_ms: now,
    version: COMMERCE_VERSION,
    offer_hash: billingOfferHash(offer),
    policy: structuredClone(policy),
    required_stake_e8s: requiredPandaStake(
      offer.amount_usd_micros,
      policy.r_num,
      policy.r_den,
    ),
    application_deadline_ms: deadline,
    committed_until_ms: offer.expires_at_ms,
  };
}

export function validateApplicationApproval(
  value: ApplicationApproval,
  app: AppRegistration,
  product: ProductRegistration,
  expectedService: Uint8Array,
  now: bigint,
): void {
  validateShape("ApplicationApproval", value);
  validateApp(app);
  validateProduct(product);
  ensure(value.version === INTEGRATION_VERSION, "UNSUPPORTED_PROTOCOL");
  ensure(!app.paused && !product.paused, "LOCKED");
  ensure(
    value.app_id === app.app_id &&
      value.app_config_version === app.config_version &&
      value.environment === app.environment &&
      value.environment === product.environment &&
      app.origins.includes(value.origin) &&
      app.product_ids.includes(product.product_id) &&
      equalBytes(value.service, expectedService) &&
      app.capabilities.includes("Checkout"),
    "FORBIDDEN",
  );
  validatePrincipal(value.actor);
  validatePrincipal(value.service);
  validateSubject(value.beneficiary, product);
  ensure(
    now < value.expires_at_ms && value.expires_at_ms - now <= AUTH_TTL_MS,
    "EXPIRED",
  );
  for (const id of [
    value.action_digest,
    value.operation_id,
    value.nonce,
    value.approving_account,
  ])
    validateNonzero(id);
}

export function validateCashQuote(
  value: CashQuote,
  offer: BillingOffer,
  product: ProductRegistration,
  acceptedAt: bigint,
): void {
  validateShape("CashQuote", value);
  ensure(value.version === COMMERCE_VERSION, "UNSUPPORTED_PROTOCOL");
  ensure(
    equalBytes(value.offer_hash, billingOfferHash(offer)) &&
      offer.allowed_settlement_methods.includes("Cash") &&
      product.ledgers.some((l) => equalBytes(l, value.ledger)),
    "FORBIDDEN",
  );
  validatePrincipal(value.payer.owner);
  validatePrincipal(value.deposit.owner);
  validateNonzero(value.conversion_hash);
  ensure(
    value.amount_atomic > 0n &&
      value.fee_reserve_atomic >= value.max_network_fee_atomic &&
      value.amount_atomic + value.fee_reserve_atomic <= U128,
    "INVALID_INPUT",
  );
  ensure(
    value.funding_deadline_ms > acceptedAt &&
      value.funding_deadline_ms - acceptedAt <= CASH_FUNDING_MS &&
      value.activation_deadline_ms >= value.funding_deadline_ms &&
      value.activation_deadline_ms - acceptedAt <= CASH_ACTIVATION_MS &&
      value.activation_deadline_ms <= offer.expires_at_ms,
    "INVALID_INPUT",
  );
}

export function matchProductReceipt(
  value: ProductReceipt,
  decision: ProductDecision,
): void {
  validateShape("ProductReceipt", value);
  validateShape("ProductDecision", decision);
  ensure(
    value.version === COMMERCE_VERSION && decision.version === COMMERCE_VERSION,
    "UNSUPPORTED_PROTOCOL",
  );
  ensure(
    equalBytes(value.decision_id, decision.decision_id) &&
      equalBytes(
        value.decision_hash,
        digest("dmsg/commerce/decision/v2", decision),
      ) &&
      equalBytes(value.adapter, decision.offer.adapter) &&
      value.applied_at_ms >= decision.decided_at_ms,
    "INTEGRITY_FAILED",
  );
  if ("Applied" in value.outcome)
    ensure(
      value.outcome.Applied.committed_until_ms ===
        decision.offer.expires_at_ms &&
        value.applied_at_ms < decision.apply_by_ms,
      "INTEGRITY_FAILED",
    );
}
