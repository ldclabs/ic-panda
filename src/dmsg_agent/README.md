# dmsg_agent

dMsg as an [Agent Protocols](https://github.com/ldclabs/agent-protocols)
local connector host (`agent-protocols` 0.11.4). The daemon (`dmsg_cli`)
runs one connector per Agent ID it hosts; `dmsg mcp` and `dmsg ap` reach
them through the daemon. Agent ID keys, their residency and the
capability grants live in [`dmsg_client`](../dmsg_client/README.md).

| Module | Content |
| --- | --- |
| `runtime` | Connector state per Agent ID in `<workspace>/agents.sqlite`, restored for every call and saved after it; the host allowlist comes only from `config.toml` |
| `tools` | The MCP catalog: the 23 standard `agent_protocols_*` tools and the `dmsg_*` tools |

Every tool call restores the connector from its snapshot (local connector
§4.1: hosts, rooms and records, members, timeline, read and presented
heads, join requests, agent statuses, profiles, the inbox with its leases,
held drafts and the next nonce) and saves it afterwards, even when the call
failed. Before each call every remembered host is disallowed and the
configured ones are allowed again, so neither a tool nor a stored snapshot
widens the allowlist. Calls run one at a time.

The file is not encrypted: a resident Agent ID works while the device is
locked. It holds room records the agent read, never a private key.

Tests (`cargo test -p dmsg_agent`) cover the allowlist (DT02) and a
restart that restores drafts, inbox leases, read positions and the nonce
(DT09), with a held write when the head moved (DT07); `tests/vectors.rs`
runs the protocol's normative vectors against the SDK this crate uses.
