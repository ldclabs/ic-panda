# dMsg desktop

The Electron shell of a dMsg device. Every key, all state and every protocol
run in the `dmsg` daemon ([`src/dmsg_cli`](../dmsg_cli/README.md)); the
desktop is one of its clients, like the CLI.

- **Main process** (`src/main`): runs `dmsg start` (which starts the daemon
  when none runs) and gets the daemon's address and owner credential from
  `dmsg app-token`, keeps one `/ws/app/v1` connection, forwards its events,
  owns the tray, notifications, `dmsg://` links and updates.
- **Preload** (`src/preload`): `window.dmsg`, a narrow bridge; the credential
  never reaches a renderer.
- **Renderer** (`src/renderer`, Svelte 5): the main window (vault, channels,
  inbox, plan and names, agents, signatures and approvals, settings with
  connected browsers) and the approval window.

Rules (`src/main/policy.ts`):

- Windows call only the methods in `WINDOW_METHODS`; `human` is stripped and
  requests the desktop starts are marked `source: "gui"`. Methods that name
  local files (`wallet/use`, `channel/send_file`, `channel/download`),
  refunds and running a tool as an agent (`ap/call`) stay with the CLI.
- A payment (paid letter, checkout, name) is only a request from the main
  window; the approval window shows its amount, ledger and payer.
- The Agents view enables the account's principal, registers and retires
  controllers, grants and revokes credentials (each an approval request; the
  approval window shows the agent, scopes, relying parties and expiry), hosts
  Agent IDs (residency, the MCP credential, shown once) and grants them
  capabilities. The host allowlist is shown, never edited.
- Only the approval window decides a request (`approval/respond` with the
  digest of what it showed and `human: true`, added by the main process);
  only the main window confirms a browser pairing (`connect/grant`).
- `dmsg://connect/pair?account=…` and `dmsg://connect/request?account=…&request=…`
  only open a view; they execute nothing.
- The system browser opens only the login page and the bridge's pairing page.
- Renderers run sandboxed with context isolation, no Node integration, a
  strict CSP, no navigation and no new windows. Closing the main window keeps
  the tray and the daemon; quitting does not stop the daemon.

Notifications say only that a request waits, never from where; clicking one
opens the approval window.

```sh
pnpm --dir src/dmsg_desktop dev        # needs a dmsg binary on PATH or DMSG_BIN
pnpm --dir src/dmsg_desktop test       # policy and update unit tests
pnpm --dir src/dmsg_desktop test:e2e   # build, then the Playwright-for-Electron smoke test
```

The smoke test runs the built app in a temporary profile against a mock
daemon: the main window, a `dmsg://` link opening the approval window,
refusal of decisions from the main window, the exact decision the approval
window sends, a channel read and send, the inbox, a subscription that
reaches the daemon only as a desktop-sourced approval request, and an agent
grant whose approval window shows the agent it names.

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
