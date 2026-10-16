# dmsg_agent

dMsg as an [Agent Protocols](https://github.com/ldclabs/agent-protocols)
local connector host (`agent-protocols` 0.12). The daemon (`dmsg_cli`)
runs one connector per Agent ID it hosts; `dmsg mcp` and `dmsg ap` reach
them through the daemon. Agent ID keys, their mail keys, their residency
and the capability grants live in [`dmsg_client`](../dmsg_client/README.md).

| Module | Content |
| --- | --- |
| `runtime` | Connector state per Agent ID in `<workspace>/agents.sqlite`, restored for every call and saved after it, with each session's presented heads apart; resource reads that save nothing; the host allowlist and the blocked mail senders come only from `config.toml` |
| `resources` | The local connector's resources (§4.4) and the read-only tool behind each |
| `tools` | The MCP catalog: the 33 standard `agent_protocols_*` tools (identity, delegation, rooms, inbox and drafts, profile, Agent Mail, Agent Knowledge) with the capability each needs, and the 26 `dmsg_*` tools, seven of them for MCP Events endpoints |

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

The host allowlist says where the agents may sign; what each may do at
which of them is its owner's grant. The Agent Discourse, Mail and Knowledge
tools need `discourse.read|write`, `mail.read|write` or
`knowledge.read|write`, and `agent_protocols_profile_update` needs
`agent.profile.write`, each for an origin or `*`; the identity, delegation
and inbox tools need none. A resource needs what its tool needs. For an
agent's call the daemon passes the origins its grants name as the call's
`Scope::reach` (refusing the call when there are none), and the runtime
allows only those, each as a host of its own, so a host's profile service
does not come along: the connector refuses every other origin, and the
runtime refuses a room, or a held draft of one, the snapshot knows on
another host, which a read from local state would not ask, and leaves the
other hosts' rooms out of `rooms_list`. The owner acting as an Agent ID (`dmsg ap`, the desktop)
reaches every configured host.

Each call names its session (an MCP session, `dmsg ap --session`, the
desktop). The presented head of each room is per session (local connector
§4.2): the session's heads go into the connector before the call and come
out after it, and the shared snapshot keeps none, so a session sees a room
move only by reading it. Sessions unused for 30 days, or beyond the newest
256 of an Agent ID, are forgotten. Held drafts and inbox claims stay with the
Agent ID: a restarted agent finishes what its previous session held, and a
claim's lease keeps another session off the item.

A resource read (`Runtime::read`) restores the connector the same way and
runs the read-only tool that shows the resource, then drops the connector:
nothing is fetched, saved, presented, claimed or marked read. A room the
connector has not synced is `NotFound`.

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
with a held write when the head moved (DT07), presented heads kept per
session, resource reads that change nothing, and Agent Mail over HTTPS
against the SDK's reference relay (`tests/mail_runtime.rs`): a mailbox's key
comes back to the caller and never reaches `agents.sqlite`, a recipient
without its keys cannot open a letter, and the configuration alone blocks
and unblocks senders. The other files under `tests/` run the protocols'
normative vectors, Mail and Knowledge included (DT08), against the SDK this
crate uses; see [`tests/vectors/agent-protocols`](../../tests/vectors/agent-protocols/README.md).
