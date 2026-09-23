import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { IDL } from "@icp-sdk/core/candid";
import { Principal } from "@icp-sdk/core/principal";
import {
  canonical,
  decodeCanonical,
  validateShape,
  validateCheckoutQuote,
  validatePandaTerms,
  validateSettlementAsset,
  fromCandid,
  toCandid,
  type CheckoutQuote,
  type CheckoutRequest,
  type PandaApplicationTerms,
  type AppRegistration,
} from "../src/index.ts";
const file = (name: string) =>
  JSON.parse(
    readFileSync(
      new URL(
        `../../../src/dmsg_types/tests/${name}_vectors.json`,
        import.meta.url,
      ),
      "utf8",
    ),
  );
const fixtures = file("commerce"),
  integration = file("integration");
const read = (values: any[], name: string) =>
  decodeCanonical(
    Buffer.from(values.find((v) => v.name === name).cbor_hex, "hex"),
  ) as any;
const quote = read(fixtures, "checkout_terms_v2")[2] as CheckoutQuote,
  terms = read(fixtures, "panda_application_v2")[2] as PandaApplicationTerms,
  app = read(integration, "app") as AppRegistration;
const request: CheckoutRequest = {
  offer: quote.offer,
  approving_account: terms.approving_account,
  product_approval: null,
  method: "Cash",
};
const now = quote.quoted_at_ms;

test("Rust checkout terms independently match fixed asset, full amount and all deadlines", async () => {
  validateShape("CheckoutRequest", request);
  await validateCheckoutQuote(
    quote,
    request,
    app,
    quote.product,
    quote.cash.deposit.owner,
    quote.cash.payer.owner,
    now,
  );
  for (const alter of [
    (v: CheckoutQuote) => v.cash.amount_atomic++,
    (v: CheckoutQuote) => v.cash.fee_reserve_atomic++,
    (v: CheckoutQuote) => (v.cash.deposit.subaccount![0] ^= 1),
    (v: CheckoutQuote) => (v.cash.ledger = new Uint8Array([99, 1])),
    (v: CheckoutQuote) => v.cash.funding_deadline_ms++,
    (v: CheckoutQuote) => (v.product.merchant.owner = new Uint8Array([99, 1])),
  ]) {
    const changed = structuredClone(quote);
    alter(changed);
    await assert.rejects(
      validateCheckoutQuote(
        changed,
        request,
        app,
        quote.product,
        quote.cash.deposit.owner,
        quote.cash.payer.owner,
        now,
      ),
    );
  }
  assert.throws(() =>
    validateSettlementAsset(
      { ...quote.asset, price_usd_micros: 900_000n },
      now,
    ),
  );
  assert.throws(() =>
    validateSettlementAsset(quote.asset, quote.asset.price_valid_until_ms),
  );
});
test("PANDA full waiver binds neuron, independent account, exact USD bill, budget and original end", async () => {
  const request: CheckoutRequest = {
    offer: terms.offer,
    approving_account: terms.approving_account,
    product_approval: null,
    method: "Panda",
  };
  const verify = (v: PandaApplicationTerms) =>
    validatePandaTerms(
      v,
      request,
      quote.product,
      terms.home_membership,
      terms.user_home,
      terms.actor,
      terms.neuron_id,
      now,
    );
  await verify(terms);
  for (const alter of [
    (t: PandaApplicationTerms) => (t.neuron_id[0] ^= 1),
    (t: PandaApplicationTerms) => (t.approving_account[0] ^= 1),
    (t: PandaApplicationTerms) => t.quote.required_stake_e8s--,
    (t: PandaApplicationTerms) => t.quote.committed_until_ms--,
    (t: PandaApplicationTerms) => t.quote.subsidy_usd_micros--,
    (t: PandaApplicationTerms) => (t.quote.policy.subsidy_budget_id[0] ^= 1),
  ]) {
    const v = structuredClone(terms);
    alter(v);
    await assert.rejects(verify(v));
  }
  assert.throws(() =>
    validateShape("CheckoutRequest", { ...request, discount_bps: 100n }),
  );
});
test("generated Candid conversion preserves empty options, empty vectors, bytes, principals and wide integers", () => {
  const type = IDL.Record({
    owner: IDL.Principal,
    sub: IDL.Opt(IDL.Vec(IDL.Nat8)),
    empty: IDL.Vec(IDL.Nat8),
    version: IDL.Nat16,
    amount: IDL.Nat,
    state: IDL.Variant({ Ready: IDL.Null }),
  });
  const wire = {
    owner: Principal.fromText("aaaaa-aa").toUint8Array(),
    sub: null,
    empty: new Uint8Array(),
    version: 2n,
    amount: (1n << 127n) + 5n,
    state: "Ready",
  };
  const value = IDL.decode(
    [type],
    IDL.encode([type], [toCandid(type, wire)]),
  )[0];
  assert.deepEqual(fromCandid(type, value), wire);
  assert.deepEqual(decodeCanonical(canonical(fromCandid(type, value))), wire);
});
