# dMsg Mail push and local events

English | [简体中文](mail_events_zh.md)

2026-10-23. Implementation contract, not evidence of production deployment or host wakeup.

## 1. Two events

`dmsg.mail.available` tells a recipient that its relay accepted ciphertext. It coalesces mailbox changes and contains only `{mailbox_id, through_seq}`, never a subject, body, attachment or sender. It is not a read receipt.

`dmsg.mail.received` is emitted after the local device verifies, decrypts, deduplicates and saves a message. `dmsg mcp` serves it by `poll` and `push`, with `mailbox_id`, `from`, `message_id` and `thread_id`. `dmsg.relay.delivery` remains the webhook transport event. Message, Packet and Mailbox Card signed formats are unchanged. The connector's `sync_mode: explicit` permits a host to drive sync from notifications.

## 2. Optional relay extension

`GET /.well-known/dmsg-mail-events` advertises `protocol: dmsg-mail-events/1`, the canonical `service`, its same-origin `/mcp` endpoint and event name, independently of the `agent-mail/1.0` discovery document.

`POST /mcp` implements `server/discover`, `initialize`, `events/list`, `events/subscribe` and `events/unsubscribe`. Subscriptions require a short-lived mailbox-owner Agent Identity JWT whose audience is the relay origin. Subscribe with `name: dmsg.mail.available`, `arguments: {mailbox_id}`, `delivery: {mode: webhook, url, secret}`, optional `ttlMs` and `cursor`:

```json
{
  "name": "dmsg.mail.available",
  "arguments": {"mailbox_id": "<owned-mailbox>"},
  "delivery": {"mode": "webhook", "url": "<callback-url>", "secret": "<whsec-secret>"},
  "ttlMs": 604800000,
  "cursor": null
}
```

Callback origins are explicitly approved by the operator. A signed Standard Webhooks challenge must succeed before activation; verification and delivery never follow redirects. IDs are stable for an owner, mailbox and URL. Leases are capped at seven days and the current mailbox card expiry; the service may shorten them, so clients renew against the returned deadline. Renew from the returned `id`, `refreshBefore`, safe `cursor` and `truncated` flag. Notifications are coalescible hints and may repeat or arrive out of order. Sync authoritative mail on first subscription, reconnection or a gap. An identical Packet retry does not create a new change; unsubscribe does not delete accepted mail.

Coalesced relay hints do not support `maxAgeMs`; the local received-event log below does.

## 3. Device lifecycle

The owner uses `dmsg agent mail-watch <agent> on|off`; omit the state to inspect status, or use the desktop Mail switch. Existing `mail.read:<relay>` and `events.endpoint` grants are required; enabling push never grants new authority.

A dedicated endpoint is reused per Agent, with subscriptions renewed per mailbox. State and callback credentials are encrypted separately per Agent in the selected account's local workspace and are not synchronized between devices. Resident Agents retain only the subscription's scoped local key in the system credential store; disabling residency or removing the Agent deletes that key. Account content root keys are not retained. Existing watches convert on the first unlock after upgrading, preserving subscriptions and cursors. Disabling, pausing or revoking access stops local processing and schedules cleanup; unreachable subscriptions eventually expire. Network waits release the shared Client, except for the existing journaled endpoint creation/deletion lifecycle.

Login and session changes wake the hint connection immediately; reconnect catches up Mail and MCP Events. The account hint channel's `mcp-event` wakes Mail sync; a hint cannot choose a new destination or authorize an action. The daemon checks every ten minutes and requests a seven-day renewal when `refresh_before` is within sixty minutes, bounded by the remaining lifetime of both required grants. Watch, Agent, grant and session changes also wake maintenance; failed renewals retry on later checks. Locked resident Agents may receive and renew using an existing callback while unpaused and authorized. Creating or deleting account-owned callback endpoints waits for unlock; locked cleanup unsubscribes upstream and retains the endpoint cleanup target. Nonresident Agents resume maintenance and catch up after unlocking. Periodic Mail sync remains the fallback.

## 4. Local MCP events and read state

`events/list` advertises `dmsg.mail.received` with an optional `mailbox_id` filter. `events/poll` and `events/stream` queries enforce the current Agent's Mail grants. Cursors belong to the Agent and mailbox filter, independently of relay cursors; null starts at the current head. `maxAgeMs` is supported. Replay outside retention reports `truncated: true`.

Events and connector snapshots commit atomically. IDs are stable for `(recipient, from, message_id)`. Each Agent retains up to 10,000 events for seven days. Persist cursors and deduplicate event IDs; inspect unread mail and the attention queue after a gap. Event delivery itself neither marks messages read nor completes work.

Claim the `mail.received` attention item, read its `from + message_id` with `mail_list(mark_read=true)`, then call `inbox_ack(handled)` after owner-authorized handling. Retried work may already be read and should still be loaded by ID; defer unfinished work. Desktop previews and historical inspection leave `mark_read=false`. Read state, completed inbox work and permission to execute a letter's request are distinct.

## 5. Host boundary

The MCP host must subscribe and schedule an Agent run. Anda's native event triggers consume push directly. Hosts without event wakeup still need explicit checks. `notifications/resources/updated` resource notifications remain useful but do not prove an LLM turn started. Repository tests cover the protocol extension, reconnection, revocation and complete local path. Production latency and each host's actual wakeup need separate acceptance tests.
