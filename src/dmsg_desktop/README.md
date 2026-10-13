# dMsg desktop

The Electron shell of a dMsg device. Every key, all state and every protocol
run in the `dmsg` daemon ([`src/dmsg_cli`](../dmsg_cli/README.md)); the
desktop is one of its clients, like the CLI.

- **Main process** (`src/main`): runs `dmsg start` (which starts the daemon
  when none runs) and gets the daemon's address and owner credential from
  `dmsg app-token`, keeps one `/ws/app/v1` connection, forwards its events,
  owns the tray, notifications, file dialogs, the clipboard, `dmsg://` links
  and updates. Its text (`src/main/text.ts`) follows the app's locale,
  Simplified Chinese or English, as the windows' does.
- **Preload** (`src/preload`): `window.dmsg`, a narrow bridge; the credential
  never reaches a renderer.
- **Renderer** (`src/renderer`, Svelte 5): the main window and the approval
  window. The main window has these views:
  - Vault: notes, logins, API credentials, keys and files; details and edits,
    stars, and conflict copies kept or adopted.
  - Channels: attachments, roles, history grants, leaving, archiving and
    reopening, and ownership and billing transfers by packet.
  - Inbox; plan and names, with name transfers and checkout records.
  - Agents: Agent Mail, Agent Knowledge and MCP Events endpoints.
  - Signatures and approvals: the month's execution allowance, application
    actions, and exporting and verifying attestation bundles.
  - Legacy migration.
  - Settings: this device's accounts, creating or joining an account, pairing
    and recovery, account security, devices, the public profile and connected
    browsers.

Rules (`src/main/policy.ts`):

- Windows call only the methods in `WINDOW_METHODS`; `human` is stripped and
  requests the desktop starts are marked `source: "gui"`. Methods that name
  local files (`wallet/use`, `channel/download`, `attest/verify`) and refunds
  stay with the CLI. Files enter and leave only through the main process's
  open and save dialogs with the path the person picked:
  - vault files (`file/import`, `file/export`) and shared channel files
    (`channel/download`; `channel/send_file` names a vault item);
  - an application action's CBOR (`attest/action`) and attestation bundles
    (`attest/export`, `attest/verify`);
  - legacy transfer files, frozen-history files and old attachments
    (`legacy/import`, `legacy/frozen`, `legacy/file`).
- `ap/call` runs only the Agent Mail and Agent Knowledge tools
  (`DESKTOP_TOOLS`) as a hosted Agent ID, in the connector session `gui`. The MCP Events methods only manage
  endpoints; a window never reads or acknowledges the deliveries meant for
  agents.
- A vault secret is shown, or copied, only after a second click. A copy goes
  from the daemon to the clipboard through the main process, so the window
  never holds it, and the clipboard is cleared after 30 seconds unless
  something replaced it.
- Revoking a device and changing its capabilities are approval requests.
- Legacy migration opens the old site (`https://dmsg.net/legacy` or
  `https://panda.fans/legacy`) only with an offer (`#pair=`) or an approval
  request (`#approve=`) in the fragment; the person compares the fingerprint,
  this device's ID and the old Principal on both sides before importing. The
  shared-channel documents (frozen source, proposal, directory record, the
  old site's approvals) are pasted or read from a chosen file.
- Each account on the device keeps its own workspace. Logging in with another
  identity switches to its account; Settings lists the accounts here and
  switches between them (locked, then a login or fast unlock opens one).
- Binding another login, removing one, the signing policy, the recovery delay
  and disputing a recovery are approval requests. "Bind another login" starts
  a second login in the system browser; once it arrives the approval window
  opens on the binding. Approving another device's pairing takes the device
  ID shown on that device.
- A payment (paid letter, checkout, name) is only a request from the main
  window; the approval window shows its amount, ledger and payer. A web
  application's checkout is quoted in the approval window (an asset, or a
  PANDA neuron) before it can be approved.
- The Agents view enables the account's principal, registers and retires
  controllers, grants and revokes credentials (each an approval request; the
  approval window shows the agent, scopes, relying parties and expiry), hosts
  Agent IDs (residency, the MCP credential, shown once) and grants them
  capabilities. The host allowlist is shown, never edited. Its audit record
  lists, locked or not, who did what on this device and through which way in
  (`audit/list`): the owner's grants, credentials, residency and decisions,
  marked `gui` when they came from a window, and what agents called.
- Only the approval window decides a request (`approval/respond` with the
  digest of what it showed and `human: true`, added by the main process);
  only the main window confirms a browser pairing (`connect/grant`).
- `dmsg://connect/pair?account=…` and `dmsg://connect/request?account=…&request=…`
  only open a view; they execute nothing.
- The system browser opens only the login page, the bridge's pairing page
  and the old site's migration links.
- Renderers run sandboxed with context isolation, no Node integration, a
  strict CSP, no navigation and no new windows. Closing the main window keeps
  the tray and the daemon; quitting does not stop the daemon.

Notifications say only that a request waits, never from where; clicking one
opens the approval window. For agents' MCP Events they say how many
deliveries arrived, or that a subscription needs renewing, never what or
from where; clicking one opens the Agents view.

```sh
pnpm --dir src/dmsg_desktop dev        # needs a dmsg binary on PATH or DMSG_BIN
pnpm --dir src/dmsg_desktop test       # policy and update unit tests
pnpm --dir src/dmsg_desktop test:e2e   # build, then the Playwright-for-Electron smoke test
```

The smoke test runs the built app in a temporary profile against a mock
daemon. It answers the main process's file dialogs and watches its clipboard
and notifications instead of the system's. It covers:

- the main window, a `dmsg://` link opening the approval window, refusal of
  decisions from the main window and the exact decision the approval window
  sends;
- channels: a read and a send, an attachment from the vault, a download, a
  role change, a history grant, an ownership proposal, archiving and
  reopening;
- the inbox, and a subscription that reaches the daemon only as a
  desktop-sourced approval request;
- agents: a grant whose approval window shows the agent it names, mail read
  and answered as an Agent ID, a refused non-Mail tool, an MCP Events
  endpoint whose secret shows once, and a delivery notification without its
  content;
- a web checkout that cannot be approved before it is quoted;
- the vault: a secret shown and copied only after confirmation, an edit that
  names the version it replaces, a new login item and an adopted conflict
  copy;
- signatures: an application action opened in the approval window, and a
  bundle exported and verified through dialogs;
- settings: a login binding approved in the approval window, a device
  revocation as an approval request, and switching to another account's
  workspace.

## Packages and updates

```sh
pnpm --dir src/dmsg_desktop package       # build, bundle dmsg, then dMsg-mac-arm64.dmg/.zip or dMsg-win-x64.exe
pnpm --dir src/dmsg_desktop package:dir   # the unpacked app only
```

- `scripts/prepare-runtime.mjs` bundles the release's `dmsg`
  (`DMSG_DESKTOP_RUNTIME`, else a local `cargo build --release`) in
  `resources/runtime`; its version must be the app's. The app never runs it
  as a daemon: each start runs `dmsg install` with it, which puts it at the
  shared location unless the one there is newer or Homebrew's, and the app
  then runs the installed `dmsg` (`src/main/runtime.ts`). On macOS the bundled
  file keeps the release binary's own signature (`signIgnore`).
- `electron-builder.yml` makes local packages: ad hoc signed, no update feed.
  `electron-builder.release.cjs` (the release workflow) adds the Developer ID
  signature and notarization on macOS (required there), Authenticode on
  Windows when a certificate is configured, and the update feed of the
  GitHub release (`latest-mac.yml`, `latest.yml`).
- Packages register the `dmsg:` scheme (`CFBundleURLTypes` on macOS, the
  NSIS installer on Windows) besides `setAsDefaultProtocolClient` at run time.
  The Windows app installs per user under `%LOCALAPPDATA%\Programs\dMsg`,
  apart from the shared `dmsg`.
- A package with a feed checks it at start and every six hours and downloads
  in the background (`src/main/updater.ts`); it never installs on quit. The
  tray then offers "Restart to Update", which first stops the daemon with
  `daemon/stop {if_idle: true}` (`src/main/update.ts`): only when no approved
  request, account update or root rotation is left unsettled, asking every two
  seconds for a minute; otherwise it says what is in flight and keeps both
  running. The new app installs and starts its own `dmsg`.
- The tray's "Open at Login" is the app's login item; opened at login, the
  app starts in the tray. `dmsg autostart` starts only the daemon.
