# dmsg

The dMsg command line client. `dmsg daemon` runs the daemon, the only process
that holds keys and state; every other command asks it over loopback. The
cryptography, storage and protocol logic live in
[`dmsg_client`](../dmsg_client/README.md).

## Home

`dmsg logout` signs out of the selected account and removes its saved login
and fast-unlock key. Its encrypted workspace and other accounts stay on this
device. The next browser login opens the same workspace with fresh delegation
targets. The desktop exposes this action in the account menu as **Log out**.

`--home <dir>`, else `$DMSG_HOME`, else `~/.dmsg`:

| Path | Content |
| --- | --- |
| `config.toml` | Deployment parameters and policies; the only place to change them. Without it the production deployment built into the binary applies ([`production.toml`](../dmsg_client/production.toml)) |
| `preferences.json` | The desktop's file settings: `files_mount` (allowed after a system dialog, beside `config.toml`'s) and `files_trash_days` (instead of `config.toml`'s). Only the desktop's main process writes it; the daemon reads it at start and on `files/preferences_reload`, never from a request's values |
| `owner.token` | Owner credential with `secure_storage = "file"`, readable only by its owner |
| `daemon.json` | Port, process and instance ID of the running daemon |
| `dmsg-daemon.lock` | Held by the running daemon: one daemon per home |
| `logs/dmsg-YYYY-MM-DD.log` | The daemon's run log, one file per UTC day, kept 30 days (`dmsg logs`) |
| `logs/stderr.log` | The stderr of a daemon started by `dmsg start` (rewritten each time) or at login: what fails before the run log opens |
| `<environment>/accounts/<account>/workspace.sqlite` | An account's workspace on this device: its own device keys, encrypted versions and chunks, sealed records, sync and backup jobs |
| `<environment>/accounts/<account>/agents.sqlite` | Agent Protocols connector state of the account's hosted Agent IDs; no private key (see [`dmsg_agent`](../dmsg_agent/README.md)) |
| `<environment>/accounts/<account>/blobs/<d0d1>/<digest>` | The file library's ciphertext chunks, one file per chunk named by its SHA-256; `blobs/tmp/` holds chunks being written |
| `<environment>/accounts/<account>/open/<nonce>/<name>` | Plaintext copies of library files opened for another program (`dmsg files open`), owner-only and kept out of Time Machine, removed after 30 minutes, on lock and on exit |
| `<environment>/accounts/<account>/mount/<random>` | A file arriving over the mounted disk, owner-only and kept out of Time Machine, until it is imported; removed on lock |
| `<environment>/logins/<principal>/` | The workspace of a login without an account; it becomes `accounts/<account>/` once the login has one |
| `<environment>/current` | The account selected last, which a restart and fast unlock use |

A home without `config.toml` uses [`production.toml`](../dmsg_client/production.toml),
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
files_max_bytes = 4294967296     # largest file the file library takes (default 4 GiB)
files_trash_days = 30            # days a trashed library file is kept, 1 to 365
files_mount = false              # allow mounting the library as a disk (loopback WebDAV); preferences.json may too
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

[wallet]                         # team wallets; production names the hub
hub = "<dmsg_wallet_hub canister>"
bitcoin_api = "https://mempool.space/api"          # an Esplora API
solana_rpc = "https://api.mainnet-beta.solana.com"
evm_rpc = { "1" = "https://ethereum-rpc.publicnode.com", "8453" = "https://mainnet.base.org" }
```

Outside `local`, every URL must be HTTPS and `root_key`/`login_url`/`connect_url`/`agent_ca` are refused.
The relay is also the Agent Delegation service of the account's principal.
`[legacy]` is needed only for frozen comparisons and shared channel
inheritance; frozen evidence of any other canister or cutover is refused. The
built-in settings name the three frozen channel canisters, the name identity
and the cutover sealed by SNS proposals 553–558. They list no OSS bucket: the
old buckets cannot freeze folders yet, so a frozen comparison of a channel
with attachments stays incomplete.
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

Fast unlock turns on with every login unlock unless `fast-unlock disable`
turned it off, and lasts thirty days after the last login unlock, as long as
the login (`dmsg login` asks Internet Identity for thirty days). On macOS the
Secure Enclave keeps its key (`src/enclave.rs`, CryptoKit through
`swift/Enclave.swift`, which `build.rs` compiles; building on macOS needs the
Xcode Swift toolchain): each fast unlock asks for Touch ID, or the login
password without it, and gives up after a minute unanswered; the login is
kept sealed under the local data key and comes back with it. Elsewhere the
key sits in the credential store, nothing asks for the person and no login is
kept. Team-wallet cosigner keys use the Secure Enclave the same way.

`device-bound enable [--biometry-only]` binds the workspace to this Mac:
another Secure Enclave key, in a credential-store entry of its own, joins
every unlock, the login's included, and alone opens fast unlock. Touch ID is
asked once to check the key. It reports what stays outside it (the device
key kept with background online, resident Agent IDs); copies of the
workspace made before keep opening with a login. Signing out, turning fast
unlock off and a failing key keep its entry; `device-bound disable` or a
reset removes it. `--biometry-only` takes only the fingerprints enrolled
now: after they change the key reports itself lost, while a missing Touch ID
keyboard is only unavailable.

Approval keys are Secure Enclave keys the account registers per device
(`device key register|request|approve|retire`); `account hardware on|off
[--payment] [--attestation]` requires their signature for account,
device and Agent principal changes, and optionally payments and formal
attestations. On a device with a registered key, a high-risk or protected
approval asks Touch ID through the key, once, and the key signs the
approval; a required approval on a device without one fails with
`APPROVAL_KEY_REQUIRED`. With keys required, a device without a working key
runs `device key request`, which asks Touch ID there and prints a request,
and an administrator device with a key runs `device key approve`.

Every workspace also has a hardware anchor: a Secure Enclave key that never
asks and works once the Mac has been unlocked after it started. The daemon
signs with it when it starts and whenever it selects a workspace. A
workspace copied to another Mac, by Migration Assistant or a restored
backup, cannot sign there and is moved (`state: "moved"`): it never unlocks
and receives no notifications with the old device's identity, and nothing in
it is erased. `device/moved_set_aside` moves it whole to
`moved/<account>-<device>/` and starts a new device of the account, which then
pairs or recovers; `device/moved_list` lists the copies and
`device/moved_discard {id}` deletes one. A workspace that already held
Secure Enclave keys when its anchor was made counts as moved when the first of
them fails on this Mac; the first that opens settles it.

Older Macs without Secure Enclave use a device-bound digest of IOPlatformUUID
to detect ordinary system copies; this is weaker than an enclave anchor.
Resident Agent credentials and notification receivers stay inactive while
hardware is foreign or still unverified. Existing approval and cosigner keys
also participate in late-anchor checks; keys created afterwards do not.

While the old device is still active, the new device can take over from a
copy without the old Mac (`dmsg device moved`). After `login`,
`approve-successor <id>` asks to succeed the copy's device and makes a
high-risk approval request on this Mac; approving it enrolls the new device,
signed as the copy's device, and rotates the content root to it, and `unlock`
binds it. The copy's device must be an administrator, and the account must
not require approval keys for device control. `import <id> --succession
<task>` then brings in the copy's file library, only adding, and settles the
succession's content step; the other steps run as in any succession, and the
copy no longer opens once the old device is revoked. A wallet the copy's
device created still binds it without a device ID: where no other device is
bound to it, `run <task> revoke` first asks to take over that wallet's
bindings; otherwise take them over with `wallet join <id> --exclusive`,
which removes this account's other bindings, before revoking. A copy whose
device bound its workspace to its hardware does not open here (`FORBIDDEN`).
The import source is opened read-only. The permitted local succession keeps
its mutable journals in `moved-operations/<copy>/`, separately from that source;
discarding the copy also removes those journals.

The hardware creation tests run without a prompt:
`cargo test --locked -p dmsg_cli --bin dmsg enclave::tests::creates_keys_without_asking -- --exact --nocapture`
and `enclave::tests::anchors_sign_without_asking` the same way.
Run it in an active macOS login session with the device unlocked. Hardware and
biometric capability checks do not guarantee that the security service will
permit key creation at that instant. A native failure now keeps its negative
`OSStatus` in the error; for example, Apple's
[`errSecInteractionNotAllowed`](https://developer.apple.com/documentation/security/errsecinteractionnotallowed)
is `-25308`. Inspect the matching test process's CryptoTokenKit logs for the
underlying rejection. A system refusal must not remove `.userPresence`, change
the key's `WhenUnlockedThisDeviceOnly` protection, or fall back to a software key.

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
  (left to `brew upgrade dmsg`). When this binary is signed by a Developer ID
  team (macOS), an installed one stays only if it verifies and is signed by
  the same team, checked before anything runs it: a shared one that fails is
  replaced, and a Homebrew one that fails gives way to the shared location.
  The desktop app runs its bundled copy only this way and then runs the
  installed one, so the CLI, agents and the app share one daemon. `--json`
  reports `{action, path, version, on_path}` with `action` one of
  `installed`, `upgraded`, `replaced`, `current`, `kept_newer`, `homebrew`.
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
  versions are kept either way. `dmsg start`, which the desktop app runs on
  every connection, also restarts a running daemon older than its binary,
  such as one left running while the app upgraded the installed binary: it
  asks once and, when work is in flight, leaves that daemon running.
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
  while offline logs its unchanged outcome once. Separate `failure` lines
  record the originating subsystem, allowlisted cause and HTTP status when
  available: timeout, DNS/TLS/connect/closed connection, response decoding,
  IC rejection/certificate, or storage permission/full/busy/corruption errors.
  These are emitted before a lost mutation reply becomes `EXECUTION_UNKNOWN`;
  that code still requires reconciliation, never blind resubmission.
  `sync/pass` adds the failed sync stage. For example:
  `failure source=relay reason=http http_status=503` followed by
  `sync/pass stage=export_page ms=8037 outcome=UNAVAILABLE source=relay reason=unavailable`.
  Lines hold fixed names, codes and numbers only: never parameters, content,
  account/device IDs, URLs (including signed URLs), headers, paths, tokens,
  raw server messages or panic payloads. Unknown tools are omitted; unknown
  methods use `unknown`. Library diagnostics pass a second strict allowlist
  at the file writer; other library targets and `RUST_LOG` verbosity are
  ignored. Panic records contain only a source filename and line. On Unix
  the log directory is owner-only and newly created log files use mode 0600.
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
dmsg device-bound enable [--biometry-only] | disable
dmsg account create | show | security
dmsg account bind-request <account> | bind-approve <request> --principal <p> | bind-accept <account>
dmsg account unbind <principal> | policy [--freeze|--unfreeze] [--daily N] [--purposes a,b] | recovery-delay <days>
dmsg account hardware on [--payment] [--attestation] | off
dmsg recovery status | request | complete | dispute
dmsg legacy delete-channel <archive-id> <canister/channel/id>
dmsg channel delete-legacy-draft <channel-id>
dmsg legacy pair [--origin https://dmsg.net|https://panda.fans] | pairs | cancel <nonce>
dmsg legacy import <file> --nonce N --fingerprint F --principal P | list | report <id> [--offset N] [--limit N]
dmsg legacy file <id> <source> <out> | frozen <id> <file.cbor>
dmsg device list | pair-request [--member] | approve <request> --device <id>
dmsg device key status | register [--biometry-only] | request [--biometry-only] | approve <request> --device <id> | retire [--device <id>]
dmsg device succession request <device> | review <request> | approve <request> --device <id> | list | status <id>
dmsg device succession run <id> <step> | skip <id> content|native_addresses [--keep a,b] | cancel <id>
dmsg device transfer receive [--session S] [--succession ID] [--wait] | status | cancel
dmsg device transfer send <session> [--to <device>] [--address host:port] [--succession ID] [--approval ID] [--wait]
dmsg device moved list | set-aside | import <id> [--succession ID] | approve-successor <id> | discard <id>
dmsg vault add --title T [--type login|api|key|ssh|wallet|card|identity|server|database|license|note] [--generate]
                [--notes N] [--username U] [--url L] [--tag T]... [--field [TYPE:]LABEL=VALUE]... [--otp URI] [--secret-stdin]
dmsg vault edit <id> [--title T] [--notes N] [--username U] [--url L] [--tag T]... [--clear-tags] [--favorite|--no-favorite]
                [--field [TYPE:]LABEL=VALUE]... [--rm-field ID|LABEL]... [--otp URI] [--secret-stdin]
dmsg vault rm <id>... | restore <id> | duplicate <id> [--title T] | list [--trash] | search <text>
dmsg vault show <id> [--reveal] [--revision R] | history <id> | revert <id> <revision> | otp <id> [--field F]
dmsg vault conflicts | resolve <id:revision> [--keep-current] | import <export> [--yes [--all]] | health
dmsg files ls [folder] [--trash] | tree | show <id> | search <text>
dmsg files add <path>... [--to <folder>] [--keep-both] | mkdir <path> | mv <id>... <folder> | rename <id> <name>
dmsg files tag <id> +t|-t... | note <id> [text] | star <id> [--off]
dmsg files get <id> [--version V] -o <path> | open <id> [--version V] | opened [--cleanup [nonce]] [--save]
dmsg files verify <id> | verify --all | versions <id> | version-use <id> <version> | version-rm <id> <version>
dmsg files rm <id>... | restore <id>... | purge [<id>...] | usage | local-only
dmsg files share <channel> <id> [--version V] [text] | save <channel> <seq> [--to <folder>]
dmsg files backup <id|folder>... [--all-versions] | backup-status | backup-cancel <id> <version>
dmsg files cloud ls [--refresh] | refresh | get <upload> [--to <folder>] | rm <upload>... | verify <upload> | usage
dmsg files evict <id> [--version V]
dmsg files link <id> [--version V] [--days 7] [--max-downloads N] [--password] | links [--refresh] | link-revoke <share_id>
dmsg files mount [--status] | unmount
dmsg sync | sync status
dmsg profile show | publish <document.json> | retry | resolve <account>
dmsg profile avatar <png|jpeg|webp>
dmsg reset --yes
dmsg approvals list [--all] | show <id> | approve <id> | reject <id> | cancel <id> | quote <id> <ledger|neuron>
dmsg approvals check-file <id> <file> | reconcile
dmsg attest text <text> | digest (--file F | --sha256 H) | file <path> --text T | action <file|->
dmsg attest history | usage | readiness | export <execution> -o <path> | verify <bundle> [--file F]
dmsg device capabilities <id> --set a,b,c | formal-approve on|off | revoke <id>
dmsg connect browsers add [--copy-link] | list | revoke <pairing>
dmsg connect requests | sync
dmsg channel list | show <id> | create <name> [--type direct|collaboration|distribution]
dmsg channel invite <id> <account> [--role member|publisher] | accept <invitation>
dmsg channel members <id> | role <id> <account> <role> | remove <id> <account> | leave <id> | archive <id>
dmsg channel rotate <id> | rekey <id> | reactivate <id> | history <id> <account> <from> <to>
dmsg channel send <id> [text] [--file <id[@version]>] [--message-id M] | read <id> [--after N] [--backfill]
dmsg channel download <id> <seq> --out <path> | pending <id> [--key K]
dmsg channel outbox <id> [--discard M] | search <id> <query> | receipts <id> | uninvite <id> <invitation>
dmsg channel settings <id> [--name N] [--mute true|false] [--hide true|false]
dmsg channel transfer|sponsor propose <id> <account> | accept <packet> | commit <packet>
dmsg channel sponsor cancel <id>
dmsg inbox policy | configure closed|contacts|free|paid [--price P] [--payee P] [--allow A]...
dmsg inbox send <recipient> <text> [--pay] [--invitation I] [--order O] | pay <recipient> <order>
dmsg inbox order <recipient> <order> | outgoing | escrow <order> | refund <order>
dmsg inbox list | mark <order> [--archive]
dmsg payment transfers <escrow> | revise <escrow> <leg>
dmsg payer use [pem] | balance <ledger>
dmsg wallet list | show <wallet> | principal | quote | create <quote> --name N [--member id:role[:label]]... | refund <quote>
dmsg wallet join <wallet> [--replaces <device>... | --exclusive] | unbind <wallet> <device>... | bindings <wallet>
dmsg wallet proposals <wallet> | proposal <wallet> <id> | send <wallet> --from A --asset X --to T --amount N --max-fee F
dmsg wallet propose <wallet> <intent.json> | member-agents <wallet> [--agent did:agent:…]... | approve|reject|veto|cancel <wallet> <id> | freeze <wallet>
dmsg wallet execute|broadcast|settle|reconcile <wallet> <id> | report <wallet> <id> <plan> broadcast|settled|failed
dmsg wallet balances|audit|refuel <wallet> | cosigners | cosigner-new <label> [--software]
dmsg wallet native-status [wallet] | native-create [--software] [--biometry-only] | native-protect [--biometry-only] | native-unprotect <index>
dmsg wallet native-key <wallet> <label> | nc-migrate <wallet> <from> [--l2] [--start]
dmsg plan catalog | status | quote <sku> <ledger|neuron> [--method cash|panda]
dmsg plan checkout <id> | advance <id> | order <id> | list | cancel <id> | reconcile <id> | refund <id> [block]...
dmsg handle resolve <name> | price <name> | claim <name> | register <name>
dmsg handle transfer <name> <account> | accept <token> | commit <id> | operations | reconcile <id> --block N
dmsg ops orders | transfers | claims [--after ID] | reconcile-order <id> | process-transfer <id>
dmsg ops revise-transfer <id> --fee F | claim <id> [--refresh] | sweep
dmsg agent principal show | enable [--type person|organization|team|project|other] | publish | refresh
dmsg agent controller register [--name N] (--unrestricted | --scope S... --audience A...) | retire <gen> | compromise <gen> --from <ms> | rename <gen> [--name N]
dmsg agent grant <subject> --scope S... --audience A... [--days 30] [--generation G] [--relationship R] [--replaces ID]
dmsg agent revoke <id> [--generation G] [--reason R] | credentials | delegations
dmsg agent id create <label> | list | use <agent> | resident <agent> on|off | credential <agent>
dmsg agent id uncredential <agent> | pause <agent> on|off | remove <agent> | profile <agent>
dmsg grants list | add <agent> <capability>... [--days D] | revoke <id>
dmsg audit [--agent owner|A] [--before SEQ] [--limit 50]
dmsg ap [--agent A] [--session S] tools | <tool> [json-input] | agent-protocols://<resource>
dmsg mcp                         # stdio MCP server; needs DMSG_AGENT_TOKEN
```

`--json` prints one JSON value per line; errors are `{"error": {"code", "message"}}`.
`dmsg status --json` prints `{"running": false}` and exits with 3 when no
daemon runs.

A new device: `login`, then `account create` for a new account, or
`device pair-request` and, on an administrator device,
`device approve <request> --device <id>`, a high-risk approval request whose
approval also rotates the content root to the new device; `unlock` on the new device then binds it.

On an account at its plan's device limit, a new device replaces an
administrator device instead: after `login`, `device succession request
<device>` names the device it replaces and prints a request with that
device's role and capabilities and, where the Secure Enclave keeps them, new
approval and cosigner keys (Touch ID asks once for each). On the old device,
`device succession review <request>` shows it with the team wallets it
covers, and `device succession approve <request> --device <id>` is a
high-risk approval request that enrolls the new device beyond the limit and
rotates the content root to it; `unlock` on the new device binds it. From
then on the old device approves only the handover, and it stops unlocking
seven days after the new device joined. Each device keeps the steps it saw
done (`list`, `status <id>`) and runs its own with `run <id> <step>`: the old
device registers the new approval key (`approval_key`) and rotates its
cosigner keys in every wallet (`cosigners`); the new device confirms the
rotation, joins its wallets (`wallets`) and finally retires the old wallet
bindings and revokes the device (`revoke`). The old wallet identity remains
bound until native-address sweeps finish, so it can read private L1 plans.
A creator binding without a device ID is named through an ordinary wallet
join approval before enrolling the successor. Keeping both devices bound
needs a free wallet binding slot; the existing four-binding limit still applies.
After approving these preliminary wallet joins, generate a new succession
request on the new device because its proof binds the account version.
Local content moves with a device transfer
for the succession (below), which settles the `content` step on both
devices. `run <id> native_addresses` lists the native addresses still bound
to another device's key, each moved with `wallet nc-migrate`, and settles the
step once none is left; `skip <id> native_addresses` leaves them, and
`skip <id> content --keep a,b` leaves content, listing what stays. `cancel` stops a succession and keeps what was done; revoking the new
device undoes its enrollment.
List/status refresh account evidence; revocation is complete only after the
root rotation is ready. A failed wallet-hub read never means there are no wallets.
Skipped file records or unsynced vault changes leave the content step open.

`device transfer` moves the file library from one device of the account to
another over the local network, end to end encrypted
([dmsg-device-transfer/1](../../docs/protocol/device_transfer.md)). On the
receiving device, `receive` opens a session on a random port, advertises it
with Bonjour on macOS and prints the command for the sender: `send <session>
--to <device>`, with `--address host:port` where Bonjour does not find the
receiver. With `--succession <id>` on both devices the succession names the
receiver. The receiver listens only during its session, one connection at a
time, and closes it once the transfer completes, nothing connects for two
minutes, `cancel` stops it or the device locks; a broken transfer resumes
with `receive --session <session>`. It only adds: records already there stay
as they are, and nothing is deleted. `status` shows the running or last
transfer; `--wait` follows it to the end.

Sending first creates a high-risk local export approval for the receiving
device, session and current library manifest. An interactive terminal can
approve it in place; otherwise approve it in the approval center and repeat
the send command with `--approval <id>`. Permission is consumed once, and a
changed library or an interrupted send requires another approval. No bytes
leave before this approval and the peer's certified identity checks.

Hidden values
leave the daemon only with `vault show --reveal` (the desktop asks for one
field at a time); file keys never do.

If account creation succeeds but its initial content root fails, `status`
shows `root_required` with `setup_pending: true`. After reconnecting or
logging in again, repeat `account create` to finish the same account. The
client reconciles the journal and checks certified state first: a live initial
reservation resumes, an invalidated one is replaced only before any root was
committed, and a committed root is opened without rotating it. An unapproved
device still needs device approval or recovery.

Vault items. An item has a type, a title, tags, notes, attached library files
and an ordered list of fields; a new item starts with its type's fields
(a login's user name, password and website, a card's holder, number, expiry,
security code and PIN, and so on). `--field` fills a field of that name,
its label or a type's own field by ID (`--field host=db.example`), and adds
others: text, url, email, phone, date (`YYYY-MM-DD`), month (`YYYY-MM`) or
concealed. Concealed fields and one-time passwords (`--otp`, an
`otpauth://totp/…` URI or a base32 key; `vault otp` prints the current code)
are hidden. `--secret-stdin` sets the type's main secret and keeps every
line, so a PEM or SSH key survives;
`--type ssh --generate` makes a new Ed25519 key, and a pasted OpenSSH private
key fills in its public key and fingerprint. Items tagged `dmsg-` are dMsg's
own keys (Agent IDs, Agent Delegation controllers): nothing edits, duplicates
or trashes them. `history` lists every version and `revert` makes an earlier
one current again as a new version. `import` reads a 1Password (`.1pux`,
CSV), Bitwarden (unencrypted JSON, CSV), Chrome, Firefox or Safari export on
this device and shows what it holds; `--yes` adds the items not already here
(same title, user name and website), `--all` those too. Delete the export
afterwards: it holds every secret in plain text. `health` lists weak and
reused passwords, addresses without TLS and expiry dates passed or within 30
days, by item and field, never by value. List and security-check reads load
current encrypted versions together, then release the client between item
decryptions. Import file reading and parsing run outside the client lock;
locking cancels the pending import, and the selected items and sync jobs
are still committed atomically after checking the preview digest.

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

The daemon watches the selected account's certified public recovery digest,
including while locked with background online disabled. Ordinary security reads
update this observation; an independent loop checks for changes every 10 seconds
and fetches security about once a minute when needed. `dmsg status` warns about a
pending recovery and points to `dmsg recovery dispute`; `--json` includes
`recovery` (`account`, `pending`, `digest`, `observed_at_ms`, `stale`). Null means
not yet checked; after two minutes without a fresh observation the state is
stale, and an existing warning remains. The observation survives daemon restarts.
When the request changes, the daemon expires the account security display
snapshot, then `recovery/changed` wakes the shared notification policy. Incident
identity and phase coverage persist across daemon or desktop restarts. Other
explicitly registered local accounts receive minimal notification projections
through scoped credentials without switching the selected workspace or loading
content roots. A stopped presenter, sleeping/offline device or disabled OS
permission cannot provide timely local alerts. The warning is informational;
cancelling still uses the existing authenticated device approval flow. The
[notification contract](../../docs/protocol/notifications_zh.md) describes remote
capability negotiation, preferences and provider limits.

Unlocking opens the root and returns as soon as local content is readable;
the daemon then syncs once in the background, even with periodic sync disabled.
A matching stored root uses the security already checked on unlock without
contacting the relay. Content sync and flush passes serialize submissions,
but release the client during network waits so local reads, edits and locking
remain available. Status includes `syncing` and the last sync error; the log's
`sync/pass` records each pass's duration and failed stage without content or
identifiers; `last_sync.stage` also identifies that step. An unlocked account
automatically retries `NETWORK_UNAVAILABLE`, `UNAVAILABLE` and `EXECUTION_UNKNOWN` failures after
60 seconds, even with periodic sync disabled. The background loop checks
every ten seconds when idle; other background work can delay the retry.
Success clears the failure. Locking pauses these retries; integrity,
permission and other permanent failures require attention rather than retrying.
Every write waits for the next sync pass (`dmsg sync`, the scheduled sync, or
the one after unlock); `status` shows pending versions and conflict copies.
An edit that lost a race is kept as a conflict copy on every
device; `vault resolve` adopts it, or keeps the current version with
`--keep-current` (the desktop also merges them part by part). An item in the trash takes only `vault restore` (or
adopting a copy) back. `profile show` and `profile resolve` return the official PrincipalDocument.
Edit its display fields in a JSON file and run `dmsg profile publish <file>`;
the account mutation checks the original updated_at and reports directory
publication separately. `dmsg profile retry` reconciles an uncertain document mutation and retries publication.
`profile avatar` uploads a public raster image and returns its immutable URL;
put the returned `public_url` in avatar_url before publishing. Production uploads return
`https://assets.dmsg.net/avatars/<account>/<upload>`; the client does not derive this
from the API origin. Uploading never publishes a draft.
The desktop reads `profile/avatar_image { avatar_url }` for both new uploads and
legacy HTTPS image links. A successful publication (including a retry) emits
`profile/changed` so the sidebar rereads the published name and avatar.


Files. The file library (`dmsg files`) keeps files on this device only, in
folders, each file as a set of versions with its own key, and nothing of it
syncs: a backup to the cloud is an explicit action, a file deleted here keeps
its backup, and a backup deleted there keeps the file here. `files add`
encrypts files and folders in one pass through a 1 MiB buffer (up to
`files_max_bytes`, 4 GiB by default), sniffs the type from the bytes, keeps
a thumbnail of an image, and resumes an interrupted import of the same file
on the same bytes; a folder comes in as a folder of the same name. A file of
the same name in the target folder takes the import as a new version, and a
folder of the same name the folder's files, unless `--keep-both` stands the
new one beside it as `name (2)`. `files/import_cancel`, which needs no lock
on the client, stops an import under way at its next chunk and drops its
unfinished file; what it added before stays. `files/search` also keeps the
files under a `folder`, the `starred` ones or those with a `tag`, and
`files/tags` counts the files of each tag. `files get` writes only a
fully verified version to a path that does not exist yet; `files open`
exports a copy to `open/` for another program and removes it after 30
minutes, on lock and on exit, asking first (`files/open_changed`) when the
program changed it, so the change can be kept as a new version. `files rm`
moves to the trash, emptied after `files_trash_days`; `files purge` empties
it now. `files verify --all` checks every chunk file against its name,
which needs no key, beside the library, so other requests go on meanwhile,
then every version while unlocked; the daemon does the same once a day
after a minute without anyone at the device, and `files usage` shows the
last check (`last_check`) with what failed. `files share` sends a version
to a channel as a new edition under its own key: it shares no key and no
chunk with the library's copy or a backup. `files save` keeps a shared file
from a channel message as a file of the library, re-encrypted under a key of
its own.

File imports, exports, previews and verification release the client between
chunks, so other local requests and locking can proceed during large files.
The desktop renders local listings and cached cloud inventories before
waiting for quota and remote refreshes; a late response cannot replace a
different folder or account's listing.

`files local-only` tells what only this device holds: local versions without a
cloud backup, changes waiting to sync and when a sync last succeeded; the
desktop shows it before revoking a device or binding the workspace to its
hardware, and when syncs have failed for a week.

Backups. `files backup` queues the requested versions durably and returns;
`files backup-status` reports their progress, and the `files/backup_run` RPC
runs an explicit upload pass. The daemon wakes to run queued jobs in the background.
A successful queue response does not mean the upload has finished.
The backup explicitly stores the library version itself in the cloud: it
uploads the current version (or every version) of the files named, folders
recursively, as `kind=backup` objects whose chunks are the version's own
and whose manifest, sealed under the current content root, holds the file's
key, path, tags and note; the object and version IDs on the relay are
random. The jobs continue in the background, locked too with
`background_online`, in slices of about five seconds with other requests
going first in between, and `backup-status` shows them; a failed attempt is
tried again five minutes later. Nothing starts a backup but this command:
not an import, not an edit, not a cloud refresh, and a copy deleted from
the cloud is never uploaded again. `files cloud ls --refresh` reads the
account's backup inventory from the relay (every device's, each entry's
signed plan checked against the certified evidence the relay kept when it
was reserved, so backups of devices replaced since still count, each
manifest opened with the root of its generation; one whose root this
device lacks is listed as such and opens at a later refresh, once the root
is here), marks local versions the backups are of, and `cloud get`
fetches one into the library, where it was or into `--to`, after checking
every chunk and the whole file.
`cloud rm` deletes cloud copies for good, which frees their quota and
changes nothing on this device; the copy a version released with `files
evict` reads from is its last, so `cloud rm` refuses it until `cloud get`
brings the version back or the version is deleted here. `files rm` on this
device changes nothing in the cloud. `cloud verify` downloads and checks a
backup; `files evict` then frees a verified version's chunks here, so it
reads only after `cloud get`. `cloud usage` shows the relay's storage room;
a backup request that would not fit fails with `QUOTA_EXCEEDED` before
anything starts.

Links. `files link` shares a version with someone outside dMsg: a fresh
edition under its own key goes to the cloud as a `share` object (its
manifest sealed under that key, never a root) and the relay publishes the
link `<site>/s/<share_id>#<key>`, the site being the one of `login_url`.
The key rides in the fragment, which servers never see; with `--password`
the link carries no key and the key travels wrapped under an Argon2id key
of the password (19 MiB, two passes, salted with the share ID), so the
person needs both. A link lasts `--days` (seven by default, thirty at
most) and `--max-downloads` starts (unlimited by default); `files links`
lists this device's links with the counts the relay last reported, and
`--refresh` reads the relay, which also lists links other devices made,
with names and URLs when this device holds the account root for their encrypted management record, and otherwise by ID. `files link-revoke` accepts asynchronous cleanup;
an expired link's copy is queued for cleanup automatically. Space returns only after physical cleanup and quota settlement. The browser page at `/s/` decrypts
by the chunk rules of `content-v1.json`.

Mount. With `files_mount = true`, or the desktop's settings allowing it,
`files mount` serves the library as a
loopback WebDAV share at `/dav/<token>/<volume-name>` and mounts it through the
system (macOS NetFS, Windows `net use`, Linux `gio mount`), as the
disk `dMsg Files` (`dMsg 文件` in Chinese). The desktop passes the volume
name in its interface language; the CLI uses its own locale. The name is
fixed for that mount, so a language change takes effect after remounting.
Finder's separate server group uses the URL host (`127.0.0.1`); changing
the volume name does not rename that system-managed group.
The token is random for each mount and lives only in the
daemon's memory; on macOS the mount also carries a random Basic password,
which NetFS takes without asking the person. The daemon sends it over stdin
to a short-lived `dmsg mount-netfs` child; no helper app or credential file
is created. Reading decrypts one 1 MiB chunk at
a time, ranges included; a file written to the disk waits, owner-only, in
`mount/` until the whole of it has arrived, then becomes a new version of
the file of that name or a new file; a folder maps to a folder, a delete
goes to the trash, a copy re-encrypts under a new key, and an editor's safe
save (a temporary file moved over the original) becomes a new version of
the original. `.DS_Store`, `._*`, `Thumbs.db`, `desktop.ini` and the like
are accepted and dropped. A lock, an account switch, `files unmount` and
the daemon's exit unmount, and the share answers nothing without the
token. Mounting is off by default because of the systems' WebDAV clients:
macOS keeps a plaintext cache of opened files in its system cache directory
while mounted; Windows refuses files over 50,000,000 bytes
(`FileSizeLimitInBytes` of the WebClient service), so larger files go
through `files get`; Linux mounts through GVFS, which programs not built on
it reach through its FUSE path. `files mount --status` shows the mount
with the system's note. A mount in progress reports `mounting: true` and
`mounted: false`. On macOS, status reads and the background pass check the
system mount table: ejecting in Finder clears the share and notifies the
desktop. Mount and unmount requests also reconcile this state, so an already
ejected disk can be mounted again or unmounted successfully. Failed unmounts
of disks still mounted return an error and remain tracked for retry.
Reads and writes count as activity for idle locking; directory
metadata polling does not. Changes through the disk notify desktop views.

The mount tests cover the daemon's HTTP routes as well as the DAV filesystem.
On macOS, after `cargo build -p dmsg_cli`, run
`cargo test -p dmsg_cli native_mount_ -- --ignored --nocapture --test-threads=1`
for explicit system mount tests using temporary offline accounts. They need
permission to mount a volume and exercise reads, writes, safe saves,
unmounting on lock and reconciliation after an external eject.
Windows and Linux require separate native validation.

Approvals are decided by a person: `approvals approve|reject`, `connect
browsers add` and the prompt after `attest …` or `device …` need an interactive
terminal without `--json`; otherwise the request is only created and waits in
the approval center (and the desktop's approval window). A request is named
by its ID or a unique prefix of at least six characters. A web request shows
its origin as relayed by the browser through connect.dmsg.net, the browser's
label, the account and the payload. `approvals list --all` shows finished
requests too, for 30 days after they were made, with who decided them,
where and when. Each source other than this device's own clients (an agent,
a client, a web origin) has at most five requests waiting. A request the
daemon rates high-risk (a payment, an added or revoked device, a change to
a device's capabilities or to the account's logins, signing policy,
recovery delay or Agent principal) shows
as such, and on a Mac whose Secure Enclave keeps the fast-unlock key
approving it first asks for Touch ID or the login password; declined, the
request keeps waiting. Where the account registered this device's approval
key, that question goes through the key, which also signs the approval. `approvals check-file <id> <file>` compares a file
with the digest a request signs (a page's document, or a digest given in
hex) without changing what is approved; `approvals reconcile` settles
approved requests whose outcome is not known yet, which the daemon also does
on its own. An attestation whose outcome the home no longer keeps (a day
past its approval) ends as `lost`.

Settings read and prepare browser invitations, candidates, grants and revocations
without holding the client during relay requests. Candidate reads and grants
recheck the local invitation and its expiry after the wait; granting still
requires fresh FormalApprove authority and explicit human confirmation. Policy
edits reuse one account read for their previous policy and approval checks.
Profile publication reuses its final principal read; account mutations retain
the existing serialized account journal.

Approval lists read in short local steps and omit filesystem metadata checks;
the selected request's detail and execution still check its file. The desktop
uses `approval/list {ids_only:true}` for waiting-request navigation, without
decrypting every request. File preparation, comparison and bundle verification
run on blocking workers; approving a local file still hashes it again before
reading fresh account state and signing. Signature history opens each execution
once, and exporting a synced signature looks up that execution directly.

`attest readiness` says whether this device can sign now: a login, formal
approval on this device, a signing policy that is not frozen, and this
month's allowance as last read. Finished attestations are kept by the
account as encrypted content objects that sync like vault items, so `attest
history` lists those of the account's other devices too and `attest export`
writes their bundles; a device recovered after all were lost has them after
its first sync.

`attest verify` needs no daemon and reports each check on its own: the
statement against the file (`matches_file`, `mismatch`, `digest_only`,
`text` or `action`), the device's signature, the receipt's certificate under
the configured root from one of the registry's user homes, the receipt's
binding to the artifact, its key, account and request, and the device's
present status; a device revoked since does not undo a signature that
verifies. The command fails with `INTEGRITY_FAILED` after its report when a
check fails. Whoever receives a bundle can also verify it in a browser at
`https://dmsg.net/_/verify`, or with `verifyAttestationBundle` from
`dmsg-sdk/bundle`.

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

Channels: `create` makes the first epoch at once. `invite` seals the current
epoch's key to the invited account's devices, so after `accept` with the
JSON printed it reads that epoch from its start, what was written before it
joined included; `rekey` first when it should not, and `history` gives it
older epochs. An invitation is accepted only in the epoch it was issued
in, so one made while the channel waits for keys rotates it first when this
account may coordinate; otherwise it carries no key and lapses with that
rotation. Removing a member, a member
leaving, a revoked device or `rekey` leave the channel at
`rotation_required`; an unlocked daemon whose account may coordinate (the
owner, an admin or a member with `can_rotate`) rotates it on its own, and
`rotate` does it at once. A device a member adds needs no new epoch.
`uninvite` withdraws an open invitation. A billing transfer (`sponsor
propose`) pauses the channel's messages until it completes, and `sponsor
cancel` calls it off. `send` journals a message before it signs it, and a
retry with the same `--message-id` never sends it twice; `outbox` lists
what is still on its way and `outbox --discard` gives one up. `settings`
renames a channel for this account, mutes or hides it, `search` looks
through what this device read, and `receipts` shows how far members' devices
took and read the messages. A message that does not open stays in `read` as
unreadable instead of stopping the channel; a history grant has its epochs
read again on the next `read`. `read` prints what this device read; while
unlocked the daemon also reads the channels the account's hint channel names. Locked, the account's
hint channel says only that a channel has new messages or a letter arrived,
and the desktop notifies without saying from whom. A command whose reply was
lost is kept in `pending`, and running it again, or `pending --key`, settles it
under the same request ID. After `leave` the relay serves the channel no more
to this account; the device keeps the last state it verified.

The channel loop reads up to four independent channels concurrently and
prioritizes hints over periodic reads. A hint during a read queues another
pass; the follow-up for the relay's five-second hint window remains.
Inviting and rotating hold only the channel's control-operation turn, so
other channels and local reads remain responsive during network waits.
The desktop opens cached messages immediately and refreshes the selected
channel in the background. A sent message appears on its durable ACK,
while earlier incoming messages are merged into their sequence positions.

Inbox refreshes resume from the last verified page. Letters and the page
cursor commit together, including pages filtered down to no visible letters;
the end is reached only when the cursor stops advancing. The desktop reads
the sealed local cache with `inbox/list {refresh:false}` before refreshing,
and reads the cached contact policy with `inbox/policy {refresh:false}`.
Sending, marking, configuring rules, requesting payment approval and checking
or recovering escrow funds wait on the network without holding the client.
An acknowledged mark updates cached flags immediately. Retrying a send keeps
its original order and sealed payload; locking stops later local writes.

Plan status reuses one service lookup and reads certified entitlement,
contract terms and catalog concurrently. Quote preparation, checkout balance
checks and name preparation release the client during network waits, with the
unlocked session and payer rechecked before saving journals or approvals.
Name prices read their three independent inputs together; transfer monitoring
batches certified owner proofs and bounds acceptance queries to four at once.
`handle/operations {refresh:false}` reads only the local operation journals.

Payments go through the approval center like signatures: `inbox pay`,
`plan checkout|advance` and `handle claim|register` only create the request,
which a person approves at the terminal or in the desktop's approval window,
seeing the exact amount, ledger and payer. The payer is the login's Principal
unless `payer use <pem>` names an identity file; the dMsg account itself
never holds funds. A paid letter is `inbox send --pay` (a quote), then
`inbox pay`; a subscription is `plan quote`, then `plan checkout`. A web
application's checkout arrives as a web request and is quoted
(`approvals quote <id> <ledger|neuron>`) before it can be approved; the page
gets the order's progress. `payment revise` resends an escrow transfer the
ledger refused at the fee it asked for, never above the escrow's limit.
`inbox refund` claims, in one transfer, what a letter's escrow holds for the
wallet: the payment after the acceptance deadline, the unused fee reserve once
every payout succeeded, and deposits the escrow did not take.

Team wallets (`dmsg wallet`) are canisters of their own, created through
the hub for a one-time ICP activation within the plan's quota. `quote` is
an update because it stores the deposit and expiry it returns; a plan with
no wallet left quotes `remaining` 0 instead of failing, and `create` refuses
that quote. The desktop reads the stateless `wallet/quote_preview` for the
fast price, fee and quota view. Proposing, executing what members approved,
broadcasting and reporting run at once; `create`, `join`, `approve`,
`reject`, `veto`, `unbind`, and a transaction this device must cosign, wait in the
approval center. `refund <quote>` returns a
quote's deposit, less the ledger fee, to the identity that paid it once the
hub failed to create the wallet or the quote expired unused; the quote's ID
is in the create request's approval details. The daemon calls wallets
with a wallet key kept on this device (`wallet principal`), which `join`
binds to the account with a device approval, one binding per device;
deciding still needs a `WalletApprove` device every time. A new device
takes over a retired one's binding with `join --replaces <device>`, or
every other device's after a login recovery with `--exclusive`; `unbind`
removes the bindings of lost devices, and `bindings` shows which of this
account's bindings belong to revoked or unknown devices. Neither changes
the member's role or votes. `execute` reads chain parameters from the
`[wallet]` endpoints, checks the wallet's plan with its own decoder, and
broadcasts what the wallet signed; `settle` reports it final once the chain
says so. `cosigner-new` keeps the key in the Secure Enclave and fails where
there is none; only `--software` makes a software key, which a wallet
requiring hardware cosigners refuses.

`native-create` makes this device's root of native Taproot (BIP-340) keys,
sealed for a new Secure Enclave key, or with `--software` under the local key;
`native-protect` and `native-unprotect` add and remove such keys, one always
staying. `native-key` derives this device's key for a wallet and prints its
`AddCosigner` intent; it and every L1 signature ask for the person.
`nc-migrate` shows how a native address moves to this device's key (register
the key, add an address, the address book when the tier requires it, then a
sweep through L1 or, with `--l2`, the chain key after the delay) and with
`--start` proposes the next step.

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

Agent principal reads, publication/refresh and approval preparation use short
session-bound client steps. Controller registration reads the next generation
once; renewal and revocation read the principal and stored credential together.
Network waits leave local reads and locking available. Approved account changes
still use the account journal's existing serialized execution path.

Agents. `agent principal enable` and every `agent controller` change are
account changes an administrator device approves; the user home publishes
the principal document to the directory (`published: false` in `principal
show` means it has not yet, and `principal publish` retries). `controller
register` creates the key in the vault; without `--scope`/`--audience` its
ceiling is `*`. `agent grant` and `revoke` sign, on a device with
`FormalApprove` and after approval, with the newest current controller whose
ceiling covers the grant, or the scopes and relying parties of the credential
revoked or replaced (`--generation` names one, which must cover it); no such
controller is `FORBIDDEN` before anything is approved. `agent grant --replaces <id>`
replaces a credential under its ID for a new lifetime, with the same
subject and scopes and relying parties among its own (anything wider is
`INVALID_INPUT`). `delegations` shows where each signed event stands and
`credentials` what the delegation service holds. `principal publish`
also has the delegation service read the published document again.

`agent id create <label>` hosts an Agent ID; the same label on another device
of the account brings the same Agent ID. `agent id use` picks the one `dmsg ap`
runs as; `resident on` keeps its key in the system credential store so it
signs while the device is locked (unavailable with `secure_storage = "file"`).
The Agent Mail keys its mailboxes need are sealed on this device, never synced
and never in the connector state; a resident Agent ID keeps a copy beside its
key, and `resident off` folds the keys it made while locked back in, so it
needs the device unlocked then.
`agent id credential` prints, once, the credential an MCP client gives
`dmsg mcp`; issuing a new one disables the old, and `uncredential` withdraws
it without a new one. `pause on` makes the credential reach nothing, whatever
the grants, until `pause off`. `remove` stops hosting the Agent ID on this
device: its grants are revoked, its credential, resident copies, mail keys
and connector state go, and its key stays in the vault, so the same label
brings it back (unlocked only). `profile` shows the Agent Profile it
published at each profile service the hosts name, its signature checked.
`grants add` gives an agent
capabilities over this account: `account.read`, `vault.list`,
`vault.read:<tag>`, `vault.write:<tag>`, `secret.use:<policy>`,
`files.read:<folder>`, `files.write:<folder>` (a folder ID or `root`,
covering the folders under it), `channel.read:<channel>`,
`channel.publish:<channel>`,
`inbox.read`, `inbox.contact`, `attest.request`, `action.request`,
`agent.delegation.read`, `events.endpoint`, `wallet.read`,
`wallet.propose:<wallet>`, `wallet.execute:<wallet>` (`<name>:*` covers every
target); and at the allowed hosts, each `:<origin>` or `:*` for every
host: `agent.profile.read` and `agent.profile.write` (Agent Profiles, at a host
or a configured Profile service), `discourse.read`, `discourse.write`, `mail.read`,
`mail.write`, `knowledge.read` and `knowledge.write`, such as
`discourse.read:*` with `discourse.write:https://discourse.dmsg.net`. An
agent's call then reaches only the configured origins its grants name: the
connector refuses the others (`permission_denied`), a room or held draft it
knows from another host included, and lists only the rooms of those it
reaches; with none of them the call is `FORBIDDEN`. `mail_sync` needs only
`mail.read`, and it also delivers the letters already queued, so revoking
`mail.write` does not hold those back. Without a grant an agent reads only
public dMsg account profiles (`profile.read`). Standard tools without an
additional grant include identity, configured service discovery, bundled
schemas, delegation (a grant or revocation still needs controller authority)
and the local inbox. Agent Profile reads separately require
`agent.profile.read`; room-specific schemas require `discourse.read`. No
vault capability reaches an item tagged `dmsg-…` (the Agent ID and
controller keys), not even with `*`, and such a tag cannot be granted. A
resident Agent ID's live grants are copied beside its key, and while the
device is locked it is checked against the copy. A device keeps 64 grants;
a new one takes the room of those revoked or expired, which the audit
record still names. An agent may ask for capabilities
(`dmsg_capabilities_request`): an approval request the owner decides, which
grants them once approved; `dmsg_capabilities` shows the agent its live
grants and the vocabulary.

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
`dmsg ap agent-protocols://rooms` (or another resource URI) reads what the
connector holds instead, from local state, changing nothing.
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
claude mcp add --scope user dmsg --env DMSG_AGENT_TOKEN=dmsg-agent-… -- dmsg mcp
```

`--scope user` serves it in every project; Claude Code's default scope is the
folder the command runs in. The credential then sits in `~/.claude.json` (and
a typed command in the shell history): whoever reads it acts as this Agent ID
until `dmsg agent id uncredential` withdraws it. The process finds the daemon
at its first request and again when it stops answering: it starts while no
daemon runs (`tools/list` then answers from its own catalog and each call
with `DAEMON_STOPPED`), and a daemon restart, which moves it to another port,
does not end the session. Only a refused connection is retried, so a call is
never sent twice.

Tools: the 41 standard `agent_protocols_*` tools of the Agent Protocols
local connector (`agent-protocols` 0.12.6: identity, delegation, rooms, inbox
and drafts, profile, Agent Mail mailboxes and letters, Agent Knowledge
submissions and reads), as this Agent ID, against the configured hosts, each
Discourse, Mail and Knowledge tool behind its protocol's read or write
capability, Profile reads behind `agent.profile.read`, and
`agent_protocols_profile_update` behind `agent.profile.write`, at the origins
the grants name (the description names it; the resources of rooms, drafts and mail need the same read
capability as their tools); and
`dmsg_account_status`, `dmsg_vault_list`, `dmsg_vault_read` (the secret
field only as below), `dmsg_channel_list|read|publish`, `dmsg_inbox_list`,
`dmsg_profile_read`, `dmsg_attest_request` (an approval request with source
`agent:<did>`, which only a person decides) and `dmsg_approval_status` (only
the agent's own requests). Every standard tool publishes the SDK's full
`inputSchema` and `outputSchema`, including nested fields and required members.
A refused call returns `isError` with the protocol's `code`, such as `permission_denied`,
`room_head_mismatch` or `FORBIDDEN`; a Knowledge payload that does not
match its type is refused before it is signed, with every failing member at
its path (`invalid_event`, `data.errors`).

More `dmsg_*` tools, each behind its capability: `dmsg_vault_read`
always masks concealed/OTP fields; the removed `secret` parameter is rejected.
`dmsg_secret_policies`, `dmsg_secret_use_request`, `dmsg_secret_use_status` and
`dmsg_secret_use_cancel` request and inspect one
owner-approved GitHub issue creation under `secret.use:<policy>`;
`dmsg_vault_write` adds or edits an item that keeps a granted tag
(`vault.write:<tag>`). Its optional `base` must match the current revision;
a stale base returns `VERSION_CONFLICT`, without opening historical content.
The item is returned only with
`vault.read` on it; it can give a concealed or one-time-password field a new
value or remove it, but refuses a type that would show its value or a change
to an existing item's tags;
`dmsg_files_list` lists a library folder and `dmsg_files_search` finds files by name, tag or note (`files.read:<folder>`
for the folder or one above); `dmsg_files_read` returns up to 1 MiB of a
version from `offset`, base64url, opening only the chunks it lies in, each
authenticated with its place in the version as the mounted disk reads them
(`files.read:<folder>`); `dmsg_files_write` writes
up to 8 MiB as a new file or a new version (`files.write:<folder>`), and no
tool backs up, shares or deletes a file; `dmsg_inbox_list` lists letters without
their text and `dmsg_inbox_read` returns one, marking it read with
`mark_read` (`inbox.read`); `dmsg_inbox_contact` writes a free letter, and a
recipient who charges only returns a quote (`inbox.contact`);
`dmsg_action_request` asks the owner to sign an application action, the
canonical CBOR a web page passes to `signAction` (`action.request`); and
`dmsg_delegations_list` lists the principal's credentials
(`agent.delegation.read`). Without any grant, `dmsg_capabilities` shows the
agent its live grants and what may be granted, and
`dmsg_capabilities_request` asks the owner for more: an approval request,
never the decision, which waits an hour. 75 tools in all, team wallets and MCP Events included.

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
`refreshBefore` is near; generic bindings only get reminders, while
`mail-watch` manages its own renewals). `events/stream` pushes
`notifications/events/active|event|heartbeat|error|terminated` with a
heartbeat at most every 25 seconds and ends on `notifications/cancelled`.
The cursor is the local sequence and survives restarts; a null cursor reads
from the oldest delivery not yet acknowledged, and `dmsg_events_ack` lets the
device forget. Local retention (30 days or the newest 10,000 records) records
loss separately for each endpoint. A read behind a retained loss position
returns `truncated: true`; an empty truncated page advances its cursor past
that position, so the next read does not repeat the same gap. Explicit ack
and sequence numbers used by other endpoints do not count as retention loss.
A push stream reports loss with `notifications/events/active` and
`truncated: true` before its events. That notice carries the preceding cursor;
clients advance only after storing the events, or on the following heartbeat
when the truncated page is empty. The CLI's `events stream` reports loss on
stderr while preserving event JSON lines on stdout. Without `endpoint_id`,
`endpoint_ids` or `session` (the
endpoint's `context.session`), a reader gets the endpoints created in or
bound to its own MCP session. Tools for clients without the events
extension: `dmsg_events_endpoint_create|list|rotate|delete`,
`dmsg_events_bind`, `dmsg_events_next` (`wait_ms` up to 25 seconds) and
`dmsg_events_ack`. Event data comes from other services: treat it as
untrusted input. The owner's `dmsg events endpoint
create|list|rotate|delete`, `events bind|poll|stream|ack|status` give the
same shapes.

`dmsg_events_bind` and the owner's `mcp-events/bind` accept only
`endpoint_id` (required), `server`, `subscription_id`, `refresh_before` and
`session`. `server` is the upstream URL, not an MCP client's local server id.
`refresh_before` is a nonnegative integer in Unix milliseconds or an RFC 3339
string, including `Z`, `+00:00` and other numeric offsets. Invalid calendar
dates and pre-epoch instants are refused. Omitted/null `server`,
`subscription_id` and `refresh_before` clear those binding values; omitted/null
`session` leaves `context.session` unchanged. The tool field is `refresh_before`; the
upstream MCP response field is `refreshBefore`.

`dmsg_events_ack` and `mcp-events/ack` require `through`, a nonempty decimal
string from `0` through `9223372036854775807`, and accept the same endpoint
selectors as reads. Missing/null/empty positions, wrong types, unknown fields
(including `cursor` instead of `through`) and invalid selectors return
`INVALID_INPUT`. Binding requests also reject unknown fields and wrong types;
an invalid request never silently clears a binding or acknowledges zero.

### Connecting MCP clients

In the desktop app, open **Agents → Agent IDs → Connect an MCP client**.
The dialog generates examples with this installation's binary, home and
one-time `DMSG_AGENT_TOKEN`. Use the same device/home that hosts the Agent ID
and keep its daemon running. Grant the Agent ID the capabilities it needs;
connecting a client does not grant account access. Issuing another credential
invalidates the previous one.

**Codex / ChatGPT desktop.** Run the generated `codex mcp add` command, or
merge the TOML into `~/.codex/config.toml` (choose one). For example, after
replacing the paths and demo credential:

```sh
codex mcp add dmsg-nova --env DMSG_AGENT_TOKEN=dmsg-agent-REPLACE-ME -- /absolute/path/to/dmsg --home /absolute/path/to/home mcp
```

```toml
[mcp_servers.dmsg-nova]
command = "/absolute/path/to/dmsg"
args = ["--home", "/absolute/path/to/home", "mcp"]

[mcp_servers.dmsg-nova.env]
DMSG_AGENT_TOKEN = "dmsg-agent-REPLACE-ME"
```

Local desktop, CLI and IDE clients on the same Codex host share this
configuration. Restart the client and use `/mcp` to check it; the CLI also
provides `codex mcp list`. See the [OpenAI MCP guide](https://developers.openai.com/codex/mcp).

**Anda bot.** Merge the generated entry into Anda's `mcp.json`, retaining any
other server entries:

```json
{
  "mcpServers": {
    "dmsg-nova": {
      "command": "/absolute/path/to/dmsg",
      "args": ["--home", "/absolute/path/to/home", "mcp"],
      "env": { "DMSG_AGENT_TOKEN": "dmsg-agent-REPLACE-ME" },
      "events": { "webhook_ingress": true }
    }
  }
}
```

`events.webhook_ingress` is an **Anda client setting**, not a dMsg command
argument or a standard `mcpServers` field. It chooses the one MCP server Anda
uses to receive upstream events that are delivered only by webhook. Grant
this Agent ID `events.endpoint`, then create an event automation in Anda.
Anda creates the dMsg callback endpoint, subscribes the upstream server,
binds and renews the subscription, and consumes/acknowledges
`dmsg.relay.delivery` events to schedule the automation. It does not open a
public listener on the local machine. Only one server may have this flag.

The flag is optional for ordinary MCP tool calls and for dMsg's native
`dmsg.mail.received` events (which use push/poll and still require the relevant
Mail grants). Adding it alone does not create a subscription or automation,
grant capabilities, or wake a Codex / ChatGPT conversation. Keep it in the
Anda example only. The [Anda implementation](https://github.com/ldclabs/anda-bot/blob/8385483/anda_bot/src/engine/mcp/events/ingress.rs)
defines this routing behavior.

The examples store the credential in client configuration; pasted commands
also remain in shell history. Keep those private and withdraw a leaked
credential in dMsg.

## Daemon protocol

The daemon listens on `127.0.0.1` only.

- `GET /daemon/status`, `POST /app/v1` (JSON-RPC 2.0) and `GET /ws/app/v1`
  (the same methods over a WebSocket, plus `event` notifications
  `{seq, type, data}`: `session/changed`, `login/changed`, `vault/changed`,
  `files/changed`,
  `files/progress {path, done, total}`, `files/open_changed`,
  `files/mount_changed`, `sync/changed`, `sync/progress`, `profile/changed`,
  `approval/changed`, `connect/changed`, `connect/hint`, `channel/changed`,
  `channel/read`,
  `inbox/changed`, `legacy/changed`, `mcp-events/changed`,
  `transfer/changed` (the transfer's view), `resync`) need
  `Authorization: Bearer <owner token>`; any request with an `Origin` header
  is refused with 401. A request or WebSocket message may be up to 16 MiB.
  Errors carry `error.data.code` such as `AUTH_REQUIRED`, `LOCKED`,
  `VERSION_CONFLICT` or `EXECUTION_UNKNOWN`. Reads announce no event, since
  windows reload on them. Requests run side by side, and the client is held
  only for local work. Calls that wait on the relay, a user home or the IC
  take it for each step alone and wait without it: the channels' reads,
  sends, attachments, downloads, previews, names and receipts; the
  library's backups, cloud copies and share links; profiles, the inbox,
  MCP Events endpoints, team wallets' reads, the plan's status, catalog
  and assets, `account/show|security`, `connect/pairings|sync` and
  `attest/readiness|action|verify`; requests for account and device
  changes (`account/bind_approve|unbind|policy|recovery_delay`,
  `device/approve|capabilities|formal_approve|revoke|succession_*`); a high-risk approval's
  presence prompt, an approved attestation's reads, submission and receipt,
  and the reconciling of attestations whose outcome is unknown; and the
  content sync and flush passes, mailbox read, backup passes and MCP Events
  fetch. Requests are signed under the client and sent after it, so a slow
  network does not hold up the library's reads. Work on the same thing
  takes turns: one content sync or flush pass, one backup pass, one profile
  publish, one MCP Events fetch, one reconcile pass and one send of a given
  message ID at a time. Login binding, recovery, device pairing, channel
  controls, payments, names, other approved requests carrying themselves
  out, unlocking, login checks and the legacy migration still wait under it.
- Methods: `app/initialize {activity?}`, `app/activity`, `daemon/status`,
  `events/poll`,
  `session/status|login {origin, bind?}|login_result|accounts|switch|unlock {fast?, account?}|lock|fast_unlock`,
  `account/create|show|security|bind_request|bind_accept|bind_approve|unbind|policy|recovery_delay`,
  `recovery/status|request|complete|dispute`,
  `device/pair_request|approve|capabilities|formal_approve|revoke`,
  `device/moved_list|moved_set_aside|moved_discard {id}`,
  `device/moved_import {id, succession?}|moved_approve_successor {id}`,
  `device/succession_request {predecessor}|succession_review {token}|succession_approve {token, device}|successions`,
  `device/succession_status|succession_cancel {id}`, `device/succession_run {id, step}`,
  `device/succession_skip {id, step, skipped?}`,
  `transfer/receive_start {session?, categories?, succession?}`,
  `transfer/send_start {session, to?, address?, succession?}`, `transfer/status`, `transfer/cancel`
  (one transfer at a time; `transfer/changed` follows its progress),
  `vault/templates|create|edit|duplicate|delete|restore|tags|list|show|reveal|history|revert|conflicts|resolve|otp|ssh_keygen|health`,
  `vault/otp_scan {image|path}`, `vault/import_preview|import` (absolute paths),
  `files/list|tree|show|search|mkdir|rename|move|update|versions|version_use|version_delete|trash|restore|purge|verify|usage|local_only`,
  `files/import|export|open|open_list|open_cleanup` (absolute paths),
  `files/share_channel|save_shared`,
  `files/backup|backup_status|backup_cancel|backup_run`,
  `files/cloud_list|cloud_refresh|cloud_fetch|cloud_delete|cloud_verify|cloud_usage`, `files/evict`,
  `files/mount {name?}|unmount|mount_status`,
  `files/share_link|share_links|share_revoke`,
  `sync/run|status`,
  `profile/display|show|pending|publish|retry|resolve|avatar|avatar_image|avatar_legacy`,
  `cache/status|clear` (owner, unlocked account only),
  `approval/list|show|respond|cancel|quote`
  (`show`, `cancel` and `quote` take a unique ID prefix),
  `approval/check_file {id, path}`,
  `attest/request {statement, file? | library: {file, version}?}|action {action: base64url CBOR}|history|usage|export|verify|readiness {id?}`,
  `connect/invite|candidates|grant|pairings|revoke|sync|status`,
  `channel/list|show|create|invite|join|control|leave|rotate|rekey|send|send_file|sync|messages|download|history|reactivate|pending|resume`,
  `channel/outbox|discard|mark|draft|settings|search|hide|preview|names|receipts`,
  `channel/owner_propose|owner_accept|owner_commit|sponsor_propose|sponsor_accept|sponsor_commit|sponsor_cancel`,
  `inbox/policy|configure|send|order|outgoing|pay|escrow|refund|list|mark`,
  `payment/transfers|revise`, `payer/use|balance`,
  `wallet/list|show|principal|quote|quote_preview|create|join|proposals|proposal|propose|member_agents|send|approve|reject|veto|cancel|freeze`,
  `wallet/execute|broadcast|settle|report|reconcile|balances|audit|refuel|cosigners|cosigner_new|policy_default|intent`,
  `wallet/native_status|native_create|native_protector_add|native_protector_remove|native_key`,
  `wallet/nc_migrate_plan|nc_migrate_status|nc_migrate_start`,
  `wallet/activation|activation_continue|assets|book|readiness|preview|transfer|change|receive|versions|health|upgrade|lock_code`,
  `wallet/rpc|rpc_set|rpc_check|audit_page|cosign|cosign_status|cosign_import|cosign_execute|channel|notify`
  (structured desktop forms and explicit cosignature collection; see [wallet desktop](../../docs/dMsg_wallet.md)),
  `plan/catalog|status|assets|quote|checkout|advance|order|checkouts|cancel|reconcile|refund`,
  `handle/resolve|price|claim|register|transfer|accept|transfer_commit|operations|reconcile`,
  `ops/orders|transfers|claims|reconcile_order|process_transfer|revise_transfer|claim|sweep`,
  `approval/reconcile`,
  `legacy/pair|pairs|cancel|import|list|channels|report|delete_channel|file|frozen` (absolute paths),
  `agent/principal|enable|register|retire|compromise|rename|publish|refresh`,
  `agent/grant|revoke|credentials|delegations`,
  `agent/ids|create|use|resident|credential|credential_revoke|pause|remove|profile`,
  `grants/list|add|revoke`,
  `audit/list {agent?, before?, limit?}`, `ap/call {agent?, session?, tool, input}`,
  `ap/read {agent?, session?, uri}` (a resource from local state, changing nothing),
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
  `{"result": …}` or `{"error": {code, message, data?}}`; a paused Agent ID's
  credential reaches none of them (`FORBIDDEN`). Events of agent changes:
  `agent/changed` for the owner's; a tool call announces only an entry in
  the audit record (`audit/changed`), so a polling agent does not have the
  windows read the principal again.
- `approval/respond` and `connect/grant` require `human: true`, which only
  the desktop approval window and an interactive terminal send; the daemon
  cannot tell them from another holder of the owner credential, so the owner
  credential is the boundary. Agents present their own credential, which
  never reaches them.
- `GET /auth/callback` and `POST /auth/callback` exist only while a login is
  pending (ten minutes) and accept one POST from the daemon's own loopback
  origin ([dmsg-login/1](../../docs/protocol/login-v1_zh.md) §4). A login
  announces `session/changed {state}` and `approval/changed`, a binding login
  `approval/changed`. `login/changed` comes when the POST arrives and when its
  check ends: `session/login_result` reports `{pending, checking, done,
  error {code, message}, approval, status}`, pending until the login page's
  time runs out or the check ends, and done once the check ended; `status`
  is null while pending. The check finishes even if the browser leaves
  before its answer.
- `idle_lock_minutes` without activity locks the daemon: the login and every
  key are dropped. Every owner request is activity except `app/initialize`,
  `daemon/status`, `events/poll` and `mcp-events/poll`; an agent's requests
  never are, so an agent that polls does not keep the device unlocked (a
  resident Agent ID works while locked). So are a completed login and, from any client, a successful
  `session/unlock`: a window that asks for Touch ID as it comes to the front
  has no input to report. A WebSocket that sent
  `app/initialize {activity: "reported"}`, as the desktop does, reports a
  person's activity with `app/activity`, and its other requests are not
  activity: its windows read on events. While unlocked, `daemon/status`
  gives the time it will lock as `lock_at` (Unix milliseconds) and the idle
  period as `idle_lock_ms`; the lock comes within 10 seconds after `lock_at`.
  Revisions already signed for submission are still sent, without any key,
  until their 45-second deadline. With `background_online` the device
  signing key stays: `dmsg sync` and the scheduled sync still send sealed
  versions and fetch ciphertext, while reading, editing and every account
  change answer `LOCKED`.
- The daemon keeps the relay's hint channel open, locked or not, with a token
  it registers while the device key is available. A `connect` hint fetches the
  mailbox (stored sealed while locked); without the device key the daemon only
  emits `connect/hint` for the desktop to notify. The mailbox is also polled
  every 30 seconds; that pass reconciles approved requests whose outcome is
  unknown and announces `approval/changed` only when there were some or the
  number waiting changed.
- While unlocked, the daemon keeps one WebSocket per active channel; an
  `available` hint, a socket opened with none before it and a closed one each
  trigger a signed read. A socket's ticket lasts about a minute, so it is
  renewed every 40 seconds: the new socket opens first and closes the one it
  replaces, which needs no read.

### Agent Profile and Mail through MCP

The 0.12.3 connector adds verified Profile reads and history, service/card
resolution, payload schemas, room archives and custom-event watches.
`agent_protocols_services_list` discovers configured services without changing
configuration. `agent_protocols_schema_get` needs no grant for bundled protocol
schemas; with `room_id`, it requires `discourse.read` for that room's host.
`room_archive` and `room_watch` also require `discourse.read`.

Grant Profile reads independently of writes, for example:

```sh
dmsg grants add helper agent.profile.read:https://api.dmsg.net agent.profile.write:https://api.dmsg.net
dmsg ap --agent helper profiles_get '{"profile_service":"https://api.dmsg.net"}'
```

An MCP agent with these grants can read, edit and republish its full profile.
Omitted optional fields are cleared; preserve desired endpoints, links and
delegation hints, and omit `updated_at` and `event_id`. Local grants do not
replace the remote service's admission checks. The owner's `dmsg ap` commands
use the configured allowlist without needing the agent's capability grants.

```sh
dmsg grants add helper mail.read:https://mail.dmsg.net mail.write:https://mail.dmsg.net
dmsg ap --agent helper mail_mailbox_publish '{"routes":["https://mail.dmsg.net"]}'
```

The returned `mailbox_id` and `address` identify the new mailbox. Include
`reply_mailbox_id` when inviting replies; `mail_sync` receives new mail before
`mail_list` or `inbox_next` reads it. `mail_resolve` requires `mail.read` at its
card routes; if using `profile_service`, grant `mail.read` for that configured
Profile origin too. A grant on a relay never includes another origin implicitly.

The daemon drives that explicit connector sync every 30 seconds for hosted
Agent IDs with a live `mail.read` grant. Paused agents are skipped; while the
device is locked, only resident agents with retained grants and keys receive.
Calls of different agents run independently, without holding the client across
the network. New letters remain unread and enter the connector's pending inbox;
subscribers to its Mail resources receive MCP resource-change notifications.
The desktop receives a count-only `agent/mail` event. A Mail `delivered` result
means the relay accepted the encrypted packet, not that the recipient fetched
or read it. These notifications do not by themselves start an LLM turn: the MCP
host must subscribe and decide how to handle a change. The separate MCP Events
transport continues to carry `dmsg.relay.delivery` occurrences.

## Cloud-copy management

The desktop Files view keeps separate Links and Cloud backups pages. Account
storage usage is shown in Membership through `files/cloud_usage`. The generic Cloud
storage interface and its file-detail entry points were removed on 2026-10-28.

**Deprecated (2026-10-28):** `files/storage` (encrypted local inventory snapshot),
`files/storage_refresh` (inventory/accounting refresh), and `files/storage_action`
(generic bulk actions). They remain callable for explicit clients, including the
existing display modes on `files/storage`, but new clients should use content-specific
APIs. Use `files/share_links` with `{refresh:true}` and `files/share_revoke` for links,
`files/cloud_*` for backups, and `files/cloud_usage` for account usage.

The deprecated `files/storage_action` still accepts at most 50 targets, each
`{owner_kind,resource,upload_id,generation,action,share_id?}`, with `delete`, `cancel`,
`retry` or `recover` and per-target outcomes. `files/cloud_delete` returns `accepted`,
not a claim that physical deletion or quota release already completed.
`files/share_quote {id,version?,channel?}` remains supported for payer/size estimates
and existing copies. Names, source versions and publication recovery details remain
account-root encrypted; deleting a local source does not remove link management.
Low-level metadata synchronization, attachment deletion evidence and quota accounting
remain supported. Sponsorship alone never grants content/deletion permission.

## Real-time Agent Mail

`dmsg agent mail-watch <agent> on` enables relay notifications; `off` stops them,
and omitting the state shows subscriptions and safe diagnostic status. The Agent
must already have `mail.read:<relay>` and `events.endpoint`. The desktop Mail
switch uses the same lifecycle. Maintenance checks every ten minutes and renews
within sixty minutes of `refresh_before`, requesting seven days bounded by the
Mail/endpoint grants and mailbox expiry. Resident, unpaused and authorized Agents
also renew while locked using a dedicated subscription key in the credential
store. Creating or deleting account callback endpoints waits for unlock.
Periodic Mail sync remains the fallback.

`dmsg mcp` also serves `dmsg.mail.received` by native `events/poll` and
`events/stream`. Its durable, metadata-only log commits with the connector
snapshot and does not consume unread mail or inbox claims. The event cursor is
independent of `dmsg.relay.delivery` and scoped to the Agent and mailbox filter.
Read new mail with `mark_read=true`, then acknowledge completed inbox handling;
preview/history readers leave read state unchanged. The full contract is in
[Mail events](../../docs/protocol/mail_events.md).

## Credential use (P0)

`dmsg secrets ref <item> <field>` prints a reference, never the value.
`dmsg secrets policies save <json-file>` requests approval of a local policy
and optional `grant_to`; `list`, `disable` and `delete <id> <base-version>`
manage it. `dmsg secrets uses list/show/cancel/reviewed` inspects uses.
See [the exact contract](../../docs/protocol/secrets_zh.md) for input shapes.

Only the fixed GitHub.com issue-create adapter is implemented. No wildcard
`secret.use`, remote execution service or credential resolve is provided. P1 owner workflows are described below. Each use needs desktop approval or an
interactive terminal with a successful system presence check. Policy changes
also require approval; disabling is immediate. A `lost` outcome may already
have created the issue and is never replayed. Tests do not use real GitHub
credentials; see [validation](../../docs/protocol/secrets_zh.md#验证入口与边界).


## Secrets P1 开发工作流

实现进程单次交付、SSH/Git 逐次审批、机密文件挂载、秘密快照分享/导入、受限扫描与轮换记录。公开数据流、owner 接口与限制见 [Secrets 合同](../../docs/protocol/secrets_zh.md)，平台和实际测试边界见 [验证范围](../../docs/protocol/secrets_zh.md#验证入口与边界)。这些 owner 功能不为 MCP 增加明文取密或任意目录访问接口；团队共享保险库仍暂缓。

CLI 入口：`dmsg run --help`、`dmsg secrets mount --help`、`dmsg secrets ssh --help`、`dmsg secrets shares --help`、`dmsg secrets packages --help`、`dmsg secrets discovery --help`、`dmsg secrets rotation --help`。密码和分享链接通过 stdin 输入，避免写进 shell 历史；普通 JSON 输出不提供已发布链接的密钥，需显式 `secrets shares link`。

## Agent Protocols 0.12.6 recovery

To recover retained mailbox records at an allowed relay, call
`agent_protocols_mail_mailboxes_list` with `host`, optionally `limit` and `cursor`,
through `dmsg ap` or MCP. Follow `next_cursor` until it is absent. This path needs
the normal `mail.read:<origin>` grant for an Agent; local inspection without
`host` remains nonblocking. Recovery persists every verified page and does not
restore missing decryption keys or historical plaintext. Inspect `key_available`
and `pending_routes`; restore existing local keys or rotate a key before receiving
new correspondence. Push subscriptions are not created for keyless records.

Mailbox publication persists encrypted key checkpoints before any signed attempt.
Restart uses the last durable checkpoint before later tool calls; successful
completion transfers the keys to their regular sealed/resident store before
clearing it. This also covers lost replies and partial publication across relays.
The owner Profile display keeps durable verified pins and does not follow
cross-origin discovery endpoints. See the [connector host](../dmsg_agent/README.md)
for persistence and verification tests.

## Display snapshots and offline reads

`profile/display` and `channel/names {accounts}` return local, LDK-sealed data
without waiting for an upstream request. Both require an unlocked account.
The former returns `{state,name,avatar,updated_at,observed_at_ms,stale,refreshing}`;
`avatar` is a validated image data URL. The latter accepts at most 100 account
IDs and returns `{account: {name,stale}}`; an unobserved account is omitted,
whereas a verified missing name is `null`. They queue bounded background work;
`profile/changed` and `names/changed` contain no names, IDs or image URLs.
`profile/show` and `profile/resolve` remain live verified document reads.

The following owner methods additionally accept `mode: "local" | "swr" | "fresh"`:
`plan/status|catalog|assets|panda`, `handle/mine`, `account/show|security`,
`recovery/status`, `wallet/show|balances|history|queue`, `inbox/policy`,
`files/storage|cloud_usage`. Parameters such as `wallet` remain required.
These display reads return `{state,data,observed_at_ms,stale,refreshing,error}`.
`local` never starts network work; `swr` returns local data and queues expired
observations; `fresh` waits for a read. `plan/status` with a mode rejects
`refresh:true`: the mode never invokes `refresh_entitlement` or relay refresh.
A wallet history snapshot is committed only after all pages and its final
certified head agree. A direct live history read exceeding the display budget
still returns the fetched rows with `persisted:false`.

`cache/changed {kind}` tells visible views to reread locally. Refreshes coalesce
per resource, use at most four snapshot workers, and back off independently for
60 seconds after failure. The profile and name workers each run one task;
profile/name freshness is 10 minutes (missing names 1 minute), avatar freshness
24 hours. A failed verification is never a missing-name observation. A newer
publication cannot be overwritten by an older document. An old image cannot
stand for a changed avatar URL.

`cache/status` reports encrypted record sizes by category; these are not SQLite
file sizes or a promise that clearing shrinks WAL/files. `cache/clear` removes
only reconstructible display records. Content, unsent work, notes, recovery
material, audit checkpoints and protocol pins are excluded. Pending refreshes
are fenced by the unlock and clear generations, and a snapshot also by an
invalidation of its own kind; another kind's invalidation does not fence it.
None of these methods is available to an Agent credential.

`daemon/status.session` is an opaque string combining the daemon instance and
unlock generation. The desktop masks disconnected views, retains edits only
after the same session is confirmed, and uses `DAEMON_DISCONNECTED` separately
from upstream `NETWORK_UNAVAILABLE`. Neither local reads nor background cache
workers renew activity. Existing serialized account mutations, connect-entry
processing and non-wallet approval reconciliation can still delay acquiring the
client lock; profile timing measurements must report that limitation separately.

Cache producers outside the background workers capture the same account,
unlock/invalidation generation and source before reading. A clear, policy edit
or endpoint change prevents an earlier response from writing back. Locks caused
by background revocation checks or a failed unlock also announce the session
change; owner views do not wait for another successful status query to clear.
