# dMsg commerce 2

Status: implemented development contracts. Production configuration and real-fund
acceptance must be recorded separately; local fixtures do not establish either.
See [integration](integration.md), [app actions](app-action.md) and
[browser v4](browser-v4.md) for their independent authentication boundaries.

## Authorities and identities

The product owns its account/project, prices, role checks, subscription contracts
and delivery receipts. dMsg user verifies the exact approving account/device;
commerce owns cash settlement; membership owns neuron qualification, global
occupancy and full-waiver commitments. A Worker or browser is never a payment or
product-delivery authority.

`Beneficiary = (product_id, authority_canister, subject_schema, subject_bytes)`.
A twelve-byte subject is not implicitly a dMsg AccountId. dMsg's own account
adapter interprets `dmsg-account-v1`; TokenList's adapter interprets an eight-byte
big-endian project ID. The independent `sample-account-v1` adapter has its own
owners. Product operator, dMsg approving account and economic wallet are checked
separately and may be different.

An authoritative `BillingOffer` fixes the complete USD-micro amount, `[S,E)`, SKU,
product terms hash, business revision, operation ID, quote authority and adapter.
Only the registered quote authority can establish these terms. A browser's USD
amount or deadline cannot replace them. Quotes do not reserve intervals or
consume successful order/claim capacity. Account approval and product approval
must succeed before admission.

## Versions, encoding and commitments

All hashes are SHA-256 of deterministic CBOR `(1, domain, value)`. Integers use
explicit atomic units, USD micros, milliseconds or nanoseconds; no floats.
Types/CDDL/SDK are generated from `integration*.rs`. Unknown fields and versions
fail closed.

| Value | Domain |
| --- | --- |
| Complete product bill | `dmsg/commerce/offer/v2` |
| Complete cash quote | `dmsg/checkout/quote/v2` |
| Cash order identity | `dmsg/checkout/order/v2`, `(home, app_id, operation_id)` |
| Complete PANDA application | `dmsg/panda/application/v2` |
| PANDA claim identity | `dmsg/panda/claim/v2`, `(home, app_id, operation_id)` |
| Product authorization | `dmsg/product-authorization/v2` |
| App configuration leaf | `dmsg/registration/app/v1` |
| Product configuration leaf | `dmsg/registration/product/v2` |
| Cash order leaf | `dmsg/checkout/certificate/v2` |
| Outgoing transfer leaf | `dmsg/checkout/transfer-certificate/v2` |
| Asset policies leaf | `dmsg/settlement-assets/v2`, `"supported"` |
| PANDA claim leaf | `dmsg/panda/claim-certificate/v2` |

The original resource catalog/entitlement and execution-usage leaf domains remain
version 1 because they describe dMsg resources, not the removed commerce-1 order
or membership-change interfaces. No v1 commercial fallback is provided.

## Cash checkout

Only ckUSDT `cngnf-vqaaa-aaaar-qag4q-cai` and ckUSDC
`xevnm-gaaaa-aaaar-qafnq-cai` are accepted outside Local. Both require six decimals,
verified ICRC-1/ICRC-3 support and an actual supported transfer block. Asset
identity comes from the fixed ledger, never its symbol. Governance registers the
policy and verifies ledger metadata; a separately configured price authority may
publish a fresh USD-micro reference observation. New quotes require an enabled
asset, a price window of at most 30 minutes and deviation of at most 1% from one
USD. There is no implicit dollar peg.

`amount_atomic = ceil(amount_usd_micros * 10^decimals / price_usd_micros)` with
arbitrary-width intermediates and one final rounding. `CheckoutQuote` freezes
merchant/product registration, payer, ledger, price, deposit subaccount, maximum
network fee, fee reserve, funding and activation deadlines. Incoming transfer
fees are separate. The client shows the exact price/reserve/fee before payment.
A later price publication does not invalidate an accepted quote: `open_checkout`
keeps the quoted observation while it is still valid, provided the current
observation is also available and every non-price asset term (ledger, kind,
decimals, fees and enablement) is unchanged. The quoted `(ledger, policy_version)`
must exactly match an authority-published snapshot retained in stable memory;
client-supplied price/time/version fields are never treated as evidence. Each
ledger retains at most 256 live snapshots; publication removes expired snapshots
and rejects a full live history instead of evicting still-valid quotes.

1. `quote_checkout` verifies the authoritative offer and selected asset.
2. The user's device signs a purpose-separated `ApplicationApproval`; the product
   authorizes the same beneficiary/offer/operator.
3. `open_checkout` persists `Reserving`, asks the product to reserve the interval,
   then reaches `AwaitingFunding`. No money is moved by opening.
4. The wallet persists the original ledger, account, memo, amount, nanosecond time
   and fee before its one transfer. Unknown never creates another payment.
5. `check_checkout_funding` reads the ledger and only follows archive callbacks
   authenticated by that ledger. `(ledger, block_index)` is the deduplication key.
6. A qualifying one-shot payment writes balances and an immutable Apply decision
   before any adapter await. The adapter commits its contract and receipt in one
   message. `reconcile_checkout` reads/replays that original decision after a lost
   ACK; a timeout alone cannot authorize a refund.

Only the selected asset, expected payer and timely sufficient deposit can fund
the price. Wrong-ledger, wrong-source, partial, extra, duplicate and late money
remains a refund obligation to its actual source. No synthetic payment reference
is produced for PANDA.

Each order owns separate balances **for each ledger**:

`incoming = refundable + service_reserve + fee_reserve + outgoing`.

A merchant can collect only its frozen order's earned portion, beginning at
`max(actual_delivery, S)`. Another merchant's reserves and another ledger's
assets cannot fund it. `claim_checkout_refund` aggregates caller-selected real
deposits from the same ledger/source; the destination cannot be replaced.
Cash cancellation requires a durable adapter receipt proving that the contract
has not started or issued rights. Started cash service has no arbitrary-refund
promise in this protocol.

Outgoing legs freeze their own source, ledger, recipient, net amount, fee, memo
and timestamp. `Unknown`/`InFlight` is resolved by the same transfer or an exact
trusted ledger block. Only a known rejection may be superseded: the recipient
explicitly reissues it with a fresh ledger timestamp, keeping its fee or adopting
the ledger's expected fee within the **original** maximum fee. This also recovers
a leg first dispatched after the ledger's 24-hour deduplication window. The gross
obligation stays fixed. An over-cap fee remains blocked; it is not
silently taken from someone else's principal or reserve. A live per-leg guard
excludes concurrent dispatch, reconciliation and replacement, regardless of how
long the first callback takes. Upgrades discard guards but retain `InFlight` and
its frozen arguments; only the ledger result can resolve that uncertainty.

## PANDA full-fee commitments

`required_stake_e8s = ceil(amount_usd_micros * R_num * 100_000_000 /
(1_000_000 * R_den))`. The amount is the full accepted interval price. Duration
is not divided into an annual rate or prorated stake. Zero/overflow fails;
PANDA never becomes a discount basis-point value, partial payment, cash balance
or withdrawable rebate. Customer cash principal is zero.

The immutable quote records rate policy/version, quoted time, required stake,
application deadline and original E. Ordinary R publication has at least 30
days' notice. There is no USD subsidy budget: admission is bounded by the
service's concurrent claim capacity and hourly application limit. New admission
reserves the global `(SNS governance, neuron_id)` occupancy before qualification.

New policy versions strictly increase, including after superseded records are
pruned. Retrying a retained version with identical business fields returns its
original publication timestamp; a retry never starts a new notice interval.

The service verifies the pinned SNS root/governance/ledger, eight decimals and,
when pinned (required outside Local), the approved governance Wasm hash and SNS
root as its sole controller via `canister_info`. Unknown permission semantics
or unverifiable module/response data is `Unverifiable`. The economic actor must
hold the relevant native economic permissions, with no other economic
controller; voting permission alone is insufficient. Net principal stake must
meet the quote, and the earliest possible unlock must not precede E, so the
neuron stays locked for the whole paid interval `[start, E)`. This is
qualification, not physical SNS custody. A claim reuses an observation for one
minute before querying SNS again.

The first **actual eligible observation** starts at least 65 minutes of cooling.
A fresh account/device and product approval is required after cooling; it covers
the original terms and a neuron observation starting at or after the cooling
deadline. The one-minute cache cannot reuse a pre-deadline observation for Apply.
A timeout does not start or
shorten cooling. At most current and contiguous next-term references can occupy
a neuron, and both must name the same beneficiary. Different products cannot
reuse that neuron concurrently. One service-global occupancy table enforces this.

Before Apply, an unused application may cancel. Once Apply is prepared, Unknown
cannot cancel or release on a timer. After successful Apply, including a future
renewal, `committed_until_ms = original E` is irreversible. No Upgrade, Replace,
Buyout, early Close or governance edit shortens it. Product closure, unlinking,
loss of qualification and termination of rights do not release occupancy or
claim capacity early. Expiry releases each reference exactly once;
releasing the current term does not remove an already committed next term.

Qualification leases last at most one hour and never pass E. Known ineligibility
stops new rights and starts a seven-day repair clock; `Unverifiable` pauses that
clock and never extends an old lease. Repair expiry permanently terminates
rights while retaining the commitment until E. A terminated term never revives.
`refresh_panda_claim`/`reconcile_panda_claim` preserve the original claim/decision;
only initial/new post-cooling approval can create a new device approval.

Temporary product reservation failures preserve the accepted application for
reconciliation. Reserve, receipt lookup, Apply and release each consume the
product-call budget only when dispatched: 200 per UTC minute globally and 10 per
economic actor. One in-flight product call sequence is allowed per claim; an
overlapping retry returns `Pending`. Admission pause does not disable recovery.
Only one SNS configuration verification is dispatched at a time; concurrent
callers receive `Pending` and can reuse its cached result on retry.

Full terminal records are retained for 30 days after the later of the terminal
transition and application deadline. The bounded commitment sweep also compacts
up to 32 due terminal records, retaining the immutable operation digest. Its
return value counts released commitments, not compacted history. Compacted
claims return `ResultExpired`; same-ID/different-terms retries remain conflicts.
Open commitments and unknown Apply decisions are not compacted. Admission caps
full records at 100,000 and total retained operation identities at 1,000,000;
these bounds are not production capacity claims. Operations pagination uses
reader indexes and includes a cursor only when more accessible full records
remain. Archived claim certificate leaves are removed.

## Product adapter and bounded rights

Both cash and PANDA use `ProductBook`, a business-revision CAS and a single
uncommitted interval reservation. It never prunes an unknown Apply. Products
implement `verify_billing_offer`, `authorize_product_billing`,
`reserve_product_billing`, `release_product_billing`, `apply_product_decision`,
`get_product_decision` and the two cash-cancellation methods. Each callback pins
its settlement caller, complete offer, selected source and original receipt.
Current role/subject and business revision checks run after external awaits.
Lease observations advance a separate lease revision, not the business CAS.

`get_checkout_for_product` and `get_panda_claim_for_product` provide restricted
consensus source reads. They do not recursively refresh/apply the decision that
called the adapter. A receipt is immutable and idempotent, including a definite
rejection. No receipt means Unknown, not nondelivery.

TokenList retains its first-settlement Included period, annual tier/renewal
rules and compliance access. Its external subscription history records dMsg
order/claim and delivery references without increasing the native FeeLedger's
withdrawable balances. A future cash cancellation restores the prior calendar
cursor. A bounded current source lease is required for added paid features;
public records, periodic reports and material-change disclosure stay available.
The Worker asks the registry for a replicated role/lease decision before buying
paid drafting; browser/session claims cannot authorize the charge themselves.

The dMsg account product uses the same services. Its base plan and storage
resources are projections of accepted contracts. Cash-only upgrades retire the
old cash resource view at delivery; they cannot exit a PANDA commitment. An
upgrade keeps the original annual term, and upgrade differences and storage
add-ons are priced for the rest of that full term. Expired add-ons do not count
toward the 64 retained add-ons. Unverifiable and known-ineligible periods both
pause paid execution time; a known-ineligible view carries its repair deadline. Monthly
execution allowance integrates the complete nonoverlapping UTC-month timeline,
clips time before account creation and rounds down once. Free quota segments
follow the catalogs actually effective during the month, with no retroactive
replacement of earlier segments. A catalog boundary ends the previous resource
lease and a refreshed projection advances its lease revision. Only current-month
and live/future resource intervals stay in the subject's working history; the
last service-end time is retained for the expired resource projection. Qualification refresh,
refund or downgrade does not reset held/charged units. Root recovery/derivation
continues to use protected safety budgets, independent of paid execution units.

## Operation recovery and deployment

Private paginated `checkout_operations`, `checkout_transfers` and
`panda_operations` reuse existing reader/owner/merchant/recipient/adapter or
governance permissions, including the merchant frozen in the order. Stable
reader indexes yield only accessible records and return a cursor only when more
accessible records remain. `checkout_deposits(order_id, after, take)` returns up
to 128 deposits with an opaque hash cursor. These pages show per-ledger obligations,
Unknown decisions, fee blockage, expired leases and retained commitments. These
observations are not delivery receipts. The extension's operations centre never
offers to clear Unknown records or end an applied PANDA commitment.

Funding/product/outgoing work has a global allowance of 400 per UTC minute and
40 per caller; ledger reads include their bounded archive traversal. Authorization
is separately limited to 200 global and 10 per caller. Qualification refreshes
allow 200 global and 20 per public caller; registered user homes share the global
limit when serving their accounts. Each actual product call consumes an allowance,
including both receipt lookup and a needed Apply. Local refund allocation,
unchanged terminal reads and blocked concurrent retries do not consume this RPC
budget. At most 128 guarded operations may be in flight.

`sweep_checkout_history` archives up to 32 orders and 32 outgoing legs per call.
An order must have no remaining reserve/refund obligation or unfinished transfer,
and a definite product outcome/released reservation. Retention lasts 30 days
after the later of its last money transition and service/activation endpoint.
Succeeded or superseded transfer legs retain their full arguments for at least
30 days after their last transition. Unknown/Rejected/Pending transfers remain
live. Orders in cold storage discard redundant authorization/decision payloads,
but retain the original input digest, quote, ledger balances, final receipt and
funding reference. Ledger/block deduplication and reader indexes are never removed.
Direct reads and indexed history remain available; retired certificate endpoints
return `ResultExpired`. A newly verified late deposit restores the original
order's live certificate and its original-source refund route without reapplying
service. Admission allows 100,000 hot orders and 1,000,000 total order identities;
these are hard safeguards, not measured production capacities.

Pausing admission preserves original-order reconciliation, original-source
refunds, known transfer recovery, existing qualification refresh and expiry
release. Never reinstall accepted financial/commitment state to resolve an error.
Development schemas initialize fresh; there is no old commercial API migration
layer. Keep original records when coordinating a release of already accepted
operations.

`make test-dmsg` exercises native rules, actual Wasm, codec vectors and the SDK.
The independent [account product](../../examples/dmsg-account-product/README.md)
exercises a second subject type without core changes. TokenList's
`scripts/acceptance-dmsg.sh` adds actual registry/project callbacks and a protected
session/action/signature/commit path. Real browser probes use disposable profiles;
production wallet origins, neuron control, approved R/budgets and bounded real
fund transfers require their own deployment evidence.

The separate message-delivery service retains delivery profile 2 and its
`dmsg/quote/v2` / `dmsg/admission-receipt/v2` contracts. It is not a subscription
fallback or a path for early PANDA exit.
