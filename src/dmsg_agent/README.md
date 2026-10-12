# dmsg_agent

dMsg as an [Agent Protocols](https://github.com/ldclabs/agent-protocols)
local connector host (`agent-protocols` 0.12). The daemon (`dmsg_cli`)
runs one connector per Agent ID it hosts; `dmsg mcp` and `dmsg ap` reach
them through the daemon. Agent ID keys, their mail keys, their residency
and the capability grants live in [`dmsg_client`](../dmsg_client/README.md).

| Module | Content |
| --- | --- |
| `runtime` | Connector state per Agent ID in `<workspace>/agents.sqlite`, restored for every call and saved after it; the host allowlist and the blocked mail senders come only from `config.toml` |
| `tools` | The MCP catalog: the 33 standard `agent_protocols_*` tools (identity, delegation, rooms, inbox and drafts, profile, Agent Mail, Agent Knowledge) and the `dmsg_*` tools |

Every tool call restores the connector from its snapshot (local connector
§4.1: hosts, rooms and records, members, timeline, read and presented
heads, join requests, agent statuses, profiles, the inbox with its leases,
held drafts, mailbox cards, card pins, stored letters and the next nonce)
and saves it afterwards, even when the call failed. Before each call every
remembered host is disallowed and every remembered blocked sender unblocked,
then the configured ones are applied again, so neither a tool nor a stored
snapshot changes them. Calls run one at a time, each inside a turn the
daemon holds from reading the Agent ID's mail keys to saving those the call
left.

The Agent Mail decryption keys are not in the snapshot: the daemon brings
them from `dmsg_client`, where they are sealed under the LDK (and copied
beside a resident Agent ID's key), and keeps what the call returns, which
drops the keys of cards past `receive_until`. Mail relays and Knowledge
services are ordinary hosts and HTTPS only; requests follow no redirect.
A local deployment may name its own CA (`agent_ca`), which then replaces the
system roots: the platform verifier refuses long-lived test certificates.

The file is not encrypted: a resident Agent ID works while the device is
locked. It holds the room records and the letters the agent read, never a
private key.

Tests (`cargo test -p dmsg_agent`) cover the allowlist (DT02), a restart
that restores drafts, inbox leases, read positions and the nonce (DT09),
with a held write when the head moved (DT07), and Agent Mail over HTTPS
against the SDK's reference relay (`tests/mail_runtime.rs`): a mailbox's key
comes back to the caller and never reaches `agents.sqlite`, a recipient
without its keys cannot open a letter, and the configuration alone blocks
and unblocks senders. The other files under `tests/` run the protocols'
normative vectors, Mail and Knowledge included (DT08), against the SDK this
crate uses; see [`tests/vectors/agent-protocols`](../../tests/vectors/agent-protocols/README.md).
