# dmsg

The dMsg command line client. `dmsg daemon` runs the daemon, the only process
that holds keys and state; every other command asks it over loopback. The
cryptography, storage and protocol logic live in
[`dmsg_client`](../dmsg_client/README.md).

## Home

`--home <dir>`, else `$DMSG_HOME`, else `~/.dmsg`:

| Path | Content |
| --- | --- |
| `config.toml` | Deployment parameters and policies; the only place to change them |
| `owner.token` | Owner credential with `secure_storage = "file"`, readable only by its owner |
| `daemon.json` | Port, process and instance ID of the running daemon |
| `dmsg-daemon.lock` | Held by the running daemon: one daemon per home |
| `<environment>/workspace.sqlite` | The device's workspace: encrypted versions and chunks, sealed records, sync jobs |
| `<environment>/agents.sqlite` | Agent Protocols connector state of the hosted Agent IDs; no private key (see [`dmsg_agent`](../dmsg_agent/README.md)) |

```toml
environment = "local"            # local, staging or production
ic_host = "http://127.0.0.1:4943"
relay = "http://127.0.0.1:8787"  # dmsg-cloud device API
handle = "<name registry canister>"
user_homes = ["<user home>"]     # optional pins; the registry is the authority
root_key = "<hex DER>"           # local only
login_url = "http://localhost:3002/_/login"  # local only; default https://dmsg.net/_/login
idle_lock_minutes = 15           # 1 to 1440
background_online = false        # keep the device key while locked
sync_interval_minutes = 0        # scheduled sync; 0 is off
secure_storage = "system"        # "system" (default on macOS and Windows) or "file"
connect_url = "http://localhost:5174"  # local only; default https://connect.dmsg.net
agent_service = "http://127.0.0.1:8788"  # local only; default https://agents.dmsg.net
agent_hosts = [                  # Agent Protocols hosts the hosted agents may sign for
  { host = "https://api.al.ink", label = "alink" },
  { host = "https://agents.example", profile_service = "https://profiles.example" },
  { host = "https://mail.example", label = "mail" },           # an Agent Mail relay
  { host = "https://knowledge.example", label = "knowledge" }, # an Agent Knowledge service
]
agent_blocked_senders = ["did:agent:…"]  # mail from these is deleted unopened
agent_ca = "/path/to/ca.pem"     # local only: replaces the system roots for agent_hosts
```

Outside `local`, every URL must be HTTPS and `root_key`/`login_url`/`connect_url`/`agent_service`/`agent_ca` are refused.
`agent_hosts` is the only source of the host allowlist: no command, method or
tool changes it, and a host must be an origin (loopback HTTP only in `local`).
Mail relays and Knowledge services are hosts like any other, and HTTPS only:
an agent fetches cards, delivers letters and submits or reads Knowledge only
at them. `agent_blocked_senders` is likewise the only source of the mail
sender policy. The connectors' requests follow no redirect.

`secure_storage = "system"` keeps the owner credential and the fast-unlock key
in the system credential store (Keychain, Credential Manager, Secret Service)
under the service `dmsg`. `"file"`, the default elsewhere, keeps the owner
credential in `owner.token`; fast unlock is then unavailable.

## Commands

```sh
dmsg daemon [--port N]           # foreground; port 0 (default) picks a free one
dmsg status [--json]
dmsg login [--origin dmsg.net|panda.fans] [--no-browser]
dmsg unlock [--fast] | lock | stop
dmsg fast-unlock enable | disable
dmsg account create | show
dmsg device list | pair-request [--member] | approve <request> --device <id>
dmsg vault add --title T [--type note|login|api|key] [--body B] [--username U] [--url L] [--tag T]... [--secret-stdin]
dmsg vault edit <id> [--title T] [--body B] [--username U] [--url L] [--tag T]... [--secret-stdin]
dmsg vault rm <id> | restore <id> | list [--trash] | search <text> | show <id> [--reveal]
dmsg vault conflicts | resolve <id:revision> [--keep-current]
dmsg file import <path> | list | get <id> -o <path> | verify <id>
dmsg sync | sync status
dmsg profile show | publish [--name N] [--bio B] [--link URL]... | resolve <account>
dmsg reset --yes
dmsg approvals list [--all] | show <id> | approve <id> | reject <id> | cancel <id>
dmsg attest text <text> | digest (--file F | --sha256 H) | file <path> --text T
dmsg attest history | export <execution> -o <path> | verify <bundle> [--file F]
dmsg device capabilities <id> --set a,b,c | formal-approve on|off | revoke <id>
dmsg connect browsers add [--copy-link] | list | revoke <pairing>
dmsg connect requests | sync
dmsg channel list | show <id> | create <name> [--type direct|collaboration|distribution]
dmsg channel invite <id> <account> [--role member|publisher] | accept <invitation>
dmsg channel members <id> | role <id> <account> <role> | remove <id> <account> | leave <id> | archive <id>
dmsg channel rotate <id> | reactivate <id> | history <id> <account> <from> <to>
dmsg channel send <id> [text] [--file <item>] [--message-id M] | read <id> [--after N] [--backfill]
dmsg channel download <id> <seq> --out <path> | pending <id> [--key K]
dmsg channel transfer|sponsor propose <id> <account> | accept <packet> | commit <packet>
dmsg inbox policy | configure closed|contacts|free|paid [--price P] [--payee P] [--allow A]...
dmsg inbox send <recipient> <text> [--pay] [--invitation I] [--order O] | pay <recipient> <order>
dmsg inbox order <recipient> <order> | outgoing | escrow <order> | refund <order>
dmsg inbox list | mark <order> [--archive]
dmsg wallet use [pem] | balance <ledger>
dmsg plan catalog | status | quote <sku> <ledger|neuron> [--method cash|panda]
dmsg plan checkout <id> | advance <id> | order <id> | list | cancel <id> | reconcile <id> | refund <id> [block]...
dmsg handle resolve <name> | price <name> | claim <name> | register <name>
dmsg agent principal show | enable [--type person|organization|team|project|other] | publish | refresh
dmsg agent controller register [--name N] [--scope S]... [--audience A]... | retire <gen> | compromise <gen> --from <ms> | rename <gen> [--name N]
dmsg agent grant <subject> --scope S... --audience A... [--days 30] [--generation G] [--relationship R]
dmsg agent revoke <id> [--generation G] [--reason R] | credentials | delegations
dmsg agent id create <label> | list | use <agent> | resident <agent> on|off | credential <agent>
dmsg grants list | add <agent> <capability>... [--days D] | revoke <id>
dmsg ap [--agent A] tools | <tool> [json-input]
dmsg mcp                         # stdio MCP server; needs DMSG_AGENT_TOKEN
```

`--json` prints one JSON value per line; errors are `{"error": {"code", "message"}}`.
`dmsg status --json` prints `{"running": false}` and exits with 3 when no
daemon runs.

A new device: `login`, then `account create` for a new account, or
`device pair-request` and, on an administrator device,
`device approve <request> --device <id>`, which also rotates the content root
to the new device; `unlock` on the new device then binds it. Secrets leave the
daemon only with `vault show --reveal`; file keys never do.

Unlocking through a login opens the root and syncs once. Every write waits
for `dmsg sync` or the scheduled sync; `status` shows pending versions and
conflict copies. An edit that lost a race is kept as a conflict copy on every
device; `vault resolve` adopts it, or keeps the current version with
`--keep-current`. An item in the trash takes only `vault restore` (or
adopting a copy) back. `file import` resumes an interrupted import of the same
file; `file get` writes only a fully verified file to a path that does not
exist yet. `profile publish` publishes exactly the fields given. `reset`
erases the workspace, unsent versions included, and starts over as a new
device; a device its account revoked does this on its own.

Approvals are decided by a person: `approvals approve|reject`, `connect
browsers add` and the prompt after `attest …` or `device …` need an interactive
terminal without `--json`; otherwise the request is only created and waits in
the approval center (and the desktop's approval window). A web request shows
its origin as relayed by the browser through connect.dmsg.net, the browser's
label, the account and the payload. `attest verify` needs no daemon: it checks
the signature, the receipt certificate under the configured root, their
binding, the file when given, and the device's present status.

Channels: `create` makes the first epoch at once; a membership change leaves
the channel at `rotation_required` until a member with `can_rotate` runs
`rotate` (the owner's `remove` does it itself). An invited account runs
`accept` with the JSON `invite` printed, then reads from the next epoch;
`history` gives it older ones. `read` prints what this device read; while
unlocked the daemon also reads on the relay's hints. A command whose reply was
lost is kept in `pending`, and running it again, or `pending --key`, settles it
under the same request ID.

Payments go through the approval center like signatures: `inbox pay`,
`plan checkout|advance` and `handle claim|register` only create the request,
which a person approves at the terminal or in the desktop's approval window,
seeing the exact amount, ledger and payer. The payer is the login's Principal
unless `wallet use <pem>` names an identity file; the dMsg account itself
never holds funds. A paid letter is `inbox send --pay` (a quote), then
`inbox pay`; a subscription is `plan quote`, then `plan checkout`.

Agents. `agent principal enable` and every `agent controller` change are
account changes an administrator device approves; the user home publishes
the principal document to the directory (`published: false` in `principal
show` means it has not yet, and `principal publish` retries). `controller
register` creates the key in the vault; without `--scope`/`--audience` its
ceiling is `*`. `agent grant` and `revoke` sign with the newest current
controller (or `--generation`) on a device with `FormalApprove`, after
approval; `delegations` shows where each signed event stands and
`credentials` what the delegation service holds.

`agent id create <label>` hosts an Agent ID; the same label on another device
of the account brings the same Agent ID. `agent id use` picks the one `dmsg ap`
runs as; `resident on` keeps its key in the system credential store so it
signs while the device is locked (unavailable with `secure_storage = "file"`).
The Agent Mail keys its mailboxes need are sealed on this device, never synced
and never in the connector state; a resident Agent ID keeps a copy beside its
key, and `resident off` folds the keys it made while locked back in, so it
needs the device unlocked then.
`agent id credential` prints, once, the credential an MCP client gives
`dmsg mcp`; issuing a new one disables the old. `grants add` gives an agent
capabilities over this account: `account.read`, `vault.list`,
`vault.read:<tag>`, `channel.read:<channel>`, `channel.publish:<channel>`,
`inbox.read`, `attest.request` (`<name>:*` covers every target); without a
grant an agent reads only public profiles (`profile.read`).

`dmsg ap <tool> [input]` runs one MCP tool as an Agent ID with the same input
and result; the tool name may leave out `agent_protocols_` or `dmsg_`, and
`dmsg ap tools` lists them. For example:

```sh
dmsg ap --agent bob mail_mailbox_publish '{"routes":["https://mail.example"]}'
dmsg ap --agent alice mail_send '{"to":"did:agent:…/mail/…?route=https://mail.example","text":"hello"}'
dmsg ap --agent bob mail_sync
dmsg ap --agent alice knowledge_query '{"service":"https://knowledge.example","tag":"caching"}'
```

## MCP

`dmsg mcp` serves MCP on stdio (line-delimited JSON-RPC; `initialize`,
`tools/list`, `tools/call`, `ping`) for the Agent ID whose credential is in
`DMSG_AGENT_TOKEN`. It reads neither the owner credential nor any key: the
daemon runs the Agent ID's connector and checks its capabilities. For Claude
Code, after `dmsg agent id create claude` and `dmsg agent id credential claude`:

```sh
claude mcp add dmsg --env DMSG_AGENT_TOKEN=dmsg-agent-… -- dmsg mcp
```

Tools: the 33 standard `agent_protocols_*` tools of the Agent Protocols
local connector (`agent-protocols` 0.12: identity, delegation, rooms, inbox
and drafts, profile, Agent Mail mailboxes and letters, Agent Knowledge
submissions and reads), as this Agent ID, against the configured hosts; and
`dmsg_account_status`, `dmsg_vault_list`, `dmsg_vault_read` (never the secret
field), `dmsg_channel_list|read|publish`, `dmsg_inbox_list`,
`dmsg_profile_read`, `dmsg_attest_request` (an approval request with source
`agent:<did>`, which only a person decides) and `dmsg_approval_status` (only
the agent's own requests). A refused call returns `isError` with the
protocol's `code`, such as `permission_denied`, `room_head_mismatch` or
`FORBIDDEN`.

## Daemon protocol

The daemon listens on `127.0.0.1` only.

- `GET /daemon/status`, `POST /app/v1` (JSON-RPC 2.0) and `GET /ws/app/v1`
  (the same methods over a WebSocket, plus `event` notifications
  `{seq, type, data}`: `session/changed`, `vault/changed`, `sync/changed`,
  `approval/changed`, `connect/changed`, `connect/hint`, `channel/changed`,
  `inbox/changed`, `resync`) need
  `Authorization: Bearer <owner token>`; any request with an `Origin` header
  is refused with 401. Errors carry `error.data.code` such as `AUTH_REQUIRED`,
  `LOCKED`, `VERSION_CONFLICT` or `EXECUTION_UNKNOWN`.
- Methods: `app/initialize`, `daemon/status`, `events/poll`,
  `session/status|login|login_result|unlock|lock|fast_unlock`,
  `account/create|show`,
  `device/pair_request|approve|capabilities|formal_approve|revoke`,
  `vault/put|edit|delete|restore|list|show|conflicts|resolve`,
  `file/import|export|verify` (absolute paths), `sync/run|status`,
  `profile/show|publish|resolve`, `approval/list|show|respond|cancel`,
  `attest/request|history|export|verify`,
  `connect/invite|candidates|grant|pairings|revoke|sync|status`,
  `channel/list|show|create|invite|join|control|rotate|send|send_file|sync|messages|download|history|reactivate|pending|resume`,
  `channel/owner_propose|owner_accept|owner_commit|sponsor_propose|sponsor_accept|sponsor_commit`,
  `inbox/policy|configure|send|order|outgoing|pay|escrow|refund|list|mark`,
  `wallet/use|balance`,
  `plan/catalog|status|assets|quote|checkout|advance|order|checkouts|cancel|reconcile|refund`,
  `handle/resolve|price|claim|register`, `approval/reconcile`,
  `agent/principal|enable|register|retire|compromise|rename|publish|refresh`,
  `agent/grant|revoke|credentials|delegations`,
  `agent/ids|create|use|resident|credential`, `grants/list|add|revoke`,
  `ap/call`, `hint/status`, `workspace/reset`, `daemon/stop`.
- An Agent ID's credential (`Bearer dmsg-agent-…`) reaches only `POST /app/v1`
  and only `mcp/tools` and `mcp/call {tool, input}`, as that Agent ID; the
  owner credential never reaches those two. A tool answers
  `{"result": …}` or `{"error": {code, message, data?}}`. Events of agent
  changes: `agent/changed`.
- `approval/respond` and `connect/grant` require `human: true`, which only
  the desktop approval window and an interactive terminal send; the daemon
  cannot tell them from another holder of the owner credential, so the owner
  credential is the boundary. Agents present their own credential, which
  never reaches them.
- `GET /auth/callback` and `POST /auth/callback` exist only while a login is
  pending (ten minutes) and accept one POST from the daemon's own loopback
  origin ([dmsg-login/1](../../docs/protocol/login-v1_zh.md) §4).
- `idle_lock_minutes` without a request locks the daemon: the login and every
  key are dropped. Revisions already signed for submission are still sent,
  without any key, until their 45-second deadline. With `background_online`
  the device signing key stays: `dmsg sync` and the scheduled sync still send
  sealed versions and fetch ciphertext, while reading, editing and every
  account change answer `LOCKED`.
- The daemon keeps the relay's hint channel open, locked or not, with a token
  it registers while the device key is available. A `connect` hint fetches the
  mailbox (stored sealed while locked); without the device key the daemon only
  emits `connect/hint` for the desktop to notify. The mailbox is also polled
  every 30 seconds.
- While unlocked, the daemon keeps one WebSocket per active channel; an
  `available` hint, a new socket and a closed one each trigger a signed read.
  A socket's ticket lasts about a minute, so it is renewed every 40 seconds.
