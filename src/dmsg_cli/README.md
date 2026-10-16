# dmsg

The dMsg command line client. `dmsg daemon` runs the daemon, the only process
that holds keys and state; every other command asks it over loopback. The
cryptography, storage and protocol logic live in
[`dmsg_client`](../dmsg_client/README.md).

## Home

`--home <dir>`, else `$DMSG_HOME`, else `~/.dmsg`:

| Path | Content |
| --- | --- |
| `config.toml` | Deployment parameters and policies; the only place to change them. Without it the production deployment built into the binary applies ([`production.toml`](production.toml)) |
| `owner.token` | Owner credential with `secure_storage = "file"`, readable only by its owner |
| `daemon.json` | Port, process and instance ID of the running daemon |
| `dmsg-daemon.lock` | Held by the running daemon: one daemon per home |
| `logs/dmsg-YYYY-MM-DD.log` | The daemon's run log, one file per UTC day, kept 30 days (`dmsg logs`) |
| `logs/stderr.log` | The stderr of a daemon started by `dmsg start` (rewritten each time) or at login: what fails before the run log opens |
| `<environment>/accounts/<account>/workspace.sqlite` | An account's workspace on this device: its own device keys, encrypted versions and chunks, sealed records, sync jobs |
| `<environment>/accounts/<account>/agents.sqlite` | Agent Protocols connector state of the account's hosted Agent IDs; no private key (see [`dmsg_agent`](../dmsg_agent/README.md)) |
| `<environment>/logins/<principal>/` | The workspace of a login without an account; it becomes `accounts/<account>/` once the login has one |
| `<environment>/current` | The account selected last, which a restart and fast unlock use |

A home without `config.toml` uses [`production.toml`](production.toml),
compiled into the binary: the production relay, IC host, name registry and
Agent Protocols hosts, so the desktop app and a fresh install work without
setup. A `config.toml` replaces it whole (nothing is merged), and `dmsg
validate-config` and `dmsg doctor` say which one applies. A release's built-in
settings change with the release; a `config.toml` stays as written.

```toml
environment = "local"            # local, staging or production
ic_host = "http://127.0.0.1:4943"
relay = "http://127.0.0.1:8787"  # dmsg-cloud relay: device API and Agent Delegation
handle = "<name registry canister>"
user_homes = ["<user home>"]     # optional pins; the registry is the authority
root_key = "<hex DER>"           # local only
login_url = "http://localhost:3002/_/login"  # local only; default https://dmsg.net/_/login
idle_lock_minutes = 15           # 1 to 1440
background_online = false        # keep the device key while locked
sync_interval_minutes = 0        # scheduled sync; 0 is off
secure_storage = "system"        # "system" (default on macOS and Windows) or "file"
connect_url = "http://localhost:5174"  # local only; default https://connect.dmsg.net
agent_hosts = [                  # Agent Protocols hosts the hosted agents may sign for
  { host = "https://api.al.ink", label = "alink" },
  { host = "https://agents.example", profile_service = "https://profiles.example" },
  { host = "https://mail.example", label = "mail" },           # an Agent Mail relay
  { host = "https://knowledge.example", label = "knowledge" }, # an Agent Knowledge service
]
agent_blocked_senders = ["did:agent:…"]  # mail from these is deleted unopened
agent_ca = "/path/to/ca.pem"     # local only: replaces the system roots for agent_hosts

[legacy]                         # the old service's freeze the deployment reviewed
channels = ["<old channel canister>"]
identity = "<old name-identity canister>"
buckets = ["<old OSS bucket>"]
cutover = "<32-byte hex cutover ID>"
```

Outside `local`, every URL must be HTTPS and `root_key`/`login_url`/`connect_url`/`agent_ca` are refused.
The relay is also the Agent Delegation service of the account's principal.
`[legacy]` is needed only for frozen comparisons and shared channel
inheritance; frozen evidence of any other canister or cutover is refused. The
built-in settings have no `[legacy]` until the old service is frozen; the
release after the freeze adds its canisters and cutover ID.
`agent_hosts` is the only source of the host allowlist: no command, method or
tool changes it, and a host must be an origin (loopback HTTP only in `local`).
Mail relays and Knowledge services are hosts like any other, and HTTPS only:
an agent fetches cards, delivers letters and submits or reads Knowledge only
at them. An entry names an origin the agents may sign for, not what it
serves: each tool reads its protocol's discovery document at the origin it
is given (`/.well-known/agent-mail`, `agent-knowledge`, `agent-discourse`,
`agent-delegation`, `agent-profile`) and fails where the origin serves none.
The relay is one of them: it is the account principal's Agent Delegation
and Agent Profile service. What each agent may do at which of the hosts,
such as reading Agent Discourse everywhere without speaking, is the owner's
grant (`grants add` below). `agent_blocked_senders` is likewise the only source of the mail
sender policy. The connectors' requests follow no redirect.

The daemon makes the home its owner's only (mode 700) when it starts: the
workspaces' metadata and the hosted agents' room records and letters are not
encrypted.

`secure_storage = "system"` keeps the owner credential and the fast-unlock key
in the system credential store (Keychain, Credential Manager, Secret Service)
under the service `dmsg`. `"file"`, the default elsewhere, keeps the owner
credential in `owner.token`; fast unlock is then unavailable.

## Install, start and update

```sh
dmsg start                       # the daemon in the background, unless one runs
dmsg stop                        # after the running operation; returns once it exited
dmsg restart [--interrupt] [--wait 60]
dmsg install [--dir D]           # this binary to the shared location
dmsg update [--check] [--force] [--interrupt] [--wait 60]
dmsg autostart install | uninstall | status
dmsg validate-config
dmsg doctor                      # checks; exits 1 when one fails
dmsg logs [-n 100]               # the newest lines of the run log
```

[`scripts/dmsg/install.sh`](../../scripts/dmsg/install.sh) (macOS Apple
Silicon, Linux) and [`install.ps1`](../../scripts/dmsg/install.ps1) (Windows
x64) download the latest release, check its SHA-256 and run `dmsg install`;
`--uninstall`/`-Uninstall` stops the daemon and removes the login registration
and the binary, never the home. Homebrew installs `ldclabs/tap/dmsg`.

- **One binary.** `dmsg install` puts the running binary at the shared
  location (`DMSG_INSTALL_DIR`, else `~/.local/bin`, or
  `%LOCALAPPDATA%\dMsg\bin` on Windows, outside the desktop app's install
  directory) unless the one there is the same release or newer, or Homebrew's
  (left to `brew upgrade dmsg`). The desktop app runs its bundled copy only
  this way and then runs the installed one, so the CLI, agents and the app
  share one daemon. `--json` reports `{action, path, version, on_path}` with
  `action` one of `installed`, `upgraded`, `current`, `kept_newer`,
  `homebrew`.
- **Releases.** dMsg client releases are tagged `dmsg-v<version>` and are the
  repository's latest release (canister releases are not).
  `dmsg update --check --json` reports `{current, latest, available,
  homebrew}`. `dmsg update` downloads this platform's binary
  (`dmsg-macos-arm64`, `dmsg-linux-x86_64`, `dmsg-linux-arm64`,
  `dmsg-windows-x64.exe`), checks it against the release's `.sha256` and, when
  this binary is signed by a Developer ID team, that the new one verifies and
  is signed by the same team; then it replaces this binary through a staged
  file. `DMSG_RELEASE_URL` replaces `https://github.com/ldclabs/ic-panda/releases`
  (tests, mirrors).
- **Stopping for an update** (plan DT10). `restart` and `update` stop a
  running daemon with `daemon/stop {if_idle: true}`: the daemon refuses new
  requests and background passes, waits for the running operation and stops
  only if no approved request, account update or root rotation is left
  unsettled; otherwise it keeps running and answers what is in flight. The
  command asks again every two seconds for `--wait` seconds and then fails with
  `BUSY`, replacing nothing. Locked, unsettled work cannot reconcile, so unlock
  first, or use `--interrupt`: the work is journaled and resumes after the
  restart. An update starts the new daemon and checks the version it reports;
  the new daemon is locked. Requests waiting for a person and unsynced
  versions are kept either way.
- **Diagnostics** (plan §13). `dmsg doctor` checks, without changing
  anything: `config.toml`; the home's permissions (and `owner.token`'s with
  `secure_storage = "file"`); the owner credential in its store; the daemon
  (running, and the same version as this binary); the relay's `/ready` and
  its environment; this clock against the `time` `/ready` reports (over 5 seconds is a
  warning, over 30 a failure: signed requests carry 45-second deadlines); the
  registry's user homes and the configured pins; the hint channel. `--json`
  prints `{ok, checks: [{name, status, detail}]}` with `status` `ok`, `warn` or
  `fail`. The run log has one line per call (method, the tool an agent ran,
  owner or agent, outcome code, milliseconds; steady polls only when they
  fail) and one per change of a background task's outcome (sync, flush,
  web requests, MCP Events, hint channel), so a task failing every round
  while offline logs once. Lines hold names, codes and numbers only: never
  parameters, content, paths, tokens or error messages.
- **At login.** `dmsg autostart install` registers this binary with this home:
  a LaunchAgent `net.dmsg.daemon` on macOS, a systemd user unit `dmsg.service`
  (XDG autostart without a user systemd) on Linux, the user's `Run` value
  `dMsg` running `dmsg start` on Windows. It takes effect at the next login;
  `uninstall` leaves a running daemon alone.

## Commands

```sh
dmsg daemon [--port N]           # foreground; port 0 (default) picks a free one
dmsg status [--json]
dmsg login [--origin dmsg.net|panda.fans] [--no-browser] [--bind]
dmsg unlock [--fast] [--account <id>] | lock | stop
dmsg accounts | switch <account>
dmsg fast-unlock enable | disable
dmsg account create | show | security
dmsg account bind-request <account> | bind-approve <request> --principal <p> | bind-accept <account>
dmsg account unbind <principal> | policy [--freeze|--unfreeze] [--daily N] [--purposes a,b] | recovery-delay <days>
dmsg recovery status | request | complete | dispute
dmsg legacy pair [--origin https://dmsg.net|https://panda.fans] | pairs | cancel <nonce>
dmsg legacy import <file> --nonce N --fingerprint F --principal P | list | report <id> [--offset N] [--limit N]
dmsg legacy file <id> <source> <out> | frozen <id> <file.cbor>
dmsg legacy shared source <source.json> | prepare <source.json> [--version N] | check <draft.json>
dmsg legacy shared request <draft.json> --manager P | propose|consent <draft.json> --manager P --approval <approval.json>
dmsg legacy shared view <key|record.json> | commit <key> | activate <key> | joined <key>
dmsg legacy shared claim-request <key|record.json> --member P | claim <key|record.json> --request <request.json> --approval <approval.json>
dmsg legacy shared grant <key> --archive <id> --member P | receive <key> --grant <id>
dmsg device list | pair-request [--member] | approve <request> --device <id>
dmsg vault add --title T [--type note|login|api|key] [--body B] [--username U] [--url L] [--tag T]... [--secret-stdin]
dmsg vault edit <id> [--title T] [--body B] [--username U] [--url L] [--tag T]... [--secret-stdin]
dmsg vault rm <id> | restore <id> | list [--trash] | search <text> | show <id> [--reveal]
dmsg vault conflicts | resolve <id:revision> [--keep-current]
dmsg file import <path> | list | get <id> -o <path> | verify <id>
dmsg sync | sync status
dmsg profile show | publish [--name N] [--bio B] [--link URL]... | resolve <account>
dmsg profile avatar <png|jpeg|webp> | avatar --remove
dmsg reset --yes
dmsg approvals list [--all] | show <id> | approve <id> | reject <id> | cancel <id> | quote <id> <ledger|neuron>
dmsg attest text <text> | digest (--file F | --sha256 H) | file <path> --text T | action <file|->
dmsg attest history | usage | export <execution> -o <path> | verify <bundle> [--file F]
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
dmsg payment transfers <escrow> | revise <escrow> <leg>
dmsg wallet use [pem] | balance <ledger>
dmsg plan catalog | status | quote <sku> <ledger|neuron> [--method cash|panda]
dmsg plan checkout <id> | advance <id> | order <id> | list | cancel <id> | reconcile <id> | refund <id> [block]...
dmsg handle resolve <name> | price <name> | claim <name> | register <name>
dmsg handle transfer <name> <account> | accept <token> | commit <id> | operations | reconcile <id> --block N
dmsg ops orders | transfers | claims [--after ID] | reconcile-order <id> | process-transfer <id>
dmsg ops revise-transfer <id> --fee F | claim <id> [--refresh] | sweep
dmsg agent principal show | enable [--type person|organization|team|project|other] | publish | refresh
dmsg agent controller register [--name N] [--scope S]... [--audience A]... | retire <gen> | compromise <gen> --from <ms> | rename <gen> [--name N]
dmsg agent grant <subject> --scope S... --audience A... [--days 30] [--generation G] [--relationship R]
dmsg agent revoke <id> [--generation G] [--reason R] | credentials | delegations
dmsg agent id create <label> | list | use <agent> | resident <agent> on|off | credential <agent>
dmsg grants list | add <agent> <capability>... [--days D] | revoke <id>
dmsg audit [--agent owner|A] [--before SEQ] [--limit 50]
dmsg ap [--agent A] [--session S] tools | <tool> [json-input]
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

Accounts. Each account keeps its own workspace on this device, with its own
device keys: a login selects the workspace of the account it is bound to, or
its own while it has none, so logging in with another identity switches
accounts and never sees the first one's data. `accounts` lists them;
`switch <account>` selects one, locked, and `unlock --fast --account <id>`
selects and fast-unlocks it. `reset` erases only the selected one.

Logins. `account security` lists the logins bound to the account. To bind
another one on this device, `login --bind` with it while the account is
unlocked: the request waits for approval and, approved, the new login accepts
at once. From another device, the new login (without an account) runs
`account bind-request <account>`, an administrator device
`account bind-approve <request> --principal <p>`, and the new login then
`account bind-accept <account>` within ten minutes. `account unbind`,
`account policy` (freeze formal signatures, executions a day, allowed
purposes) and `account recovery-delay` are approvals too.

Recovery, when every device is lost: a new device logged in with a bound
login runs `recovery request`; any device of the account sees it in
`account security` and can `recovery dispute` it during the delay (one to
seven days, three by default). After the delay, `recovery complete` takes the
account over, derives the committed root's key once, opens it and rotates to
a root only this device holds; run again, it resumes.

Unlocking through a login opens the root and syncs once. Every write waits
for `dmsg sync` or the scheduled sync; `status` shows pending versions and
conflict copies. An edit that lost a race is kept as a conflict copy on every
device; `vault resolve` adopts it, or keeps the current version with
`--keep-current`. An item in the trash takes only `vault restore` (or
adopting a copy) back. `file import` resumes an interrupted import of the same
file; `file get` writes only a fully verified file to a path that does not
exist yet. `profile publish` publishes exactly the fields given and keeps the avatar;
`profile avatar` publishes an image (PNG, JPEG or WebP, at most 512 KiB) as
the public avatar, which the relay serves to anyone unencrypted, and keeps the
other fields. At the end of a sync while unlocked, chunks nothing needs any
more are deleted (`collected` in the report): replaced versions', lapsed or
abandoned uploads' and channel downloads'. `reset`
erases the workspace, unsent versions included, and starts over as a new
device; a device its account revoked does this on its own.

Approvals are decided by a person: `approvals approve|reject`, `connect
browsers add` and the prompt after `attest …` or `device …` need an interactive
terminal without `--json`; otherwise the request is only created and waits in
the approval center (and the desktop's approval window). A web request shows
its origin as relayed by the browser through connect.dmsg.net, the browser's
label, the account and the payload. `attest verify` needs no daemon: it checks
the signature, the receipt certificate under the configured root from one
of the registry's user homes, their binding, the file when given, and the
device's present status.

`attest action` takes an application action its product prepared, as the
canonical CBOR a page would send to `signAction`
([app-action](../../docs/protocol/app-action.md)): the app's current
registration must admit it and it must name this account. The origin shown
is the action's own; nothing relays it. Approving asks the product
authority (`inspect_app_action`) first, and a refused action fails without a
signature; the request waits no longer than the action is valid.

The CLI's own text (reviews, prompts, summaries) is in Simplified Chinese
when the locale (`LC_ALL`, else `LC_MESSAGES`, else `LANG`) starts with `zh`,
otherwise English; `--json` output, error codes, the daemon's messages and
`--help` stay as they are.

Channels: `create` makes the first epoch at once; a membership change leaves
the channel at `rotation_required` until a member with `can_rotate` runs
`rotate` (the owner's `remove` does it itself). An invited account runs
`accept` with the JSON `invite` printed, then reads from the next epoch;
`history` gives it older ones. `read` prints what this device read; while
unlocked the daemon also reads on the relay's hints. Locked, the account's
hint channel says only that a channel has new messages or a letter arrived,
and the desktop notifies without saying from whom. A command whose reply was
lost is kept in `pending`, and running it again, or `pending --key`, settles it
under the same request ID. After `leave` the relay serves the channel no more
to this account; the device keeps the last state it verified.

Payments go through the approval center like signatures: `inbox pay`,
`plan checkout|advance` and `handle claim|register` only create the request,
which a person approves at the terminal or in the desktop's approval window,
seeing the exact amount, ledger and payer. The payer is the login's Principal
unless `wallet use <pem>` names an identity file; the dMsg account itself
never holds funds. A paid letter is `inbox send --pay` (a quote), then
`inbox pay`; a subscription is `plan quote`, then `plan checkout`. A web
application's checkout arrives as a web request and is quoted
(`approvals quote <id> <ledger|neuron>`) before it can be approved; the page
gets the order's progress. `payment revise` resends an escrow transfer the
ledger refused at the fee it asked for, never above the escrow's limit.
`inbox refund` claims, in one transfer, what a letter's escrow holds for the
wallet: the payment after the acceptance deadline, the unused fee reserve once
every payout succeeded, and deposits the escrow did not take.

Names move between accounts in three steps, the last two within a minute:
the owner's `handle transfer <name> <account>` prints a token, the target
account's `handle accept <token>` authorizes receiving it, and the owner's
`handle commit <id>` moves it; both are approvals. `handle reconcile <id>
--block N` settles a registration whose charge is unknown with its ledger
block. `ops` reads the orders, cash transfers and PANDA commitments the wallet
may see as payer, merchant or operator and repeats their original operations:
reconcile an order, retry a transfer, revise a refused transfer's fee as its
recipient, refresh or reconcile a claim, release expired commitments.
`attest usage` shows this month's formal executions and archives finished
months in the workspace before the home drops them.

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
`vault.read:<tag>`, `vault.write:<tag>`, `vault.secret:<item>`,
`file.read:<tag>`, `channel.read:<channel>`, `channel.publish:<channel>`,
`inbox.read`, `inbox.contact`, `attest.request`, `action.request`,
`agent.delegation.read`, `events.endpoint` (`<name>:*` covers every
target); and at the allowed hosts, each `:<origin>` or `:*` for every
host: `agent.profile.write` (its Agent Profile, at a host or a host's
profile service), `discourse.read`, `discourse.write`, `mail.read`,
`mail.write`, `knowledge.read` and `knowledge.write`, such as
`discourse.read:*` with `discourse.write:https://discourse.dmsg.net`. An
agent's call then reaches only the configured origins its grants name: the
connector refuses the others (`permission_denied`), a room it knows from
another host included, and lists only the rooms of those it reaches; with
none of them the call is `FORBIDDEN`. Without a grant an agent reads only
public profiles (`profile.read`), and of the standard tools uses only its
identity, the delegation tools (a grant or revocation needs the Agent ID to
be a principal's controller) and its inbox, at every configured host. No vault capability reaches an item
tagged `dmsg-…` (the Agent ID and controller keys), not even with `*`, and
such a tag cannot be granted. A resident Agent ID's live grants are copied
beside its key, and while the device is locked it is checked against the
copy.

`dmsg audit` shows who did what on this device, newest first: the owner's
capability grants and revocations, Agent ID creation, credentials and
residency, and approval decisions, with the way in (`cli` or `gui`); and the
calls agents make through `dmsg mcp` (`mcp`) or the owner makes as an Agent ID
(`dmsg ap`, the desktop), except those that only read the agent's own
connector or poll. An entry names the method or tool, its target ID and the
outcome (`ok` or the error code), never content; the record stays on the
device, is readable while locked, and keeps a year or 10,000 entries.

`dmsg ap <tool> [input]` runs one MCP tool as an Agent ID with the same input
and result; the tool name may leave out `agent_protocols_` or `dmsg_`, and
`dmsg ap tools` lists them. The owner needs no grant for a standard tool.
Each command runs in the connector session
`--session` (`cli` by default; the desktop's is `gui`), which keeps what that
session was shown of each room, so its writes are checked against its own
view and not against what an MCP session read. For example:

```sh
dmsg ap --agent bob mail_mailbox_publish '{"routes":["https://mail.example"]}'
dmsg ap --agent alice mail_send '{"to":"did:agent:…/mail/…?route=https://mail.example","text":"hello"}'
dmsg ap --agent bob mail_sync
dmsg ap --agent alice knowledge_query '{"service":"https://knowledge.example","tag":"caching"}'
```

## MCP

`dmsg mcp` serves MCP on stdio (line-delimited JSON-RPC) for the Agent ID
whose credential is in `DMSG_AGENT_TOKEN`: `initialize` (MCP 2025-11-25 down
to 2024-11-05), or the 2026-07-28 lifecycle, where `server/discover` takes its
place, every request names its version and the client's capabilities in
`_meta` (else `-32602`; an unknown version is `-32022`), and results carry
`resultType: "complete"` (lists and reads also `ttlMs: 0` and
`cacheScope: "private"`); `ping`, `tools/list|call`,
`resources/list|templates/list|read|subscribe|unsubscribe`,
`subscriptions/listen` and `events/list|poll|stream`. Requests run
concurrently. Each process is one MCP session, with its own presented heads
(local connector §4.2). It reads neither the owner credential nor any key: the
daemon runs the Agent ID's connector and checks its capabilities. For Claude
Code, after `dmsg agent id create claude` and `dmsg agent id credential claude`:

```sh
claude mcp add dmsg --env DMSG_AGENT_TOKEN=dmsg-agent-… -- dmsg mcp
```

Tools: the 33 standard `agent_protocols_*` tools of the Agent Protocols
local connector (`agent-protocols` 0.12: identity, delegation, rooms, inbox
and drafts, profile, Agent Mail mailboxes and letters, Agent Knowledge
submissions and reads), as this Agent ID, against the configured hosts, each
Discourse, Mail and Knowledge tool behind its protocol's read or write
capability and `agent_protocols_profile_update` behind
`agent.profile.write`, at the origins the grants name (the description
names it; the resources of rooms, drafts and mail need the same read
capability as their tools); and
`dmsg_account_status`, `dmsg_vault_list`, `dmsg_vault_read` (the secret
field only as below), `dmsg_channel_list|read|publish`, `dmsg_inbox_list`,
`dmsg_profile_read`, `dmsg_attest_request` (an approval request with source
`agent:<did>`, which only a person decides) and `dmsg_approval_status` (only
the agent's own requests). A refused call returns `isError` with the
protocol's `code`, such as `permission_denied`, `room_head_mismatch` or
`FORBIDDEN`.

More `dmsg_*` tools, each behind its capability: `dmsg_vault_read` with
`secret: true` returns the secret field (`vault.secret:<item>`);
`dmsg_vault_write` adds or edits an item that keeps a granted tag, against a
`base` like the CLI (`vault.write:<tag>`), and returns the item only with
`vault.read` on it; `dmsg_file_read` returns up to
1 MiB of a file of at most 64 MiB from `offset`, base64url, after checking
the whole file (`file.read:<tag>`); `dmsg_inbox_list` lists letters without
their text and `dmsg_inbox_read` returns one, marking it read with
`mark_read` (`inbox.read`); `dmsg_inbox_contact` writes a free letter, and a
recipient who charges only returns a quote (`inbox.contact`);
`dmsg_action_request` asks the owner to sign an application action, the
canonical CBOR a web page passes to `signAction` (`action.request`); and
`dmsg_delegations_list` lists the principal's credentials
(`agent.delegation.read`). 56 tools in all.

Resources (local connector §4.4): `agent-protocols://identity/current`,
`hosts`, `rooms`, `inbox/pending`, `drafts/held`, `mail/mailboxes`,
`mail/messages`, and per known room `rooms/{room_id}` with `/members`,
`/timeline` (the latest 100 records), `/types` and `/agent-status`, as
`application/json`. They are read from local state through the read-only
tool that shows the same thing, and the read is not saved: it presents no
head, claims nothing and marks nothing read. `resources/subscribe` and a
`subscriptions/listen` stream (its `resourceSubscriptions` and
`resourcesListChanged`; tool lists never change) are told
`notifications/resources/updated` and `list_changed` after any connector call
of the Agent ID changes what they watch; listen notifications carry the
stream's `subscriptionId`.

MCP Events. An agent on this machine has no public webhook address, so the
dMsg relay receives webhooks for it: `dmsg_events_endpoint_create` (with
`events.endpoint`) returns a callback URL on the relay's origin and a
`whsec_` secret, which the agent gives the upstream MCP server in
`events/subscribe` (`delivery: {mode: "webhook", url, secret}`). The relay
answers the upstream's `verification` itself, checks the Standard Webhooks
`v1` signature of every delivery and keeps it sealed to this device for seven
days; the daemon fetches while unlocked (on the relay's `mcp-event` hint, and
every minute), checks `v1` again and, when the server bound with
`dmsg_events_bind` publishes `/.well-known/mcp-webhook-jwks.json`, the
end-to-end `v1a`, dropping a delivery whose `v1a` fails. `dmsg mcp` declares
`capabilities.events` with one event type, `dmsg.relay.delivery` (`poll` and
`push`): its data is `{endpoint_id, subscription_id, kind, upstream,
received_at, verified: {v1, v1a}, context}`, `kind` one of `event`,
`verification`, `gap`, `terminated`, `relay_gap` (the relay dropped
`dropped` unfetched deliveries) and `refresh_due` (the bound
`refreshBefore` is near; the daemon renews nothing). `events/stream` pushes
`notifications/events/active|event|heartbeat|error|terminated` with a
heartbeat at most every 25 seconds and ends on `notifications/cancelled`.
The cursor is the local sequence and survives restarts; a null cursor reads
from the oldest delivery not yet acknowledged, and `dmsg_events_ack` lets the
device forget. Without `endpoint_id`, `endpoint_ids` or `session` (the
endpoint's `context.session`), a reader gets the endpoints created in or
bound to its own MCP session. Tools for clients without the events
extension: `dmsg_events_endpoint_create|list|rotate|delete`,
`dmsg_events_bind`, `dmsg_events_next` (`wait_ms` up to 25 seconds) and
`dmsg_events_ack`. Event data comes from other services: treat it as
untrusted input. The owner's `dmsg events endpoint
create|list|rotate|delete`, `events bind|poll|stream|ack|status` give the
same shapes.

## Daemon protocol

The daemon listens on `127.0.0.1` only.

- `GET /daemon/status`, `POST /app/v1` (JSON-RPC 2.0) and `GET /ws/app/v1`
  (the same methods over a WebSocket, plus `event` notifications
  `{seq, type, data}`: `session/changed`, `vault/changed`, `sync/changed`,
  `approval/changed`, `connect/changed`, `connect/hint`, `channel/changed`,
  `inbox/changed`, `legacy/changed`, `mcp-events/changed`, `resync`) need
  `Authorization: Bearer <owner token>`; any request with an `Origin` header
  is refused with 401. Errors carry `error.data.code` such as `AUTH_REQUIRED`,
  `LOCKED`, `VERSION_CONFLICT` or `EXECUTION_UNKNOWN`.
- Methods: `app/initialize`, `daemon/status`, `events/poll`,
  `session/status|login {origin, bind?}|login_result|accounts|switch|unlock {fast?, account?}|lock|fast_unlock`,
  `account/create|show|security|bind_request|bind_accept|bind_approve|unbind|policy|recovery_delay`,
  `recovery/status|request|complete|dispute`,
  `device/pair_request|approve|capabilities|formal_approve|revoke`,
  `vault/put|edit|delete|restore|list|show|conflicts|resolve`,
  `file/import|export|verify` (absolute paths), `sync/run|status`,
  `profile/show|publish|resolve`, `approval/list|show|respond|cancel|quote`,
  `attest/request|action {action: base64url CBOR}|history|usage|export|verify`,
  `connect/invite|candidates|grant|pairings|revoke|sync|status`,
  `channel/list|show|create|invite|join|control|leave|rotate|send|send_file|sync|messages|download|history|reactivate|pending|resume`,
  `channel/owner_propose|owner_accept|owner_commit|sponsor_propose|sponsor_accept|sponsor_commit`,
  `inbox/policy|configure|send|order|outgoing|pay|escrow|refund|list|mark`,
  `payment/transfers|revise`, `wallet/use|balance`,
  `plan/catalog|status|assets|quote|checkout|advance|order|checkouts|cancel|reconcile|refund`,
  `handle/resolve|price|claim|register|transfer|accept|transfer_commit|operations|reconcile`,
  `ops/orders|transfers|claims|reconcile_order|process_transfer|revise_transfer|claim|sweep`,
  `approval/reconcile`,
  `legacy/pair|pairs|cancel|import|list|report|file|frozen` (absolute paths),
  `legacy/shared_source|shared_prepare|shared_check|shared_request|shared_propose|shared_consent|shared_view|shared_commit|shared_activate|shared_claim_request|shared_claim|shared_joined|shared_grant|shared_receive`,
  `agent/principal|enable|register|retire|compromise|rename|publish|refresh`,
  `agent/grant|revoke|credentials|delegations`,
  `agent/ids|create|use|resident|credential`, `grants/list|add|revoke`,
  `audit/list {agent?, before?, limit?}`, `ap/call {agent?, session?, tool, input}`,
  `mcp-events/create|list|rotate|delete|bind|poll|ack|status`,
  `hint/status`, `workspace/reset`, `daemon/stop {if_idle?}` →
  `{stopped, in_flight: {approvals, account_update, root_rotation}}`. While a
  stop decides or after it, every method but `app/initialize`,
  `daemon/status`, `session/status` and `events/poll` answers `UNAVAILABLE`.
- An Agent ID's credential (`Bearer dmsg-agent-…`) reaches only `POST /app/v1`
  and only `mcp/tools`, `mcp/call {tool, input, mcp_session?}`,
  `mcp/resources`, `mcp/read {uri}` and `mcp/changed {after, wait_ms}` (the
  count of connector calls, waiting up to 25 seconds for it to pass `after`),
  as that Agent ID; a waiting `dmsg_events_next` or `mcp/changed` is not
  activity; the owner credential never reaches those. A tool answers
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
