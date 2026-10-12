# dMsg desktop

The Electron shell of a dMsg device. Every key, all state and every protocol
run in the `dmsg` daemon ([`src/dmsg_cli`](../dmsg_cli/README.md)); the
desktop is one of its clients, like the CLI.

- **Main process** (`src/main`): gets the daemon's address and owner
  credential from `dmsg app-token` (starting `dmsg daemon` when none runs),
  keeps one `/ws/app/v1` connection, forwards its events, owns the tray,
  notifications and `dmsg://` links.
- **Preload** (`src/preload`): `window.dmsg`, a narrow bridge; the credential
  never reaches a renderer.
- **Renderer** (`src/renderer`, Svelte 5): the main window (vault, signatures
  and approvals, settings with connected browsers) and the approval window.

Rules (`src/main/policy.ts`):

- Windows call only the methods in `WINDOW_METHODS`; `human` is stripped and
  requests the desktop starts are marked `source: "gui"`.
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
pnpm --dir src/dmsg_desktop test       # policy unit tests
pnpm --dir src/dmsg_desktop test:e2e   # build, then the Playwright-for-Electron smoke test
```

The smoke test runs the built app in a temporary profile against a mock
daemon: the main window, a `dmsg://` link opening the approval window,
refusal of decisions from the main window, and the exact decision the
approval window sends. Packaging, signing and updates come with the release
work package (W8).
