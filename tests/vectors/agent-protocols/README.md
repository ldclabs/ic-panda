# Agent Protocols vectors

Normative vectors and schemas of [agent-protocols](https://github.com/ldclabs/agent-protocols)
(MIT), copied unchanged from tag `v0.12.0` (`85abd0b`); SDK 0.12.1, which the
workspace uses, left them and the five test files as they were. `src/dmsg_agent/tests/` runs the SDK's own conformance tests
for Identity, Delegation, Profile, Agent Mail and Agent Knowledge against
them, and `discourse_vectors.rs` checks the Discourse record chain, so an SDK
upgrade that changes a byte rule fails here first. The Mail vectors come from
the protocol's cross-language generator, so passing them is the Rust half of
DT08; the desktop test `desktop_mail_knowledge` adds packets this SDK seals
and the TypeScript relay accepts.

| File | Source |
| --- | --- |
| `agent-identity-1.0.vectors.json` | `docs/protocols/agent-identity/1.0.vectors.json` |
| `agent-delegation-1.0.vectors.json` | `docs/protocols/agent-delegation/1.0.vectors.json` |
| `agent-profile-1.0.vectors.json` | `docs/protocols/agent-profile/1.0.vectors.json` |
| `agent-discourse-1.0.vectors.json` | `docs/protocols/agent-discourse/1.0.vectors.json` |
| `agent-mail-1.0.vectors.json`, `agent-mail-1.0.schema.json` | `docs/protocols/agent-mail/1.0.{vectors,schema}.json` |
| `agent-knowledge-1.0.vectors.json`, `agent-knowledge-1.0.schema.json` | `docs/protocols/agent-knowledge/1.0.{vectors,schema}.json` |

Upgrading the SDK: copy these files and the five test files
(`crates/agent-protocols/tests/{identity_vectors,delegation_conformance,profile_vectors,mail_vectors,knowledge_vectors}.rs`,
with the vector and schema paths pointed here; the Mail and Knowledge ones
also allow `clippy::result_large_err`, which this workspace denies) from the
new tag, then run `cargo test -p dmsg_agent`.
