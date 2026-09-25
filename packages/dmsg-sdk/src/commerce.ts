import { Principal } from "@icp-sdk/core/principal";
import type {
  AppRegistration,
  ProductRegistration,
  CashQuote,
  CheckoutRequest,
  CheckoutQuote,
  SettlementAsset,
  PandaApplicationTerms,
} from "./contracts.ts";
import { ensure } from "./cose-errors.ts";
import { digest, equalBytes, canonical } from "./encoding.ts";
import {
  CASH_ACTIVATION_MS,
  CASH_FUNDING_MS,
  CKUSDC_LEDGER,
  CKUSDT_LEDGER,
  COMMERCE_VERSION,
  billingOfferHash,
  quotePanda,
  validateBillingOffer,
  validateShape,
} from "./validation.ts";

export const PRICE_WINDOW_MS = 1_800_000n;
export const MAX_DEPEG_USD_MICROS = 10_000n;
const min = (a: bigint, b: bigint) => (a < b ? a : b);
const same = (a: unknown, b: unknown) =>
  equalBytes(canonical(a), canonical(b));

export const checkoutId = (home: Uint8Array, request: CheckoutRequest) =>
  digest("dmsg/checkout/order/v2", [
    home,
    request.offer.app_id,
    request.offer.operation_id,
  ]);

export const checkoutQuoteHash = (quote: CheckoutQuote) =>
  digest("dmsg/checkout/quote/v2", quote);

export const pandaApplicationHash = (terms: PandaApplicationTerms) =>
  digest("dmsg/panda/application/v2", terms);

export const pandaClaimId = (terms: PandaApplicationTerms) =>
  digest("dmsg/panda/claim/v2", [
    terms.home_membership,
    terms.offer.app_id,
    terms.offer.operation_id,
  ]);

export function validateCheckoutRequest(
  request: CheckoutRequest,
  app: AppRegistration,
  product: ProductRegistration,
  now: bigint,
) {
  validateShape("CheckoutRequest", request);
  validateBillingOffer(request.offer, app, product, now);
  ensure(
    request.offer.allowed_settlement_methods.includes(request.method),
    "FORBIDDEN",
  );
  if (request.product_approval)
    ensure(
      request.product_approval.method === request.method &&
        request.product_approval.expires_at_ms > now,
      "POLICY_STALE",
    );
}

export function validateSettlementAsset(asset: SettlementAsset, now: bigint) {
  validateShape("SettlementAsset", asset);
  ensure(
    asset.version === COMMERCE_VERSION && asset.policy_version > 0n,
    "UNSUPPORTED_PROTOCOL",
  );
  ensure(
    asset.decimals === 6n &&
      asset.price_usd_micros > 0n &&
      asset.network_fee_atomic > 0n &&
      asset.network_fee_atomic <= asset.max_network_fee_atomic &&
      asset.max_network_fee_atomic <= 10_000_000n &&
      asset.price_valid_until_ms > asset.price_observed_at_ms &&
      asset.price_valid_until_ms - asset.price_observed_at_ms <=
        PRICE_WINDOW_MS,
    "INVALID_INPUT",
  );
  if (asset.environment !== "Local")
    ensure(
      Principal.fromUint8Array(asset.ledger).toText() ===
        (asset.asset === "CkUsdc" ? CKUSDC_LEDGER : CKUSDT_LEDGER),
      "FORBIDDEN",
    );
  ensure(
    asset.enabled &&
      asset.price_observed_at_ms <= now &&
      now < asset.price_valid_until_ms &&
      asset.price_usd_micros >= 1_000_000n - MAX_DEPEG_USD_MICROS &&
      asset.price_usd_micros <= 1_000_000n + MAX_DEPEG_USD_MICROS,
    "POLICY_STALE",
  );
}

/** Rebuild the complete cash quote as the commerce canister does, then require identical terms. */
export function validateCheckoutQuote(
  quote: CheckoutQuote,
  request: CheckoutRequest,
  app: AppRegistration,
  product: ProductRegistration,
  home: Uint8Array,
  payer: Uint8Array,
  now: bigint,
) {
  validateShape("CheckoutQuote", quote);
  validateCheckoutRequest(request, app, product, now);
  validateSettlementAsset(quote.asset, now);
  const { asset, quoted_at_ms: at } = quote,
    offer = request.offer;
  ensure(
    request.method === "Cash" &&
      asset.environment === offer.environment &&
      product.ledgers.some((l) => equalBytes(l, asset.ledger)),
    "FORBIDDEN",
  );
  const cash: CashQuote = {
    version: COMMERCE_VERSION,
    offer_hash: billingOfferHash(offer),
    ledger: asset.ledger,
    amount_atomic:
      (offer.amount_usd_micros * 10n ** asset.decimals +
        asset.price_usd_micros -
        1n) /
      asset.price_usd_micros,
    conversion_hash: digest("dmsg/asset-policy/v2", asset),
    payer: { owner: payer, subaccount: null },
    deposit: { owner: home, subaccount: checkoutId(home, request) },
    max_network_fee_atomic: asset.max_network_fee_atomic,
    fee_reserve_atomic: asset.max_network_fee_atomic * 2n,
    funding_deadline_ms: min(at + CASH_FUNDING_MS, offer.expires_at_ms),
    activation_deadline_ms: min(at + CASH_ACTIVATION_MS, offer.expires_at_ms),
  };
  ensure(
    at <= now &&
      same(quote, { product, offer, cash, asset, quoted_at_ms: at }),
    "INTEGRITY_FAILED",
  );
}

/** Rebuild the PANDA quote as the membership canister does, then bind this account and neuron. */
export function validatePandaTerms(
  terms: PandaApplicationTerms,
  request: CheckoutRequest,
  app: AppRegistration,
  product: ProductRegistration,
  membership: Uint8Array,
  home: Uint8Array,
  actor: Uint8Array,
  neuron: Uint8Array,
  now: bigint,
) {
  validateShape("PandaApplicationTerms", terms);
  validateCheckoutRequest(request, app, product, now);
  const q = terms.quote;
  const expected = quotePanda(
    terms.offer,
    app,
    product,
    q.policy,
    q.quoted_at_ms,
  );
  ensure(
    request.method === "Panda" &&
      same(terms.offer, request.offer) &&
      same(q, expected) &&
      equalBytes(terms.home_membership, membership) &&
      equalBytes(terms.user_home, home) &&
      equalBytes(terms.actor, actor) &&
      equalBytes(terms.neuron_id, neuron) &&
      equalBytes(terms.approving_account, request.approving_account) &&
      q.quoted_at_ms <= now &&
      now < q.application_deadline_ms,
    "INTEGRITY_FAILED",
  );
}
