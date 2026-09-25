# Independent account product adapter

This minimal product uses `sample-account-v1` subjects with twelve opaque bytes.
They are **not** dMsg AccountIds: its administrator assigns native product owners,
and its operator approval is checked separately from the dMsg approving account
and economic wallet. It uses the same generic checkout and membership services
as dMsg's own account product and TokenList's project product.

No product name, account schema or role was added to the checkout ledger or
membership occupancy engine for this example.

1. Install with explicit service IDs, environment, app/product IDs, USD annual
   amount, terms hash and administrator. The deployment has no production defaults.
2. Register `ProductRegistration` with this canister as quote authority,
   beneficiary authority and adapter, `subject_schema = sample-account-v1`,
   `subject_size = 12` and explicit merchant/ledgers.
3. Register its exact app origins, dMsg homes and `Checkout` capability. Add its
   product ID to a published PANDA rate policy.
4. `assign_account` binds at most 100 sample subjects to product owners.
   `prepare_billing_offer` requires that owner and freezes USD terms plus a
   `ProductApproval`. A new product should connect these checks to its real
   account and role model.
5. Send the returned offer/approval to `@dmsg/sdk` checkout with the expected
   dMsg approving account. The economic payer/neuron actor can be different
   from the product owner.
6. Both service callbacks use `ProductBook`: one reserved interval, durable
   Apply identity, no expiry eviction of unknown Apply, current ownership/CAS,
   atomic contract plus original receipt, and cash-only unstarted cancellation.
7. Consume `entitlement`, which refreshes bounded PANDA qualification and does
   not grant paid rights from an expired cache. Keep base access separate in
   the real product.

`lose_next_apply_ack` is available only for Local deployments and the configured
administrator. It commits before losing a reply, so PocketIC can test recovery
through `get_product_decision` and upgrade. It grants no permission and changes
no business outcome. This example is deliberately limited, not a complete
account/authentication product or a deployment recommendation.

Build and run the actual reference tests from repository root:

```sh
cargo build --locked --release --target wasm32-unknown-unknown \
  -p dmsg_account_product -p dmsg_user -p dmsg_commerce -p membership \
  -p dmsg_test_ledger -p dmsg_test_sns
cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane commerce:: -- --test-threads=1
```

The matrix includes shared cash/PANDA interval competition, a lost Apply ACK,
upgrade recovery, a neuron shared across two different products being refused,
and two contiguous same-beneficiary commitments retaining the second reference
when the first expires. The default example does not transfer SNS custody.
