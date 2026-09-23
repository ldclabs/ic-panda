# dMsg chain-key library

Workspace Rust implementation maintained in `ic-panda` for dMsg. It exports no
Candid entry points and owns no namespace, account, authorization, or storage.

The crate was initially derived from
[`ldclabs/ic-cose`](https://github.com/ldclabs/ic-cose) commit
`eccf135fec02fa804347d6efd15f800cbd3b4d8a` under the retained MIT/Apache-2.0
licenses. It now evolves independently in this repository; upstream changes are
evaluated and applied explicitly rather than synchronized as a byte-for-byte
snapshot.

- `Operation`: construct once, estimate the request payment + outgoing call reservation, persist the
  authorized execution and budget, then consume the operation with `execute`.
- `PublicKey`: retain the public key and chain code for offline child derivation.
- `classify_failure`: preserve unsent, rejected, and unknown outcomes. Never retry
  an unknown outcome by silently issuing another signing or derivation call.
- vetKD accepts exact context/input bytes. Each caller owns its domain encoding.

`Cost` includes the management request payment and `cost_call` reservation for
the maximum response and callback. `cost_upper_bound` subtracts refunded attached
cycles, retaining the conservative call reservation; an unsent call returns zero.
These bounds are not actual bills and exclude other instructions, storage and
query costs.
Persist request deduplication before awaiting management calls. A consumed Rust
value alone does not provide durable deduplication or exactly-once execution.
