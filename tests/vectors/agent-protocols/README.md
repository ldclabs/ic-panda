# Agent Protocols vectors

Normative vectors of [agent-protocols](https://github.com/ldclabs/agent-protocols)
(MIT), copied unchanged from tag `v0.11.4` (`fc85a4b`), the SDK version the
workspace pins. `src/dmsg_agent/tests/` runs the SDK's own conformance tests
for Identity, Delegation and Profile against them, and `discourse_vectors.rs`
checks the Discourse record chain, so an SDK upgrade that changes a byte
rule fails here first.

| File | Source |
| --- | --- |
| `agent-identity-1.0.vectors.json` | `docs/protocols/agent-identity/1.0.vectors.json` |
| `agent-delegation-1.0.vectors.json` | `docs/protocols/agent-delegation/1.0.vectors.json` |
| `agent-profile-1.0.vectors.json` | `docs/protocols/agent-profile/1.0.vectors.json` |
| `agent-discourse-1.0.vectors.json` | `docs/protocols/agent-discourse/1.0.vectors.json` |

Upgrading the SDK: copy the four files and the three test files
(`crates/agent-protocols/tests/{identity_vectors,delegation_conformance,profile_vectors}.rs`,
with the vector paths pointed here) from the new tag, then run
`cargo test -p dmsg_agent`.
