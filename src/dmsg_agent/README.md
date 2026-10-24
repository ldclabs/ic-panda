# dmsg_agent

dMsg as an [Agent Protocols](https://github.com/ldclabs/agent-protocols)
local connector host (`agent-protocols` 0.12.6). The daemon (`dmsg_cli`)
runs one connector per Agent ID it hosts; `dmsg mcp` and `dmsg ap` reach
them through the daemon. Agent ID keys, their mail keys, their residency
and the capability grants live in [`dmsg_client`](../dmsg_client/README.md).

| Module | Content |
| --- | --- |
| `runtime` | Connector state per Agent ID in `<workspace>/agents.sqlite`, restored for every call and saved after it when it changed, with each session's presented heads apart; resource reads that save nothing; the sessions an Agent ID was used in, and forgetting everything of one that is no longer hosted; its published Agent Profile, read from the configured profile services and checked against its signature; the host allowlist and the blocked mail senders come only from `config.toml` |
| `resources` | The local connector's resources (§4.4) and the read-only tool behind each |
| `tools` | The MCP catalog: the 41 standard `agent_protocols_*` tools (identity, delegation, rooms, inbox and drafts, profile, Agent Mail, Agent Knowledge) with the capability each needs, and the 34 `dmsg_*` tools, six of them for team wallets, seven for MCP Events endpoints and two for the agent's own capabilities |

Every tool call restores the connector from its snapshot (local connector
§4.1: hosts, rooms and records, members, timeline, read and presented
heads, join requests, agent statuses, profiles, the inbox with its leases,
held drafts, mailbox cards, card pins, stored letters and the next nonce)
and saves it afterwards, even when the call failed; a call that leaves it as
it was does not rewrite it and says so, and only a change wakes the MCP
sessions that watch resources. The snapshot keeps no host, and before each
call the configured hosts and blocked senders replace what it remembers, so
neither a tool nor a stored snapshot changes them. The SDK keeps each room's
latest 1000 records and timeline items and 1000 inbox entries, so a call's
cost does not grow with a room's age, and the connectors share one discovery
cache: each host's discovery document is read once, and again only after a
network error, 429 or 5xx.
Calls of one Agent ID run one at a time, each inside a turn the daemon holds
from reading the Agent ID's mail keys to saving those the call left; calls
of different Agent IDs do not wait for each other.

The host allowlist says where the agents may sign; what each may do at
which of them is its owner's grant. The Agent Discourse, Mail and Knowledge
tools need `discourse.read|write`, `mail.read|write` or
`knowledge.read|write`; Profile reads need `agent.profile.read`, and updates
need `agent.profile.write`, each for an origin or `*`. Identity, configured
service discovery, delegation and inbox tools need no grant. Bundled schema
lookup needs none either; a schema lookup with `room_id` needs `discourse.read`.
A resource needs what its tool needs. For an agent's call, the daemon passes
the origins its grants name as the call's `Scope::reach` (refusing the call when there are none), and the runtime
allows only those, each as a host of its own, so a host's profile service
does not come along: the connector refuses every other origin, and the
runtime refuses a room, or a held draft of one, the snapshot knows on
another host, which a read from local state would not ask, and leaves the
other hosts' rooms and held drafts out of `rooms_list` and `drafts_list`; a
resource of such a room is `Forbidden`. The owner acting as an Agent ID
(`dmsg ap`, the desktop) reaches every configured host.

Each call names its session (an MCP session, `dmsg ap --session`, the
desktop). The presented head of each room is per session (local connector
§4.2): the session's heads go into the connector before the call and come
out after it, and the shared snapshot keeps none, so a session sees a room
move only by reading it. Sessions unused for 30 days, or beyond the newest
256 of an Agent ID, are forgotten. Held drafts and inbox claims stay with the
Agent ID: a restarted agent finishes what its previous session held, and a
claim's lease keeps another session off the item.

Every standard tool declares the SDK catalog's full `inputSchema` and
`outputSchema`, as local connector Section 4.3 defines them. Input schemas do
not use `oneOf`, `anyOf` or `allOf`,
which Anthropic's API refuses in a tool's input schema, failing the whole
request (the SDK's catalog; `tools::tool_list` adds each tool's
capability). A test calls each tool with only its required inputs, and with
none, so an SDK upgrade that changes what the connector reads shows. The
connector refuses a Knowledge payload that does not match its type before
signing, with every failing member at its path (`invalid_event`,
`data.errors`).

A resource read (`Runtime::read`) restores the connector the same way and
runs the read-only tool that shows the resource, then drops the connector:
nothing is fetched, saved, presented, claimed or marked read. A room the
connector has not synced is `NotFound`.

Mail inspection (`mail_list` without `mark_read`, and `mail_mailboxes_list`
without a remote `host`) reads the last committed snapshot without taking the
network turn or saving state. Mailbox views receive the currently available
keys so `key_available` is accurate; no key is returned in the view. Reads that mark messages continue through
the normal serialized call, as do mailbox changes and receiving. Capability,
pause and residency checks still apply in the daemon.

The Agent Mail decryption keys are not in the snapshot: the daemon brings
them from `dmsg_client`, where they are sealed under the LDK (and copied
beside a resident Agent ID's key), and keeps what the call returns, which
drops the keys of cards past `receive_until`. Mail relays and Knowledge
services are ordinary hosts and HTTPS only; requests follow no redirect.
A local deployment may name its own CA (`agent_ca`), which then replaces the
system roots: the platform verifier refuses long-lived test certificates.

The file is not encrypted: a resident Agent ID works while the device is
locked. It holds room records and letters the agent read, plus encrypted Mail
key checkpoints; it never stores an unencrypted private key.

Tests (`cargo test -p dmsg_agent`) cover the allowlist (DT02), a restart
that restores drafts, inbox leases, read positions and the nonce (DT09),
with a held write when the head moved (DT07), presented heads kept per
session, resource reads that change nothing, the reach of an agent's grant,
unchanged state left unwritten, turns per Agent ID, and Agent Mail over HTTPS
against the SDK's reference relay (`tests/mail_runtime.rs`): a mailbox's key
comes back to the caller and never reaches `agents.sqlite`, a recipient
without its keys cannot open a letter, and the configuration alone blocks
and unblocks senders. The other files under `tests/` run the protocols'
normative vectors, Mail and Knowledge included (DT08), against the SDK this
crate uses; see [`tests/vectors/agent-protocols`](../../tests/vectors/agent-protocols/README.md).

## Connector 0.12.6 workflows

All 41 standard tools are classified explicitly; together with 34 dMsg tools,
`tools/list` exposes 75 tools. New tool permissions are:

| Tool (with `agent_protocols_` prefix) | Capability |
| --- | --- |
| `profiles_get`, `profile_history` | `agent.profile.read:<origin>` |
| `mail_resolve` | `mail.read:<origin>` for every relay and, when used, the Profile service |
| `room_archive`, `room_watch` | `discourse.read:<origin>` |
| `schema_get` | None for bundled schemas; `discourse.read:<origin>` with `room_id` |
| `services_list`, `delegation_history` | No additional grant; operator host policy and protocol checks still apply |

Configured `profile_service` origins are explicit grant targets for Profile
reads/writes and Mail resolution. Granting a room or relay origin does not
implicitly grant its Profile service. The desktop capability controls offer
Profile read and write separately and list these additional origins.

Read `profiles_get` before `profile_update`: updates replace the entire profile.
Preserve desired fields, especially service endpoints and delegation hints, and
remove service-derived `updated_at` and `event_id`. The service may enforce its
own admission policy in addition to the local capability grant. Omit
`mailbox_id` and pass nonempty `routes` to `mail_mailbox_publish` to create an
owned mailbox; keys stay in the daemon's sealed store. `mail_resolve` obtains
verified addresses, `mail_send` optionally includes `reply_mailbox_id`, and an
explicit `mail_sync` precedes local inbox/message reads. `identity_current`
reports the configured Profile service URLs and the core's explicit sync mode.

The desktop acceptance case `connector::desktop_connector_profile_and_mail`
runs these paths through the real stdio MCP client, daemon and local services,
including missing grants, wrong origins, grant revocation, profile read/edit/
read-back, mailbox creation, reply threading, read state and restart persistence.
It uses temporary local accounts and test identities, never production data.

## Durable Mail events

`mail_events` records newly received messages in the same SQLite transaction as
the connector snapshot. It stores IDs and relay scope only, with stable event
IDs, per-Agent retention and independent cursors bound to the mailbox filter.
Queries check live relay grants, report retention gaps and do not alter Mail
read state or inbox work. The daemon exposes this log as `dmsg.mail.received`.

## Publication checkpoints and verified profiles

Every `Runtime::call` takes the host-only key sealer of `DurableMail`.
Before each Mail publication attempt, the SDK callback encrypts the key snapshot
and commits it with the connector snapshot in one SQLite transaction. It keeps
the original database handle, so a concurrent workspace reset cannot recreate
the removed state file. A failed checkpoint stops publication and leaves the
last successfully committed checkpoint intact.

On restart, the daemon restores the encrypted checkpoint before using the
connector. It clears the checkpoint only after the returned keys have reached
the normal sealed/resident key stores. The checkpoint uses a workspace- and
Agent-specific key derived under `dmsg/agent-mail-checkpoint/1`; it does not retain
the account's content root during network calls. The key codec and the encrypted
blob are host-only, never MCP results. Existing connector snapshots remain version 2.

`mail_mailboxes_list` with `host` is authenticated network recovery and runs under
the normal per-Agent turn, saves verified cards and honors relay grants. It may
recover records without their decryption keys. Local inspection and resources
report `key_available` and pending routes; inventory is not key or plaintext
recovery. The TLS regression tests cover pre-publication failures, acceptance
followed by persistence failure, restart recovery and a concurrent reset.

The owner's public Profile display also keeps signed greatest-nonce pins in
SQLite, independently of its in-memory clients. It checks the latest envelope,
rejects rollback and same-nonce conflicts across configured services and process
restarts, and refuses discovery endpoints on another origin. A lower candidate
read earlier in a multi-service round is reported stale if a newer one is found.
The SDK's per-Agent connector Profile cache is retained in its normal snapshot.
