# dMsg client changelog

The `dmsg` CLI and daemon ([`src/dmsg_cli`](../dmsg_cli/README.md)) and this
desktop app share one version and one release, tagged `dmsg-v<version>`.
Canister and website changes are released separately; they appear here only
where the client depends on them.

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

[1.0.2]: https://github.com/ldclabs/ic-panda/compare/dmsg-v1.0.1...dmsg-v1.0.2
[1.0.1]: https://github.com/ldclabs/ic-panda/compare/dmsg-v1.0.0...dmsg-v1.0.1
[1.0.0]: https://github.com/ldclabs/ic-panda/releases/tag/dmsg-v1.0.0
