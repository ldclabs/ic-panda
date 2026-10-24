# Agent Protocols vectors

Normative vectors and schemas of [agent-protocols](https://github.com/ldclabs/agent-protocols)
(MIT), adopted from SDK **0.12.6**, source commit `13257c1`.
The eight normative JSON files are byte-for-byte copies; `source.json` records
the adopted version, commit and SHA-256 digests, and `LICENSE` retains the MIT
notice. The Rust and TypeScript dependency declarations and Rust lockfile must
agree with that version.

`src/dmsg_agent/tests/` runs the upstream Identity, Delegation, Profile, Mail
and Knowledge suites, the Discourse record-chain check and the new shared
protocol-boundary suite. These cover strict HTTP JSON, the inclusive replay
window, bounded nonce recovery, current delegation status, durable Profile
pins, portable whitespace/patterns and atomic type imports. Host tests add
actual HTTP/TLS persistence and restart behavior.

| File | Source |
| --- | --- |
| `agent-identity-1.0.vectors.json` | `docs/protocols/agent-identity/1.0.vectors.json` |
| `agent-delegation-1.0.vectors.json` | `docs/protocols/agent-delegation/1.0.vectors.json` |
| `agent-profile-1.0.vectors.json` | `docs/protocols/agent-profile/1.0.vectors.json` |
| `agent-discourse-1.0.vectors.json` | `docs/protocols/agent-discourse/1.0.vectors.json` |
| `agent-mail-1.0.vectors.json`, `agent-mail-1.0.schema.json` | `docs/protocols/agent-mail/1.0.{vectors,schema}.json` |
| `agent-knowledge-1.0.vectors.json`, `agent-knowledge-1.0.schema.json` | `docs/protocols/agent-knowledge/1.0.{vectors,schema}.json` |

Updating the baseline (Python 3.9+, from the repository root):

```sh
python3 scripts/sync-agent-protocols.py --update --source /path/to/agent-protocols
python3 scripts/sync-agent-protocols.py --check --source /path/to/agent-protocols
cargo test --locked -p dmsg_agent
```

The source checkout must be committed and match the declared SDK version.
Normal CI and `make test-dmsg` run `--check` offline. An SDK update must also
review the upstream test changes: the adapted Rust suites keep the same tests,
with paths redirected here and the host's clippy allowances for SDK error types.
The desktop `connector::desktop_connector_profile_and_mail` case additionally
checks authenticated inventory recovery, persistent decryption keys, grants,
profile reads and Mail delivery through the real daemon and local services.
