import { digest, equalBytes, canonical } from "./encoding.ts";
import {
  requireValid,
  validateShape,
  validateRatePolicy,
  validateBillingOffer,
  requiredPandaStake,
  CKUSDC_LEDGER,
  CKUSDT_LEDGER,
} from "./validation.ts";
import { Principal } from "@icp-sdk/core/principal";
import type {
  AppRegistration,
  ProductRegistration,
  CheckoutRequest,
  CheckoutQuote,
  SettlementAsset,
  PandaApplicationTerms,
} from "./contracts.ts";
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
  requireValid(
    request.offer.allowed_settlement_methods.includes(request.method),
    "FORBIDDEN",
  );
  if (request.product_approval)
    requireValid(
      request.product_approval.method === request.method &&
        request.product_approval.expires_at_ms > now,
      "POLICY_STALE",
    );
}
export function validateSettlementAsset(asset: SettlementAsset, now: bigint) {
  validateShape("SettlementAsset", asset);
  requireValid(
    asset.version === 2n &&
      asset.policy_version > 0n &&
      asset.decimals === 6n &&
      asset.enabled &&
      asset.price_observed_at_ms <= now &&
      now < asset.price_valid_until_ms &&
      asset.price_valid_until_ms - asset.price_observed_at_ms <= 1_800_000n &&
      asset.price_usd_micros >= 990_000n &&
      asset.price_usd_micros <= 1_010_000n &&
      asset.network_fee_atomic > 0n &&
      asset.network_fee_atomic <= asset.max_network_fee_atomic &&
      asset.max_network_fee_atomic <= 10_000_000n,
    "POLICY_STALE",
  );
  if (asset.environment !== "Local")
    requireValid(
      Principal.fromUint8Array(asset.ledger).toText() ===
        (asset.asset === "CkUsdc" ? CKUSDC_LEDGER : CKUSDT_LEDGER),
      "FORBIDDEN",
    );
}
export async function validateCheckoutQuote(
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
  const c = quote.cash,
    asset = quote.asset,
    amount =
      (request.offer.amount_usd_micros * 1_000_000n +
        asset.price_usd_micros -
        1n) /
      asset.price_usd_micros;
  requireValid(
    request.method === "Cash" &&
      equalBytes(canonical(request.offer), canonical(quote.offer)) &&
      equalBytes(canonical(quote.product), canonical(product)) &&
      quote.quoted_at_ms <= now &&
      equalBytes(c.ledger, asset.ledger) &&
      product.ledgers.some((p) => equalBytes(p, c.ledger)) &&
      c.amount_atomic === amount &&
      c.fee_reserve_atomic === asset.max_network_fee_atomic * 2n &&
      c.max_network_fee_atomic === asset.max_network_fee_atomic &&
      equalBytes(c.payer.owner, payer) &&
      c.payer.subaccount === null &&
      equalBytes(c.deposit.owner, home) &&
      c.deposit.subaccount !== null &&
      equalBytes(c.deposit.subaccount, await checkoutId(home, request)) &&
      c.funding_deadline_ms ===
        min(quote.quoted_at_ms + 1_800_000n, request.offer.expires_at_ms) &&
      c.activation_deadline_ms ===
        min(quote.quoted_at_ms + 86_400_000n, request.offer.expires_at_ms) &&
      equalBytes(
        c.offer_hash,
        await digest("dmsg/commerce/offer/v2", request.offer),
      ) &&
      equalBytes(
        c.conversion_hash,
        await digest("dmsg/asset-policy/v2", asset),
      ),
    "INTEGRITY_FAILED",
  );
}
export async function validatePandaTerms(
  terms: PandaApplicationTerms,
  request: CheckoutRequest,
  membership: Uint8Array,
  home: Uint8Array,
  actor: Uint8Array,
  neuron: Uint8Array,
  now: bigint,
) {
  validateShape("PandaApplicationTerms", terms);
  const q = terms.quote;
  validateRatePolicy(q.policy);
  requireValid(
    q.version === 2n &&
      q.policy.effective_at_ms <= q.quoted_at_ms &&
      q.policy.product_ids.includes(request.offer.product_id) &&
      q.application_deadline_ms ===
        min(q.quoted_at_ms + 86_400_000n, request.offer.expires_at_ms),
    "INTEGRITY_FAILED",
  );
  requireValid(
    request.method === "Panda" &&
      equalBytes(canonical(terms.offer), canonical(request.offer)) &&
      equalBytes(terms.home_membership, membership) &&
      equalBytes(terms.user_home, home) &&
      equalBytes(terms.actor, actor) &&
      equalBytes(terms.neuron_id, neuron) &&
      equalBytes(terms.approving_account, request.approving_account) &&
      q.quoted_at_ms <= now &&
      now < q.application_deadline_ms &&
      q.committed_until_ms === request.offer.expires_at_ms &&
      q.required_stake_e8s ===
        requiredPandaStake(
          request.offer.amount_usd_micros,
          q.policy.r_num,
          q.policy.r_den,
        ) &&
      equalBytes(
        q.offer_hash,
        await digest("dmsg/commerce/offer/v2", request.offer),
      ),
    "INTEGRITY_FAILED",
  );
}
const min = (a: bigint, b: bigint) => (a < b ? a : b);
