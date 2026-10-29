# dMsg desktop

The Electron shell of a dMsg device. Every key, all state and every protocol
run in the `dmsg` daemon ([`src/dmsg_cli`](../dmsg_cli/README.md)); the
desktop is one of its clients, like the CLI.

- **Main process** (`src/main`): runs `dmsg start` (which starts the daemon
  when none runs) and gets the daemon's address and owner credential from
  `dmsg app-token`, keeps one `/ws/app/v1` connection, forwards its events,
  owns the tray, notifications, file dialogs, the clipboard, `dmsg://` links
  and updates. `src/main/text.ts` selects its interface text from the per-language
  catalogs in `src/main/messages`; `messages/en.ts` defines their `Text` contract.
  While it is not connected, the main window shows why: the error `dmsg`
  reported, such as a `config.toml` it refused. Without a `config.toml` the
  daemon uses the production deployment built into `dmsg`.
- **Languages** (`src/shared/locales.ts`): English, Simplified and Traditional
  Chinese, Russian, Arabic, French, Spanish, Hindi, Indonesian, Brazilian
  Portuguese, Japanese, German, Korean, Italian and Turkish. The saved choice
  takes precedence over system preferences and updates both windows, the tray,
  notifications and native dialogs. TW/HK/MO share one Traditional catalog;
  Portuguese preferences use the Brazilian catalog. Arabic uses RTL layout.
- **Appearance** (`src/shared/appearance.ts`): the system's, light or dark,
  kept in the profile's `appearance` file. The main process sets Electron's
  `nativeTheme` from it, so both windows, their menus and dialogs follow it
  through `prefers-color-scheme`.
- **Preload** (`src/preload`): `window.dmsg`, a narrow bridge; the credential
  never reaches a renderer.
- **Renderer** (`src/renderer`, Svelte 5, Tailwind CSS 4): the main window
  and the approval window, designed in [DESIGN.md](DESIGN.md) (the plan and
  its record: [archived desktop design record](../../archived/docs/dmsg_desktop_design_zh.md)).
  `app.css` holds the dMsg tokens of `src/dmsg_frontend/DESIGN.md` with a
  dark theme; `components/` is the kit the views compose: `ui.ts` (button
  and menu classes), Remix icons (`glyphs.ts`), and bits-ui checkboxes,
  radios, toggle groups, dropdowns, context menus, dialogs, tabs,
  suggestions, collapsibles and toasts in place of native controls. Inter, the icons and the EFF wordlist
  of the password generator (loaded only for a passphrase) are bundled; their
  licenses ship in `public/licenses`. Motion stops when the system asks
  for reduced motion. The text is in `i18n/messages`, one catalog per
  language, each checked against the English one.

  Native catalogs contain strings and typed formatting functions for Electron
  APIs; renderer catalogs contain page strings with named placeholders, used by
  the reactive Svelte translation layer. Both share `src/shared/locales.ts`.
  The catalogs themselves are pure modules; reusable text shared by both processes
  belongs in `src/shared`, while the renderer's runtime depends on Svelte and DOM APIs.

  The main window's shell (`shell/`) has a sidebar with the account
  switcher (this device's accounts, another login), the views in three
  groups (work; authorization; account) and Settings, and at its foot the
  gate rail: while the device is not unlocked, its state and the step that
  opens it (Log in, Unlock with Touch ID, Fast unlock, or Continue setup in
  Settings); the versions waiting to sync, background online while it is
  on, the daemon connection, and one Lock button while it is unlocked.
  Views the device cannot show yet offer the same step. A title bar names
  the view and opens the command palette (⌘K, or Ctrl+K elsewhere), which
  goes to any view and runs the session, appearance and language commands;
  ⌘1–⌘8 and ⌘, go to a view, ⌘L locks.
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
    The list and saved selections appear without waiting for conflict or
    security checks; security findings are reused until item versions or
    the date change. Late reads cannot repopulate a locked/account-switched
    view, and leaving an item stops its one-time-code refreshes.
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
    per letter), open in dialogs. Verified cached letters appear first;
    remote letters and rules refresh independently, and a network failure
    leaves cached letters readable. Approval events update sent items without
    downloading the inbox again. Send retries keep the same order ID, and
    late replies cannot restore content after locking or switching accounts.
  - Plan and names: the plan and its limits, the plans on offer as cards,
    checkouts, names with their transfers, the payer and the payment
    operations. Each section refreshes independently; a slow usage or balance
    read cannot delay a newer plan or name result. Local checkout and name
    journals appear before remote status checks, with order reads bounded to
    four at once. A fundless checkout retries its still-valid quote; closing
    the dialog or changing accounts prevents late replies from opening approval.
  - Team wallets (`/wallet/<wallet>/<proposal?>`): Overview, Proposals,
    Assets and addresses, Members and policy, Audit, and Operations. Exact
    asset-unit amounts and per-chain fee previews precede transfers; the
    wallet rechecks policy. Member, policy, address-book, cosigner, unfreeze,
    refuel and upgrade forms create typed proposals. Approval tickets show
    every intent and its changes. Proposals page, show votes and receipts,
    and distinguish execution, rebroadcast and replacement. Receive views
    include verified QR codes and both ICP account formats. Cosignatures
    from separate devices can be checked, collected and explicitly executed
    for one plan. RPC preferences belong to this account on this device;
    audit pages verify against a fresh certified head, and optional channel
    cards are encrypted hints. See [the team-wallet guide](../../docs/dMsg_wallet.md).
  - Agents, in tabs: the principal with its controllers and credentials,
    the hosted Agent IDs and the host allowlist, a matrix of what each
    Agent ID reaches (the Agent Protocols services by origin, and other
    capabilities) with every grant, Agent Mail and Knowledge, MCP Events
    endpoints, and the audit record as a timeline. Principal, credential and
    local journal reads complete independently and run only on the principal
    tab; unrelated Agent changes do not reload offscreen network panels.
    Session changes clear private views and discard late replies.
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
    it lives. Pending requests load independently of signature history, which
    is read when Evidence is opened. Authorization sections display their own
    results without waiting for unrelated queries, with failures shown locally.
    The approval window ignores reads superseded by a decision or session
    change; locking clears its request and stops any approval hold.
  - Settings, in sections with their own routes (`/settings/session`,
    `security`, `devices`, `profile`, `browsers`, `files`, `appearance`,
    `legacy`, `about`): this session and the device's accounts, fast unlock
    (on after every login unlock unless turned off here; it shows how it
    checks the person) and when the login expires, creating or joining an
    account, pairing and recovery, account security, devices, the public
    public PrincipalDocument (type, name, description, avatar and named links),
    connected browsers, the file library's settings
    (the disk mount, the trash's days), the interface language and appearance,
    the legacy migration, and About dMsg with the app's version and its update.
    Each section loads only what it uses: local accounts do not wait for account
    queries, and Devices does not repeat the parent page's account read. Browser
    pairing polls finish before scheduling the next poll and stop on leaving the
    section or changing sessions; late replies cannot restore an old invitation.
    File settings coalesce refreshes and reject reads superseded by a save.

  The public document editor reads its document, publication state and optional
  legacy archives independently; delayed archives cannot block editing. Lookup
  of another profile has its own loading state. Publication uses the account
  home and certified directory. It preserves link images in `extra["dmsg.link_images"]`, including
  legacy imports. Avatar uploads only change the draft; Publish commits the
  document. A delayed directory publication stays pending with a retry action.
  Reopening the editor also checks the local account journal: a lost update
  reply still requires reconciliation even when the directory already published it.

  Every login, from the gate rail, a locked view, the account menu, the
  palette, Settings or "Bind another login", runs one flow
  (`shell/login.svelte.ts`) shown in a dialog: the page opening, the login
  in the browser (with the page's expiry, Open the page again and Copy
  link), and the daemon's check, read from `session/login_result` on its
  `login/changed` and `session/changed` events. A failure says why (no
  network, cancelled in the browser, expired, refused) with localized text,
  a stable error code and Retry; closing the dialog leaves a login finished later to
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

  Network recovery is shared by all views. The daemon's IC/relay transports
  make bounded read retries; the desktop shows one reconnecting/offline banner
  after exhausted transient reads, keeps already loaded content, and retries
  only reviewed reads from `shared/requests.ts` in the background (5/10/20/40/60
  seconds). Recovery emits `resync`, removing stale error notices as views
  refresh. Locks, account changes and daemon disconnects discard retry queues
  and late read replies. No retry changes a read into an approval or mutation.
  New/unknown methods default to mutations until their semantics are reviewed.

  Desktop read RPCs time out after 45 seconds; writes after 180 seconds, while
  existing long file jobs remain untimed. A lost or timed-out submitted write
  reports `EXECUTION_UNKNOWN`, never a definite failure, and is never replayed
  by the desktop. A write whose own reads were unavailable, or one called while
  disconnected, reports `UNAVAILABLE`: nothing retries it, so it asks to be
  tried again rather than showing the recovering reads' notice. Concurrent
  identical calls share one in-flight request.
  The daemon's existing journals reconcile original approval, payment and
  signing operations. Transient failures and unconfirmed outcomes use localized
  warning text; raw service messages, URLs and tokens are not shown in notices.
  Known data-integrity and permission failures retain error treatment. Detailed
  causes are available through the allowlisted daemon diagnostics (`dmsg logs`).

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
  - the public avatar (`profile/avatar` for an upload; removing it only edits
    the draft). A window shows avatars as data URLs from
    `profile/avatar_image { avatar_url }`, using the full PrincipalDocument URL
    for both uploaded images and retained legacy HTTPS links. The daemon reads
    at most 512 KiB, accepts PNG/JPEG/WebP, sends no credentials and follows no
    redirects; the window loads nothing from the network. Successful document
    publication or retry emits `profile/changed`, refreshing the sidebar's
    name and avatar. Uploading or editing an unpublished draft does not.
- `ap/call` allows mailbox maintenance and inspection of Agent Mail and public
  Agent Knowledge (`DESKTOP_TOOLS`), in the connector session `gui`. The desktop
  cannot send/reply, publish/assess/retract knowledge, or mark Agent mail read.
  Mail viewing preserves the Agent’s unread queue; Knowledge defaults to the
  selected Agent’s contributions and shows service-scoped lifecycle and evidence.
  Opening Mail and changing the selected Agent show saved correspondence
  immediately while receiving in the background. Changing folders reads local
  mail only; refreshing explicitly receives again. Local inspection does not
  wait for the connector’s network turn or rewrite its snapshot. The daemon
  also syncs agents with Mail read access every 30 seconds (resident agents
  only while locked). `agent/mail` refreshes the visible list without another
  network sync; a new-letter notification shows only a count and opens that
  Agent's Mail view. Sent letters say "Accepted by relay", with an explanation
  that receipt and reading by the recipient have not been confirmed.

  The Mail view also exposes the owner's real-time receive switch through
  `agent/mail_push` and `agent/mail_push_set`. It shows subscription setup, active
  leases and fallback status; it never reads callback secrets. Enabling requires
  existing Mail read and event-endpoint grants. Pausing, revoked grants and locked
  nonresident agents retain their existing access limits.

  The MCP Events methods only manage
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
- System notifications use fixed, content-free text and the shared Rust policy.
  Messages and approvals aggregate for three seconds, with persistent per-source
  and installation budgets; ordinary sounds default off. Agent Mail alerts are
  opt-in per Agent after verified local receipt. MCP events and relay hints only
  wake synchronization. The global Reminders view keeps unresolved work and
  recent history separate from business unread state.
- A new installation starts with system notifications off. An installation
  upgraded from a version without these settings keeps alerting its selected
  account, as before. The app badge, tray and Reminders count include the
  selected account whether or not it alerts.
- Recovery notices retain their incident identity and phases across restarts.
  Certified public observation works while locked; a verified execution time
  enables the 24-hour and one-hour phases. Missing time never prevents the first
  warning. Registered inactive accounts receive minimal metadata with scoped
  credentials; no content roots are loaded. Clicks resolve the original account
  and item, require explicit account switching/unlock, and reread business state.
  Closing the window retains the tray presenter; fully quitting stops local
  system presentation while the daemon may continue synchronization. See the
  [notification validation record](docs/notifications-validation.md) for native
  OS and remote-provider limits.
- Legacy migration opens the old site (`https://dmsg.net/legacy` or
  `https://panda.fans/legacy`) only with an offer (`#pair=`). Pairing and archive
  reads wait for an unlocked account. Before then, the page explains whether
  to sign in, unlock, finish setup or approve the device, and links to Session
  and accounts. An account whose first content root was interrupted offers
  **Retry account setup**, which safely resumes the existing account.
  Personal history channels can be deleted after confirmation: the archive is rewritten
  without their messages, files, channel keys and cached copies, and the
  change syncs to the account's other devices. Old revision retention and
  the original exported migration file are separate from this deletion.
  Shared-channel inheritance has been removed. Empty, unactivated legacy
  drafts without pending activation can be deleted from the channel menu.
- Each account on the device keeps its own workspace. Logging in with another
  identity switches to its account; Settings lists the accounts here and
  switches between them (locked, then a login or fast unlock opens one).
- Binding another login, removing one, the signing policy, the recovery delay
  and disputing a recovery are approval requests. "Bind another login" starts
  a second login in the system browser, followed by the login dialog; once
  it arrives the approval window opens on the binding. Its live login stays
  in the daemon after acceptance, so a legacy holder can claim a name without
  changing the account's current login. The Names lookup refreshes after
  binding and approval; once the holder's credential is unavailable it offers
  a login without asking to bind again. The credential expires with its
  delegation and is discarded on removal, lock or account switch.
  Approving another device's pairing takes the device ID shown on that device
  and opens a high-risk approval request with its role and capabilities.
- Security → Approval keys shows this Mac's Secure Enclave approval key,
  every device's and the purposes keys are required for. Registering a key
  (optionally Touch ID only), retiring one, requiring keys for account,
  device and Agent principal changes (and optionally payments and formal
  signatures) and approving another device's key request are approval
  requests; requiring keys is offered only once this device has one, and the
  approval window lists the devices it shuts out. "Ask another device" makes
  a key here, asks Touch ID once and shows the request to approve on a
  device with a key. On a device with a registered key a high-risk or
  protected approval asks Touch ID through the key instead of the presence
  check.
- Security → Device-bound unlock binds the account's local data to this
  Mac's Secure Enclave (asking Touch ID once) and lists what it leaves out;
  unbinding needs a login.
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
  the Codex and Claude Code commands, Codex / ChatGPT desktop TOML, generic
  `mcpServers` JSON and Anda bot JSON (with its optional
  `events.webhook_ingress` routing flag). All use the installed binary and
  home (`dmsg:runtime`). The dialog explains Anda's `events.endpoint` grant
  and event automation, and where each
  client keeps the credential; see [MCP client setup](../dmsg_cli/README.md#connecting-mcp-clients).
  It also offers credential withdrawal, the
  sessions last used, the Agent Profile each published (signature checked,
  self-described) and removal. It grants capabilities from the daemon's
  vocabulary, with clickable suggestions and editable targets, checked before
  they are sent (unfilled target placeholders cannot be granted), for days or until revoked, and
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
  Requests the daemon rates high-risk (`risk`: payments, adding or revoking a
  device, changing its capabilities, binding or removing a login, the signing policy, the recovery delay, the
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

Notifications never approve, read, sign for or complete business work. Clicking
opens the original account's item or filtered reminder list. System submission,
actual viewing and business completion remain separate. The tray icon is the dMsg
mark with its diamond. A confirmed locked session adds a small padlock at the
bottom right; unlocking removes it. The top-right approval badge remains visible
independently, including while locked. Unknown/connecting states do not claim a
confirmed lock. On macOS all four states use template images with 1x/2x assets;
Windows keeps its green tile and amber approval badge. Regenerate the lock variants
with `node scripts/generate-tray-locks.mjs` (requires `rsvg-convert`).

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
- agents: a grant whose approval window shows the agent it names, mail inspected
  without changing Agent read marks, public knowledge scoped by Agent and service,
  refused authoring tools, an MCP Events
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
  page, a login binding approved in the approval window, a device pairing
  and an approval key's registration as approval requests, a device
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
- languages: picked in the main window's Settings, kept, and followed by the
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

The smoke test normally checks native macOS fullscreen transitions as well.
`DMSG_DESKTOP_SMOKE=settings pnpm test:e2e` checks all nine Settings routes,
profile publication and lookup drafts, device capability edits, file
preferences and the system mount confirmation, cache clearing, browser
pairing cancellation, late replies, setup/recovery eligibility, keyboard
navigation and a four-case theme/language/window-size screenshot matrix.
`DMSG_DESKTOP_SMOKE=security pnpm test:e2e` runs the isolated account-security
flows: unchanged/invalid drafts, combined policy approval, retry and refresh,
pending receipts, recovery and hardware settings, late replies, keyboard
navigation, and light/dark screenshots in Chinese, French and Arabic at the
minimum window size. It skips the unrelated full smoke flows.
`DMSG_DESKTOP_SMOKE=legacy pnpm test:e2e` stops after the account-setup and
legacy flows, including unavailable-account guidance and a failed initial
setup followed by a successful retry. Later flows are explicitly skipped.
On an automation host that cannot enter a native Space,
`DMSG_DESKTOP_NATIVE_FULLSCREEN=0 pnpm test:e2e` runs the functional window
flows and windowed hide/reopen assertions, and explicitly reports fullscreen
as skipped. This is not a passing native fullscreen validation.

## Cloud copies and sharing

The Cloud storage page, its navigation and file-detail links were removed on
2026-10-28. Membership shows account storage usage through `files/cloud_usage`.
Files → Links uses `files/share_links` and `files/share_revoke`; it remains independent
of the local source and can recover link metadata from other devices. Cloud backups
and file/version backup actions remain in Files. The desktop no longer allows or
calls the deprecated `files/storage`, `files/storage_refresh` or `files/storage_action` RPCs.

Sharing still previews the estimated new storage and payer and identifies existing
copies. Local deletion does not revoke links or remove backups; removing a cloud
copy cannot recall downloaded copies. Link revocation requires confirmation, keeps
its error available for retry, and ignores replies after locking or leaving the page.
Electron smoke covers links, backup navigation, membership usage, and absence of the
removed cloud-space interface in both themes, narrow windows and RTL.

## Credential use

Signatures & approvals → Credential use configures device-local GitHub
issue-create policies using masked field selectors. Changes and optional
Agent grants open the existing approval window. Uses show the exact target,
credential revision and full literal title/body, with high-risk confirmation.
History distinguishes `lost` from success, preserves the current page during
background refresh, and supports manual-review markers and immediate policy
disabling. Policy cards show requesting Agents and MCP setup guidance.
The workflow navigation groups credential use, sharing/import and maintenance;
each page owns its loading, retry and error state. Source selectors search
metadata without retaining hidden values. P1 owner workflows are described below.
`node scripts/secrets-smoke.mjs` exercises the built Electron app and IPC
against a local fixture; it does not test OS biometrics or contact GitHub.
See [validation](../../docs/protocol/secrets_zh.md#验证入口与边界).
File details show complete, copyable file and version IDs and copy
`dmsg-file://<file>/<version>` references; Vault items show their ID and get
value-free field references from `secrets/ref`. `node scripts/references-smoke.mjs`
covers both against a local fixture with an intercepted clipboard; see the
[tutorial](../../docs/dmsg_secrets_tutorial_zh.md).


## Secrets P1 开发工作流

实现进程单次交付、SSH/Git 逐次审批、机密文件挂载、秘密快照分享/导入、受限扫描与轮换记录。公开数据流、owner 接口与限制见 [Secrets 合同](../../docs/protocol/secrets_zh.md)，平台和实际测试边界见 [验证范围](../../docs/protocol/secrets_zh.md#验证入口与边界)。这些 owner 功能不为 MCP 增加明文取密或任意目录访问接口；团队共享保险库仍暂缓。

桌面入口位于“签名与授权 → 凭据使用”；策略、机密文件、SSH/Git、健康与发现、快照、导入及本设备活动使用独立页签，按任务分组。健康结果可筛选并打开来源处理；轮换按实际凭据变化与依赖清单开放步骤；审批以结构化摘要显示范围，原始数据可展开核对。挂载和快照预览在表单变化后失效，迟到结果不能重新启用提交。导入失败保留当前页面内的掩码输入，成功、离页及锁定时清除；导入记录提供结果入口。原生 Apple FSKit 的开发构建和打包见 [native/fskit](native/fskit/README.md)。macOS 不会自动启用文件系统扩展：“机密文件”页提示用户启用 “dMsg secret files”，提供打开系统设置的按钮，启动失败时在映射卡片上显示固定原因对应的说明。

## Mail recovery in SDK 0.12.6

Mailbox management can recover records from the entered relay origins, following
all inventory pages through the daemon. It shows missing decryption keys and
pending publication routes. Recovery restores metadata, not keys or message text;
a keyless mailbox cannot be renewed until its key is restored or rotated. Closing
a keyless mailbox rotates its publication key as required by the connector.
Local inspection continues during background receiving and reports actual key
availability. The recovery messages are present in all six interface languages.
The pure pagination tests and Electron smoke flow exercise recovery and the
keyless mailbox controls.

## Offline display data

The account switcher reads the daemon's encrypted profile snapshot in one call,
including already stored avatar bytes. Locked menus show account IDs only.
Public-name batches have no renderer TTL or persistent browser store; lock and
account/session changes clear the in-memory names and discard late replies.

Membership, account/security, wallet summaries and balances, names, Inbox policy
and cloud inventory read local display snapshots before asynchronous refreshes.
Historical observations show their saved time. Display snapshots do not approve
payments, signing, membership entitlement updates or wallet actions. Ordinary
membership reads are query-only; the existing explicit refresh and the 15-minute
follow-up after this page starts a payment retain their separate update path.
Cache events reread locally, including during payment follow-up, so they cannot
form an entitlement-update loop. The main-process reminder reads only a saved
membership snapshot. Settings → Files exposes cache size and clearing.

A lost daemon connection masks both main and approval windows. A confirmed
same-session reconnect preserves edits; a different daemon instance or unlock
generation remounts the views. `DAEMON_DISCONNECTED` does not enter read recovery.

Validation commands:

- `pnpm --dir src/dmsg_desktop check` and `pnpm --dir src/dmsg_desktop test`
- `pnpm --dir src/dmsg_desktop build`
- `cargo test -p dmsg_cli display_tests`
- `cargo test -p dmsg_cli real_electron_cached_profile_and_reconnection -- --ignored --nocapture`

The last test launches real Electron against the real Rust daemon HTTP/WebSocket
handler with a reopened temporary SQLite workspace, a synthetic memory keystore
and unreachable upstream services. It measures post-unlock cached rendering and
checks same-session form preservation. It does not exercise the user's databases,
Touch ID, production accounts or hardware-keystore latency. Mock-daemon smoke
results and these measured timings are separate evidence.

A session-change event destroys the previous views immediately, including when
the following status query is delayed or fails. Connection-only interruptions
mask portalled dialogs and make the document inert while preserving same-session
edits. Each queued display read carries its own mode, so unrelated completions
cannot turn a cache-event local reread into an entitlement refresh. Visible
windows periodically reread profile/name snapshots; the daemon alone owns TTLs.

## Localization verification (2026-10-28)

All 23 locales include the main window, approval window, tray, notifications,
updates and native dialogs. Wallet and credential workflow copy lives in the
same checked catalogs as other interface text. Existing translations were
preserved. A window reload receives the main process's current language instead
of reverting to its original process argument.

`pnpm check`, `pnpm test` (130 tests), and `pnpm build` passed. With the built
app, `DMSG_DESKTOP_SMOKE=languages node scripts/smoke.mjs` verifies all 23
choices through keyboard navigation, persistence and reloading both windows.
It captures both themes at the minimum main/approval sizes (760×540/400×560).
`DMSG_DESKTOP_SMOKE=settings node scripts/smoke.mjs` covers all nine settings
routes, editor focus, drafts, retry and late replies, including German,
Traditional Chinese, Hindi, French, all eight second-stage languages,
and Arabic/Urdu RTL screenshots.

The translations received terminology and representative screenshot review.
Native-speaker review, Windows/Linux UI runs and production OS identity/file
dialogs were not performed for this change.
