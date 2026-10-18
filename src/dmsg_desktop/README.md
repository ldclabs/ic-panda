# dMsg desktop

The Electron shell of a dMsg device. Every key, all state and every protocol
run in the `dmsg` daemon ([`src/dmsg_cli`](../dmsg_cli/README.md)); the
desktop is one of its clients, like the CLI.

- **Main process** (`src/main`): runs `dmsg start` (which starts the daemon
  when none runs) and gets the daemon's address and owner credential from
  `dmsg app-token`, keeps one `/ws/app/v1` connection, forwards its events,
  owns the tray, notifications, file dialogs, the clipboard, `dmsg://` links
  and updates. Its text (`src/main/text.ts`) is in the interface language.
  While it is not connected, the main window shows why: the error `dmsg`
  reported, such as a `config.toml` it refused. Without a `config.toml` the
  daemon uses the production deployment built into `dmsg`.
- **Languages** (`src/shared/locales.ts`): English, Simplified Chinese,
  Russian, Arabic, French and Spanish, as on the website. The main process
  resolves one (the person's choice, kept in the profile's `locale` file,
  else the system's preferred languages, else English) and passes it to
  each window; the language menu in the main window changes it for the tray,
  dialogs and both windows at once. Arabic lays the windows out right to
  left: panes, menus and directional icons mirror, while IDs, code and
  numbers keep their left-to-right order.
- **Appearance** (`src/shared/appearance.ts`): the system's, light or dark,
  kept in the profile's `appearance` file. The main process sets Electron's
  `nativeTheme` from it, so both windows, their menus and dialogs follow it
  through `prefers-color-scheme`.
- **Preload** (`src/preload`): `window.dmsg`, a narrow bridge; the credential
  never reaches a renderer.
- **Renderer** (`src/renderer`, Svelte 5, Tailwind CSS 4): the main window
  and the approval window, designed in [DESIGN.md](DESIGN.md) (the plan and
  its record: [docs/dmsg_desktop_design_zh.md](../../docs/dmsg_desktop_design_zh.md)).
  `app.css` holds the dMsg tokens of `src/dmsg_frontend/DESIGN.md` with a
  dark theme; `components/` is the kit the views compose: `ui.ts` (button
  and menu classes), Remix icons (`glyphs.ts`), and bits-ui checkboxes,
  radios, toggle groups, dropdowns, context menus, dialogs, tabs,
  suggestions, collapsibles and toasts in place of native controls. Inter, the icons and the EFF wordlist
  of the password generator (loaded only for a passphrase) are bundled; their
  licenses ship in `public/licenses`. Motion stops when the system asks
  for reduced motion. The text is in `i18n/messages`, one catalog per
  language, each checked against the English one.

  The main window's shell (`shell/`) has a sidebar with the account
  switcher (this device's accounts, another login), the views in three
  groups (work; authorization; account) and Settings, and at its foot the
  gate rail: locked or not with the idle lock's countdown (`lock_at` from
  `daemon/status`), the step that opens the device when it is not (Log in,
  Unlock with Touch ID, Fast unlock, or Continue setup in Settings), the
  versions waiting to sync,
  background online while it is on, the daemon connection, the language and
  appearance menus and Lock. Views the device cannot show yet offer the same
  step. A title bar names the view and opens the command palette (⌘K, or
  Ctrl+K elsewhere), which goes to any view and runs the session,
  appearance and language commands; ⌘1–⌘8 and ⌘, go to a view, ⌘L locks.
  Beside the sidebar toggle, back and forward (⌘[ and ⌘], or Alt+← and
  Alt+→) walk the views the window showed. The sidebar collapses to icons,
  and the title bar then takes the toggle and the arrows. On macOS the
  window has no title bar of its own: the traffic lights sit over the
  sidebar, on the window's material, and the buttons follow them; full
  screen hides the lights and the buttons move to the edge. A legacy
  pairing that was started and not imported shows a dismissible notice.
  The main window has these views:
  - Vault: filters (starred, each type and tag, the trash), the list and the
    open item side by side, with a context menu on each row; new items in a
    dialog, edits in place; conflict copies compared side by side, line by
    line, and kept or adopted. The command palette finds items by title.
  - Files: the library's folders (each with a context menu), starred
    files, the tags, the cloud backups and the trash on the left; the
    current folder in the middle, with a breadcrumb back up, a sort (name,
    time, size or type, remembered) and its rows rendered a page at a
    time (search within it, or across the library once the text leaves it,
    the results showing where they lie); the selected file on the right
    with a preview of any version, tags, note, file statements, versions
    and what each version has (on this device, backed up, only in the
    cloud), or the selected folder with its counts and backup states. The
    + menu adds files or a folder from the system dialog, which comes in as
    a folder, or makes a new one; drops go into the folder shown or the
    folder of the list or tree they land on. A name already in the folder
    is asked about in a system dialog (a new version, or both kept). An
    import shows one card per file or picked folder and can be cancelled.
    Rows drag into folders; several selected rows back up, move, export,
    star or go to the trash together, and the keyboard selects (⌘A,
    Shift+arrows), opens (Enter), renames (F2), trashes (Delete, ⌘⌫) and
    goes up (⌘↑). Moving to the trash can be undone from its notice;
    deleting for good, deleting a version and freeing a version's local
    space ask first. Backing up asks first and shows the cloud's room and
    what is not yet in it; sharing to a channel sends a fresh edition under
    its own key; a file statement over a version's SHA-256 is signed in the
    approval window and its bundle exported from the file; a folder's agent
    access (`files.read`, `files.write`) is granted from its menu. The
    sidebar's foot checks the whole library, lists the plaintext copies
    still open, and mounts the library as a disk when allowed (`files_mount`
    or Settings › Files, which asks in a system dialog), after the system's
    caveat; deleting a cloud copy is a second, danger-toned confirmation
    and leaves the device alone, as the trash leaves the cloud alone. The
    cloud view reads the relay once when it first opens, and a backup is
    fetched where it was or into a folder picked. A backup under way can be
    cancelled from its card. Open hands a temporary copy to its program; a
    copy another program changed is offered as a new version before it is
    removed (`files/open_changed`). Routes `/files`, `/files/cloud`,
    `/files/trash`.
  - Channels: the list, the open channel's timeline by day and its
    composer, and the members in a drawer. A channel opens with its latest
    200 messages and then reads only those after the last one shown. The
    library's files for an attachment are read when its menu opens. A
    message shows as sending
    until the relay stores it, and stays with a retry if the daemon
    refused it; unsent text is kept per channel. Attachments come from the
    file library (as shared editions under their own keys) or a file added
    to it on the spot, and a received file can be saved into the library;
    roles, rotation, removal and history grants are
    in each member's menu; creating, joining, leaving, and ownership and
    billing transfers by packet open in dialogs, a transfer in its three
    steps.
  - Inbox: the inbox, sent letters and the archive beside the open letter;
    a new letter, and who may write (nobody, contacts, everyone, or a price
    per letter), open in dialogs.
  - Plan and names: the plan and its limits, the plans on offer as cards,
    checkouts, names with their transfers, the payer and the payment
    operations.
  - Team wallets (`/wallet`): the account's wallets with members, policy
    and addresses checked against the wallet's keys, proposals with their
    approvals, balances, the audit chain and this device's cosigner keys;
    proposing, executing and broadcasting run here, while creating,
    joining, deciding and cosigning open the approval window. Activation
    payments this device made that created no wallet are listed with
    what comes back; a failed creation's or an expired quote's is refunded
    to the paying account with one click.
  - Agents, in tabs: the principal with its controllers and credentials,
    the hosted Agent IDs and the host allowlist, a matrix of what each
    Agent ID reaches (the Agent Protocols services by origin, and other
    capabilities) with every grant, Agent Mail and Knowledge, MCP Events
    endpoints, and the audit record as a timeline.
  - Signatures and approvals, in tabs: the requests waiting (each opened,
    or withdrawn when this device's own clients made it, with its time
    left and why an approval did not run), running (reconciled now on
    request) and decided (filtered by kind, opened read-only); signing,
    after a readiness check (login, formal approval on this device, which
    turns on or off here, the signing policy): text, a statement about a
    file or a file's digest (the file picked in the main process's dialog,
    or a digest in hex), application actions, and the month's allowance;
    evidence: a bundle picked or dropped and verified check by check into a
    report that can be saved, and the account's signatures, its other
    devices' included, each verified in place or exported; and what the
    account has authorized (devices that approve, paired browsers, web
    sessions in force, agents' grants and credentials), each managed where
    it lives.
  - Settings, in sections with their own routes (`/settings/session`,
    `security`, `devices`, `profile`, `browsers`, `files`, `legacy`,
    `about`): this session and the device's accounts, fast unlock (on after
    every login unlock unless turned off here; it shows how it checks the
    person) and when the login expires, creating or joining an account,
    pairing and recovery, account security, devices, the public profile
    with its avatar, connected browsers, the file library's settings (the
    disk mount, the trash's days), the legacy migration, and About dMsg
    with the app's version and its update.

  Every login, from the gate rail, a locked view, the account menu, the
  palette, Settings or "Bind another login", runs one flow
  (`shell/login.svelte.ts`) shown in a dialog: the page opening, the login
  in the browser (with the page's expiry, Open the page again and Copy
  link), and the daemon's check, read from `session/login_result` on its
  `login/changed` and `session/changed` events. A failure says why (no
  network, cancelled in the browser, expired, refused) with the daemon's
  message and Retry; closing the dialog leaves a login finished later to
  open the device. A first login without an account goes on to Settings.

  A locked device with fast unlock opens without a login
  (`shell/session.ts`): the daemon asks for Touch ID, or the Mac password,
  where the Secure Enclave keeps the key, and brings the login back too.
  The main window asks once when it comes to the front, or opens locked in
  front; never right after the person locked it there, and not again after
  a decline, when the gate's Unlock with Touch ID asks. Locked views also
  offer "Log in with Internet Identity". Fast unlock that has run out
  (thirty days after the last login) goes to the login dialog.

  Pages share a small kit (`components/page`): sections, notices, state
  pills, ID chips that copy the whole ID, fields, facts, tab bars and the
  locked placeholder.

  Views follow the daemon's events (`rpc.ts` `onChange`): a view reads
  again when an event names what it shows, such as `vault/changed`,
  `files/changed` or `channel/changed`, and on `resync` after missed
  events. Its reads run one at a time (`shared/single.ts`), so a burst of
  events reads twice at most, and an action does not read again itself
  unless it opens what it made. Independent reads go out together.

Rules (`src/main/policy.ts`):

- Windows call only the methods in `WINDOW_METHODS`, those their views use;
  `human` is stripped and
  requests the desktop starts are marked `source: "gui"`. Methods that name
  local files (`payer/use`, `channel/download`, `attest/verify`) and refunds
  stay with the CLI. Files enter and leave only through the main process's
  open and save dialogs with the path the person picked, or, for the file
  library, files dropped on the window: the preload resolves each to its
  path (`webUtils.getPathForFile`), which a file a page made does not have:
  - library files (`files/import` with the paths and the folder,
    `files/export`, and `files/open`, whose temporary copy the main process
    hands to `shell.openPath`) and shared channel files (`channel/download`;
    `channel/send_file` names a library file and version);
  - an application action's CBOR (`attest/action`) and attestation bundles
    (`attest/export`, `attest/verify`);
  - legacy transfer files, frozen-history files and old attachments
    (`legacy/import`, `legacy/frozen`, `legacy/file`);
  - the public avatar (`profile/avatar`; removing it goes the same way). A
    window shows avatars as data URLs from `profile/avatar_image`, so it
    loads nothing from the network.
- `ap/call` runs only the Agent Mail and Agent Knowledge tools
  (`DESKTOP_TOOLS`) as a hosted Agent ID, in the connector session `gui`. The MCP Events methods only manage
  endpoints; a window never reads or acknowledges the deliveries meant for
  agents.
- A vault item's hidden field is shown, or copied, only on a second click
  within three seconds of the first (⌘⇧C, pressed twice, copies the main
  secret); shown, it hides again after a minute. The window asks the daemon
  for one field at a time (`vault/reveal`); an edit never loads a saved
  hidden value, which stays unless the person changes or clears it. A copy
  goes from the daemon to the clipboard through the main process, so the
  window never holds it, marked to stay out of clipboard histories and the
  universal clipboard (macOS `org.nspasteboard.ConcealedType` and
  `TransientType`, Windows' history and cloud exclusions, KDE's password
  hint), and leaves the clipboard after 30 seconds, or when dMsg locks or
  quits, unless something replaced it. A one-time password copies as its
  current code. Imports read only the export the person picked in the main
  process's dialog, and QR codes only a picked image or the clipboard's;
  items open their web addresses in the system browser, http and https
  only. dMsg's own keys (tagged `dmsg-`) are read-only.
- Revoking a device and changing its capabilities are approval requests.
- Only a person's input in a window (a click, a key, a scroll) and a
  successful unlock are activity for the idle lock. The main process
  declares so at `app/initialize` and reports input with `app/activity` at
  most every 10 seconds; the windows' reads, on events or the status
  refresh, never keep the device unlocked. Windows call neither method.
  The daemon records the unlock itself: the Touch ID the main window asks
  for as it comes to the front has no input in a window.
- Notifications say that something happened, never what or from whom: a web
  request waiting, deliveries to agents, and, while the daemon is locked, a
  channel's new messages or a new letter (the relay's `channel` and `inbox`
  hints), each kind at most once a minute.
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
  a second login in the system browser, followed by the login dialog; once
  it arrives the approval window opens on the binding. Approving another device's pairing takes the device
  ID shown on that device.
- A payment (paid letter, checkout, name) is only a request from the main
  window; the approval window shows its amount, ledger and payer. A web
  application's checkout is quoted in the approval window (an asset, or a
  PANDA neuron) before it can be approved.
- The Agents view enables the account's principal, registers, renames,
  rotates, retires and marks controllers compromised, grants, renews (under
  the same ID, never wider) and revokes credentials, grouped by agent with
  the profile hint an agent publishes (each change an approval request; the
  approval window shows the agent by name, scopes, relying parties,
  relationship and expiry), and lists what this device signed for them. It
  hosts Agent IDs: residency, pausing, the MCP credential, shown once with
  the Claude Code command and the `mcpServers` entry that run `dmsg mcp` with
  the installed binary and home (`dmsg:runtime`), withdrawing it, the
  sessions last used, the Agent Profile each published (signature checked,
  self-described) and removal. It grants capabilities from the daemon's
  vocabulary, checked before they are sent, for days or until revoked, and
  asks again for a grant reaching every target, a secret or a wallet's
  execution; vault tags and team wallets open the same grants for
  themselves, and a wallet member names the Agent IDs that propose for it.
  Agent Discourse shows what a connector holds (rooms, latest records, held
  drafts, pending inbox) through `ap/read`, which fetches and marks nothing;
  Agent Mail and Agent Knowledge run their tools as an Agent ID. The host
  allowlist and the blocked senders are shown, never edited. Each part
  loads on its own, so the Agent IDs show while locked or offline. Its audit
  record lists, locked or not, who did what on this device and through which
  way in (`audit/list`, followed through `audit/changed`): the owner's
  grants, credentials, residency and decisions, marked `gui` when they came
  from a window, and what agents called.
- Only the approval window decides a request (`approval/respond` with the
  digest of what it showed and `human: true`, added by the main process);
  only the main window confirms a browser pairing (`connect/grant`). The
  window reads as a ticket: the kind of decision and the time left, who
  asks and the way the request came (a website's through the paired browser
  and connect.dmsg.net, drawn as relayed), what is signed, paid or changed,
  what approving does, and the technical fields folded. An application
  action reads as its certified registration's schema words it; a file
  statement names its file, and a digest without one can be compared with a
  file the person picks. Reject has the focus; Approve opens 1.2 seconds
  after the request appears, again after it changes (a quote, for one) and
  when the window comes back to the front, and stays closed while the
  readiness check finds a blocker, which it names with the way to fix it.
  Requests the daemon rates high-risk (`risk`: payments, revoking a device,
  binding or removing a login, the signing policy, the recovery delay, the
  Agent principal) are approved by holding the button for a second and,
  where the Secure Enclave keeps the fast-unlock key, with Touch ID or the
  login password. After a decision the window shows each real step (for a
  web request: the relay's authorization, the result, the page taking it)
  and the way to the next waiting request. Locked, it shows nothing of the
  request and offers the unlock.
- `dmsg://connect/pair?account=…` and `dmsg://connect/request?account=…&request=…`
  only open a view; they execute nothing.
- The system browser opens only the login page, the bridge's pairing page
  and the old site's migration links.
- Renderers run sandboxed with context isolation, no Node integration, a
  strict CSP, no navigation and no new windows. Closing the main window keeps
  the tray and the daemon; quitting does not stop the daemon. In macOS full
  screen the window leaves full screen before it hides, since a window
  hidden there leaves its Space behind, black.

Notifications say only that a request waits, never from where; clicking one
opens the approval window. For agents' MCP Events they say how many
deliveries arrived, or that a subscription needs renewing, never what or
from where; clicking one opens the Agents view. The tray icon is the dMsg
mark with its diamond; while a request waits, a badge cut out of its top
right corner joins it. On macOS it is a template image; on Windows it keeps
its green tile, and the badge is amber.

```sh
pnpm --dir src/dmsg_desktop dev        # needs a dmsg binary on PATH or DMSG_BIN
pnpm --dir src/dmsg_desktop test       # policy, update, language and view-rule unit tests
pnpm --dir src/dmsg_desktop test:e2e   # build, then the Playwright-for-Electron smoke test
```

The smoke test runs the built app in a temporary profile against a mock
daemon. It answers the main process's file dialogs and watches its clipboard
and notifications instead of the system's. It covers:

- the main window, a `dmsg://` link opening the approval window, refusal of
  decisions from the main window and the exact decision the approval window
  sends;
- channels: a read and a send, the timeline's latest page and the reads
  after it, an attachment from the file library, a download, a
  role change and a history grant from a member's menu, an ownership
  proposal from its dialog, archiving and reopening;
- the inbox, its contact rules, and a subscription that reaches the daemon
  only as a desktop-sourced approval request;
- team wallets: an activation payment that created no wallet refunded
  with one click and no approval request, and one that waits for its
  quote to expire;
- agents: a grant whose approval window shows the agent it names, mail read
  and answered as an Agent ID, a refused non-Mail tool, an MCP Events
  endpoint whose secret shows once, and a delivery notification without its
  content;
- a web checkout that cannot be approved before it is quoted;
- the vault: a secret shown and copied only on a second click, with the
  clipboard markers, a one-time code, an edit that keeps the hidden values
  it never loaded and names the version it replaces, a version restored
  from the history, dMsg's own key read-only, the security check's filter,
  a new login from its dialog, a key pasted over several lines kept whole,
  a new item closed with something typed asking first, an import, a tag
  added to two items, a conflict copy merged part by part and an item found
  from the palette;
- the file library: a file added through the dialog, a file a page made
  refused, a backup cancelled from its card and a file moved to the trash
  from its row's menu;
- signatures: an application action shown by its schema's labels; the
  readiness check; a file statement whose path only the main process's
  dialog gives (a window's own path is dropped) and whose file the approval
  window names; a digest compared with a picked file; a request withdrawn;
  another device's signature listed, one verified in place, a bundle
  exported, verified, verified against a changed file check by check and
  its report saved; the authorizations in force; and the approval window
  hiding its request while locked and showing it again once unlocked;
- settings: an edit of the signing policy kept while events reload the
  page, a login binding approved in the approval window, a device
  revocation as an approval request, the avatar shown, set from a picked
  image and removed (the window cannot name a path), the locked daemon's
  `inbox` and `channel` hints notified once each, fast unlock turned on
  and a locked account opened with it instead of a login; with Touch ID, the
  locked window asking once as it comes to the front, a declined ask leaving
  it locked without a login, and the gate asking again; switching to
  another account's workspace, and About dMsg with the app's version and no update
  check in a development run (only the main window reaches updates);
- logging in: logged out, the gate rail, the account menu and the locked
  vault offer Log in; a start that fails shows its reason and retries, the
  page opens again on request, and the dialog follows the daemon's check to
  the unlocked vault;
- languages: picked only in the main window, kept, and followed by the
  approval window, Arabic right to left.

## Packages and updates

```sh
pnpm --dir src/dmsg_desktop package       # build, bundle dmsg, then dMsg-mac-arm64.dmg/.zip or dMsg-win-x64.exe
pnpm --dir src/dmsg_desktop package:dir   # the unpacked app only
```

- `scripts/prepare-runtime.mjs` bundles the release's `dmsg`
  (`DMSG_DESKTOP_RUNTIME`, else a local `cargo build --release`) in
  `resources/runtime`; its version must be the app's. The app never runs it
  as a daemon: each start runs `dmsg install` with it, which puts it at the
  shared location unless the one there is newer or Homebrew's and, on macOS,
  signed by the same Developer ID team, and the app
  then runs the installed `dmsg` (`src/main/runtime.ts`). If the install
  fails, the main window shows its error and the next start tries again. On macOS the bundled
  file keeps the release binary's own signature (`signIgnore`).
- `electron-builder.yml` is the base: ad hoc signed, no update feed;
  `package:dir` uses it. `electron-builder.local.cjs` (`package`) signs a
  local package with the keychain's Developer ID Application identity when
  there is one, with the release entitlements and without notarization, and
  gives it the release's update feed, so it downloads and installs releases
  signed by the same team; without one, or with
  `CSC_IDENTITY_AUTO_DISCOVERY=false`, it stays ad hoc with no feed. A
  macOS package gets the feed only with its dmg or zip, never from `--dir`.
  `electron-builder.release.cjs` (the release
  workflow) adds the Developer ID signature and notarization on macOS
  (required there), Authenticode on Windows when a certificate is configured,
  and the update feed of the GitHub release (`latest-mac.yml`, `latest.yml`).
- Packages register the `dmsg:` scheme (`CFBundleURLTypes` on macOS, the
  NSIS installer on Windows) besides `setAsDefaultProtocolClient` at run time.
  The Windows app installs per user under `%LOCALAPPDATA%\Programs\dMsg`,
  apart from the shared `dmsg`.
- A packaged app checks for updates at start and every six hours
  (`src/main/updater.ts`); the tray's "Check for Updates", the palette's
  command and Settings › About dMsg check on request, and a check from the
  tray says what it found. A package with the feed downloads the update in
  the background and never installs on quit. The tray and About dMsg then
  offer "Restart to Update", which first stops the daemon with
  `daemon/stop {if_idle: true}` (`src/main/update.ts`): only when no approved
  request, account update or root rotation is left unsettled, asking every two
  seconds for a minute; otherwise it says what is in flight and keeps both
  running. The new app installs and starts its own `dmsg`.
- A package without the feed (an ad hoc local package, or a macOS release
  built without the Developer ID) checks the same GitHub release feed but cannot
  install a signed release: the tray and About dMsg offer "Download dMsg
  <version>", which opens that release's page. A development run does not
  check.
- The tray's "Open at Login" is the app's login item; opened at login, the
  app starts in the tray. `dmsg autostart` starts only the daemon.
