# dmsg_commerce

Merchant subscriptions, independent storage products, SNS product adapter, short resource leases and execution-month timelines. See the [public contract](../../docs/protocol/commerce.md), [types](../dmsg_types/src/billing.rs), and [Candid](dmsg_commerce.did).

Cash uses one configured six-decimal ledger with the DFINITY ICRC-3 transfer/archive adapter. Governance confirms ledger metadata before opening orders. Order subaccounts isolate funds. Reconciliation classifies every deposit and sends refunds to the proven original source. The accounting assertion is:

```
confirmed_in = service_reserve + earned + refundable + fee_reserve
             + outgoing + transferred + network_fees
```

Subscription activation is atomic with the incoming funds decision. Outgoing legs retain their exact parameters across Unknown and upgrades. `collect_revenue` is governance restricted and consumes only released earned balances. Qualification face value never enters cash accounting.

Catalog versions, cash terms, PANDA decisions and their outcomes are fixed. A source entering Closing issues no new leases; refunds wait for the maximum outstanding lease deadline. Old execution charges remain in the user canister. Only configured user homes may request replicated execution entitlement or register an account creation time.

The default plan helper is public, but init takes an explicit catalog. Policy publication requires thirty days' notice. Admission pause does not withdraw active commitments. `refresh_catalog` publishes scheduled transitions for certified consumers; `refresh_entitlement` never creates arbitrary unknown subjects. Existing users can obtain their first Free projection through `dmsg_user.refresh_execution_entitlement`.
