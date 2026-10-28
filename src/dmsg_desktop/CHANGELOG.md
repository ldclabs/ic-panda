# dMsg client changelog

The `dmsg` CLI and daemon ([`src/dmsg_cli`](../dmsg_cli/README.md)) and this
desktop app share one version and one release, tagged `dmsg-v<version>`.
Canister and website changes are released separately; they appear here only
where the client depends on them.

## [1.1.3] - 2026-10-28

### Desktop

- The tray icon adds a padlock while the account is locked. The approval
  badge stays visible beside it, and connecting or setup states show no lock.

### Files

- The Cloud storage page is removed. Membership still shows storage usage,
  and Files keeps the Links and Cloud backups pages.
- Links searches by name or ID, copies active links and revokes them after a
  confirmation. A failed revocation keeps its error for a retry; replies that
  arrive after locking or leaving the page are dropped.
- Links reads the relay when it opens or on Refresh. Library changes,
  background backups included, only re-read the saved list.
- File details no longer list cloud copies, and sharing only notes how many
  copies of the version already exist.

### Account setup and uploads

- Upload deadlines stay five minutes below the relay's 24-hour limit, so a
  device clock slightly ahead of the relay no longer fails account setup,
  sync, backups, links or avatars with `RESULT_EXPIRED`.
- When the initial root upload expires, setup checks the old upload first,
  reuses it if it was committed, and otherwise retries once under a new
  upload ID with the same root, bundle and reservation. Setup plans saved by
  earlier versions resume the same way.

### Daemon

- `files/storage`, `files/storage_refresh` and `files/storage_action` are
  deprecated. They still answer explicit callers; the desktop no longer
  uses them.

## [1.1.2] - 2026-10-27

### Desktop

- New windows open Channels by default.
- Settings show the current configuration before opening an editor for a
  specific task. Profile, device, account security, file, browser connection
  and migration pages explain the next action and keep drafts after failures
  or background refreshes.
- Unchanged or invalid settings cannot be submitted. Signing limits and
  purposes share one approval; pending changes show their request status
  until they finish, and cancelling a system dialog no longer reports success.
- Opening a local file cleans up an unfinished temporary copy if the operation
  is interrupted.

### Team wallets

- Code management distinguishes the installed version from an accepted
  version waiting for installation, without displaying internal debug text.
  An already accepted version no longer offers a duplicate upgrade proposal.
- Installation progress, failures and completion appear beside the install
  button. Pending installation prevents duplicate clicks; a failed attempt
  offers a state refresh and can be retried.
- A completed installation updates the displayed version immediately. Older
  reads cannot overwrite that result, and leaving the page discards late
  feedback.

### Membership

- App Store terms are identified as managed by Apple. While such a base term
  is active, plan cards do not offer cash or PANDA renewals, upgrades or
  switches, and do not show a separate renewal window.

### Upgrade notes

- Installing an already accepted upgrade from a `canisters-v3.1.0` wallet
  requires the wallet-hub fix from this source. Upgrade the hub first, then
  install the accepted wallet version; no new proposal or approval is needed.
- The service-side refusal of overlapping App Store and cash/PANDA base
  terms requires the corresponding `dmsg_commerce` update.

## [1.1.1] - 2026-10-26

### Credential use

- The credential pages open on one compact navigation row. Sources are chosen
  from searchable pickers that show names and IDs, never values; empty pages
  explain how to set up each workflow.
- Approvals show structured summaries, health findings link to the item or
  setting to fix, rotations list their steps in order, imports report what
  they created, and device activity filters by status, type and period with
  stable pages.
- Each workflow loads, fails and retries on its own. A snapshot or mount
  preview no longer applies once its inputs change, a failed import keeps the
  masked input, and results that arrive after leaving a page are dropped.
- A rotation offers "new credential created" only once the field's value
  actually changed; editing only the item's title no longer enables it. The
  daemon compares the values without returning them.
- Health reports and cloud storage's sharing management read file share links
  created by earlier versions again; they failed with an integrity error.

### Account security

- Fast unlock waits for Touch ID without holding the daemon, so other windows
  and commands keep answering while the prompt is open. Locking or switching
  accounts meanwhile cancels the unlock, and a failed unlock clears the
  previous account's views.
- When the account requires an approval-key signature for an action, a device
  without the registered key refuses before sending, instead of leaving the
  user home to reject it. Replacing a required approval key asks for Touch ID
  once, with the stricter setting of the old and new key.

### Desktop

- Choices, dates, sliders, progress bars and confirmations use the app's
  themed controls everywhere, with persistent labels. Settings fields share one
  layout, failed edits keep their drafts, invalid date ranges are refused, and
  dialogs and popovers stay inside narrow windows.
- File and Vault item details keep IDs at the end, with shorter version chips;
  tooltips and copies still give full values. File places open in a dialog and
  toolbar actions wrap, so details stay readable in narrow windows.
- When this Mac joins an account as a new device, onboarding also shows the
  pairing request as a QR code.
- Saving a channel attachment to Files checks the Files size limit before
  downloading and re-encrypts it chunk by chunk, instead of holding the whole
  file in memory.

## [1.1.0] - 2026-10-25

### Credential use

- Signatures & approvals → Credential use lets programs, Git and agents use
  a Vault credential without receiving the Vault. Every use is an approval
  request; approvals and history never hold values. Fields are referenced as
  `dmsg-secret://<account>/<item>/<field>`, Files versions as
  `dmsg-file://<file>/<version>`.
- `dmsg run` delivers referenced fields and file versions to one approved run
  of a program, as environment variables (`--env-file`, references only) or as
  inherited sequential pipes (`--file`, up to 8 files of 8 MiB each). The plan
  binds the program, its arguments, working directory and sources; a change to
  any of them, to the credential or to the session before the start voids it.
- SSH and Git: an Ed25519 key generated in the Vault signs through
  `dmsg secrets ssh socket` with one approval per signature, for commit
  signing (`git_sign`) and SSH login (`ssh_auth`, session-bind only, no agent
  forwarding). The private key never leaves dMsg.
- Secret files: a project mapping mounts `.env` content and Files versions as
  ordinary paths whose reads wait for approval (on macOS a window of at most
  60 seconds per file and user, on Linux per open handle, over FUSE). On macOS
  it needs the "dMsg secret files" extension; the Secret files page explains
  how to turn it on in System Settings, and each start failure names its
  cause. A mount never falls back to a plaintext file.
- Agents: a device-local policy lets an agent ask to create a GitHub issue with
  a concealed token (MCP `dmsg_secret_use_request`, `dmsg_secret_use_status`,
  `dmsg_secret_use_cancel`). The approval shows the repository, title and body;
  the agent gets the issue number and URL, never the token. An uncertain
  result is `lost` and is never sent again.
- Secret snapshots copy chosen fields, notes and attachments of one item to a
  link (one day and one download session by default; 6 minutes to 7 days, 1 to
  100 sessions) or to a channel. Nothing is selected by default, recipients
  import verified copies as new items, and copies cannot be recalled.
- Health and discovery reviews findings, scans a chosen directory for
  `.env`-style credentials to import, lists the policies, grants, mounts, SSH
  bindings and snapshots that use a field, and tracks a rotation through
  upstream revocation, checking that list again before it completes.
- File details and Vault items show complete, copyable IDs. Files copy
  version references; fields get value-free references from the daemon.
  Walkthrough (Chinese): [credentials, file references and developer
  workflows](../../docs/dmsg_secrets_tutorial_zh.md).

### Account security

- Approval keys: a Mac can register a Secure Enclave approval key, and the
  account can require its signature for account, device and Agent principal
  control, optionally also payments and attestations. The user home verifies
  it, so a stolen software device key and login are no longer enough. A login
  recovery retires every key and clears the policy.
- Device-bound unlock makes login unlock also need this Mac's Secure Enclave
  secret, as fast unlock does; Touch ID only removes the password fallback. A
  fingerprint state that cannot be read counts as unknown, not changed, so a
  temporarily unavailable Touch ID keeps the key.
- Pairing a device and changing its capabilities are high-risk approval
  requests, like revoking one; the approval shows the device, role and
  capabilities.
- Pending recovery alerts: the daemon checks the selected account's certified
  recovery status about once a minute, also while locked. The desktop notifies
  once per request, without account details, and opens Account security to
  cancel it; `dmsg status` warns as well. Alerts need the daemon and desktop
  running, the device online and notifications allowed.

### Desktop

- Profiles, names, the account, membership, wallets, Inbox policy and cloud
  storage open from encrypted, account-local display snapshots and refresh in
  the background. Settings → Files → Display cache shows and clears them;
  locking or switching accounts takes them off screen.
- Legacy migration waits until the account is ready and says which setup step
  remains. An interrupted initial account setup resumes on the same account.
- Team wallets with hundreds of proposals load their history again: the
  history cache is stored in pages.
- The Chinese interface calls the vault 保险库.

### Upgrade notes

- Approval keys add fields to the user home's account (`dmsg_user` schema 16,
  canisters 3.1.3). This client cannot read the account from an older user
  home, so upgrade the user homes first.
- Opening a secret snapshot link in a browser needs the website's share page
  from this source.
- On macOS, secret files need a dMsg.app whose daemon and extension are signed
  by the same team, as packaging checks. The extension stays off until turned
  on in System Settings.

## [1.0.2] - 2026-10-24

### Team wallets

- The wallet is organized in four sections: Overview, Assets and addresses,
  Transactions and proposals, and Team and security. History and audit open
  from transactions; runtime and code management from team and security.
  Transfers open in a dialog that keeps the draft.
- Policies take durations in days, hours and minutes (decimals included) and
  amounts at each asset's precision. Reviews show the network, the settlement
  and the highest total spend before anything is proposed.
- Policy usage per asset (approved but unexecuted reservations, amounts
  executed in the last 24 hours), a complete pending queue that does not
  depend on the first page of history, history filtered by number, asset,
  recipient, note and date with CSV export (spreadsheet formulas escaped),
  and bookkeeping notes kept encrypted on this device, never shared, approved
  or sent as a memo. Daemon methods `wallet/queue`, `wallet/history`,
  `wallet/policy_usage` and `wallet/notes`.
- Cosigner setup in four steps (generate, register, bind, test). The key
  self-test (`wallet/cosigner_test`) signs and verifies a random local
  challenge and builds no transaction.
- Approval and cosigning read the certified proposal and judge expiry and
  delay against the certified wallet version and the current time. Approval
  tickets show labels, canister methods and other proposer text exactly as
  proposed; only booleans and enumerations are translated.
- Payment recovery: an ICP/ICRC transfer with an unknown result is reconciled
  against the ledger block you name (`wallet reconcile-transfer`), never paid
  again. An interrupted activation whose code was not installed can be
  aborted and refunded (`wallet abort-activation`). Surplus and late
  activation deposits stay listed and claimable. A pending EVM transfer offers
  cancel or release bound to that proposal and nonce, and a Solana address can
  get a durable nonce account.
- Native EVM transfers estimate gas from the final call, so a contract
  recipient is no longer treated as a 21,000-gas transfer. An independently
  decoded EVM cancellation must match the nonce of the approved intent.
- Fixes: Bitcoin transfers take at most 16 recipients, as the wallet does; a
  memo hidden after changing the number of sources or recipients is no longer
  sent; Transfer loads token metadata and the address book from every tab; the
  wallet header keeps its name readable in narrow windows.
- Creating a wallet from the desktop stays paused. Existing wallets, joining,
  continuing activations and refunds work as before.

### Agents

- Agent Protocols 0.12.6. Publishing a mailbox first commits its encrypted
  keys to a checkpoint, and every connector call restores from the last
  checkpoint, so a restart or a lost reply no longer leaves a mailbox whose
  keys were never saved.
- Mailbox recovery: Agents → Mail → Recover mailbox records reads retained
  records from the relays you enter, page by page (from `dmsg ap` or MCP,
  `agent_protocols_mail_mailboxes_list` with `host`). It shows missing keys
  and pending routes. It restores records, not keys or message text; a
  mailbox without its key can be closed or have its key rotated but not
  renewed, and background push skips it.
- The owner's view of public Agent Profiles keeps verified pins across
  restarts and services, refusing rollbacks, conflicting envelopes with the
  same nonce and cross-origin discovery.
- A controller's definitive `409 nonce_not_greater`, as when another device
  used a higher nonce, allows one journaled re-sign under the same approval
  instead of failing it.
- An agent with `vault.write` can no longer reveal a concealed or one-time
  password field by changing its type. It can still set a new value or remove
  the field; any other type is refused with `FORBIDDEN`.

### Network recovery

- IC queries, status reads and relay GETs share one bounded retry policy
  (three attempts, 5 seconds each, 12 seconds in all). Chunk reads and file
  downloads allow 30 seconds per attempt and 60 in all, so slow links can
  finish a chunk. Writes are sent once: a lost reply stays
  `EXECUTION_UNKNOWN` for the journals to reconcile and is never replayed.
- During an outage the desktop keeps loaded content, shows one
  reconnecting or offline notice and retries reviewed reads in the background
  after 5, 10, 20, 40 and 60 seconds, starting afresh for each outage. Locking,
  switching accounts or losing the daemon drops the queue. Identical
  concurrent calls share one request; reads time out after 45 seconds and
  writes after 180.
- A write that could not reach the network reports `UNAVAILABLE` and asks to
  be tried again. Notices use localized text, never raw transport messages,
  URLs or tokens; integrity and permission failures stay errors.
- An unlocked account retries a failed sync (`NETWORK_UNAVAILABLE`,
  `UNAVAILABLE`, `EXECUTION_UNKNOWN`) after 60 seconds, even with periodic
  sync off.
- `dmsg logs` records the failing subsystem, an allowlisted cause, the HTTP
  status and the failed sync stage, never parameters, IDs, URLs or messages.
  On Unix the log directory is owner-only and new log files are mode 0600.

### Desktop

- Simpler surfaces: one style of icon tools and panel closes, no decorative
  frames or static shadows, with boundaries kept for inputs, focus, risk and
  floating layers. Agent maintenance actions and read failures are gathered
  in one place, and membership summaries are compact.
- Vault list previews show a user name, a web address's host or the item
  type, never notes, hidden fields or credentials in URLs.
- Long mail addresses no longer overflow.
- macOS: a Secure Enclave failure keeps its native `OSStatus` code for
  diagnosis.

### Upgrade notes

- Policy usage, transfer reconciliation and activation abort call methods
  added in the `dmsg_wallet` and `dmsg_wallet_hub` canisters of this source,
  together with their safety fixes (membership changes invalidate unexecuted
  proposals, spending reservations last until a definite outcome, a Solana
  retry cannot pay twice, upgrades and code locks are serialized). Upgrade
  wallet instances through the hub first, then the hub. Until then the
  desktop shows policy usage as unavailable and the other two are refused.
- Cancel or release proposals created before that upgrade, which do not name
  their target proposal, must be created again.

## [1.0.1] - 2026-10-23

- Connect an MCP client adds Codex (`codex mcp add`, or TOML for
  `~/.codex/config.toml`, shared by the Codex CLI and the ChatGPT desktop) and
  Anda bot (`mcp.json`) to Claude Code and generic `mcpServers` JSON. It
  explains Anda's optional `events.webhook_ingress` flag and the
  `events.endpoint` grant its event automations need. See
  [Connecting MCP clients](../dmsg_cli/README.md#connecting-mcp-clients).
- An Agent ID without a published profile suggests asking the agent to
  publish one; the Chinese interface calls agent profiles 档案.
- Capability grants: suggestions are clickable and never added twice, `<…>`
  target placeholders are selected for editing, and a capability with an
  unfilled placeholder cannot be granted.

## [1.0.0] - 2026-10-23

The first release of the dMsg client.

[1.1.0]: https://github.com/ldclabs/ic-panda/compare/dmsg-v1.0.2...dmsg-v1.1.0
[1.0.2]: https://github.com/ldclabs/ic-panda/compare/dmsg-v1.0.1...dmsg-v1.0.2
[1.0.1]: https://github.com/ldclabs/ic-panda/compare/dmsg-v1.0.0...dmsg-v1.0.1
[1.0.0]: https://github.com/ldclabs/ic-panda/releases/tag/dmsg-v1.0.0
