---
name: dMsg desktop
colors:
  light:
    paper: '#F7F8F4'
    paper-dim: '#EEF2EB'
    surface: '#FFFFFF'
    surface-2: '#F3F6F1'
    pale: '#E8F0E9'
    wash: 'rgb(16 37 31 / 5%)'
    line: '#DAE3D9'
    edge: '#84968A'
    edge-soft: '#C9D4CB'
    ink: '#10251F'
    ink-2: '#2B3F36'
    muted: '#4E6257'
    faint: '#637769'
    forest: '#145C45'
    forest-hover: '#0D4433'
    on-forest: '#FFFFFF'
    focus: '#145C45'
    gem: '#B45309'
    gem-pale: '#FBF1E6'
    danger: '#B91C1C'
    danger-pale: '#FDF0F0'
    ok: '#166534'
    ok-pale: '#ECF7EF'
    warn: '#92400E'
    warn-pale: '#FCF4E4'
  dark:
    paper: '#0E1512'
    paper-dim: '#111A16'
    surface: '#151F1B'
    surface-2: '#1A2621'
    pale: '#1E2D26'
    wash: 'rgb(229 238 232 / 6%)'
    line: '#26372F'
    edge: '#5F776A'
    edge-soft: '#34483E'
    ink: '#E5EEE8'
    ink-2: '#CBD8CF'
    muted: '#A3B7AC'
    faint: '#8EA398'
    forest: '#5CC497'
    forest-hover: '#7ED6AE'
    on-forest: '#08201A'
    focus: '#A9D5BD'
    gem: '#F2A65A'
    gem-pale: '#2B2117'
    danger: '#F08A8A'
    danger-pale: '#2C1818'
    ok: '#74D39E'
    ok-pale: '#13261B'
    warn: '#F0B84A'
    warn-pale: '#2A2210'
typography:
  plan-name:
    fontFamily: Inter Variable
    fontSize: 26px
    fontWeight: '600'
  item-title:
    fontFamily: Inter Variable
    fontSize: 17px
    fontWeight: '600'
  dialog-title:
    fontFamily: Inter Variable
    fontSize: 15px
    fontWeight: '600'
  view-title:
    fontFamily: Inter Variable
    fontSize: 14.5px
    fontWeight: '600'
  section-title:
    fontFamily: Inter Variable
    fontSize: 14px
    fontWeight: '600'
  ui:
    fontFamily: Inter Variable
    fontSize: 13.5px
    fontWeight: '400'
    lineHeight: '1.55'
  control:
    fontFamily: Inter Variable
    fontSize: 13px
    fontWeight: '500'
  secondary:
    fontFamily: Inter Variable
    fontSize: 12.5px
    fontWeight: '400'
  meta:
    fontFamily: Inter Variable
    fontSize: 12px
    fontWeight: '400'
  label:
    fontFamily: Inter Variable
    fontSize: 11.5px
    fontWeight: '500'
  technical:
    fontFamily: ui-monospace
    fontSize: 11.5px–12.5px
    fontWeight: '400'
rounded:
  pill: 6px
  control: 8px
  panel: 12px
  section: 16px
  dialog: 16px
  full: 9999px
spacing:
  unit: 4px
  control-inline: 8px–12px
  panel: 16px–24px
  between-groups: 20px–32px
  control-height-xs: 28px
  control-height-sm: 32px
  control-height-md: 32px
  control-height-lg: 40px
  sidebar: 236px
  sidebar-collapsed: 64px
  page-max: 860px
  page-max-wide: 1040px
  main-window: 1180×760 (min 760×540)
  approval-window: 460×720 (min 400×560)
motion:
  spring: '--ease-spring: what pops (notices, a radio dot)'
  spring-soft: '--ease-spring-soft, 200ms: what arrives (the command palette, dialogs)'
  calm: '--ease-calm: what moves (the sidebar collapsing, a collapsible opening)'
---

## Brand & Style

The desktop is the **Gatehouse** (门房) of the dMsg mark: it keeps the vault
and the channels, receives what websites, the terminal and agents ask for,
and lets the person decide what passes. It shares the website's palette,
type and icon family (`src/dmsg_frontend/DESIGN.md`) and adds what an
everyday application needs: density, a dark theme, and states for hover,
selection and overlays.

Three ideas decide the layout:

1. **The gate rail keeps state in view.** Locked or not, the versions
   waiting to sync, background online and the daemon connection sit at the
   foot of the sidebar, always in the same place. The lock has one control:
   its state and the step that opens the device while locked, a Lock button
   once unlocked.
2. **Objects first.** A view is a list beside the object it opens; new
   items, imports and transfers open in dialogs instead of sharing the page
   with the list.
3. **Passing the gate is a ritual.** The approval window reads as a ticket:
   who asks, the way the request came, what is signed or paid, what
   approving does, then the technical fields.

The plan, its diagnosis and its record are in
[archived desktop design record](../../archived/docs/dmsg_desktop_design_zh.md).

### Scope and Source of Truth

This file describes the desktop renderer as built (`src/renderer`). The
tokens above are the values of `src/renderer/app.css`; when they differ, the
code wins and this file is corrected. Website pages keep their own design
file; the website's `/legacy` migration panels use the same controls in the
site's tokens (`src/dmsg_frontend/src/lib/legacy/ui`).

## Rules

- **R1 State before content.** Lock, sync and waiting requests have fixed
  places and are told by text and icon, never by color alone.
- **R2 One object, one place.** Selecting opens the object beside its list;
  flows that create or move something open in a dialog.
- **R3 Identifiers are recognizable, inspectable and copyable.** An ID shows
  as a neutral chip with its first and last four characters; the tooltip
  holds all of it and a click copies it. Equal abbreviations do not prove
  equal IDs. Internal file, version and vault-item IDs belong at the end of
  detail content, after the content and version history, in the shortened
  `Id` presentation. Version rows also use shortened IDs. Hover or keyboard
  focus exposes the full value; copying always uses the complete identifier.
- **R4 Consequence first, details after.** Approvals, grants and transfers
  say in one sentence what will happen; raw fields stay folded below.
- **R5 Risk costs one more step.** Reject has the focus; Approve waits 1.2
  seconds after a request appears or changes, or the window comes back to
  the front; what the daemon rates high-risk (payments, device revocation,
  the signing policy, the recovery delay, binding or removing a login, the
  Agent principal) is approved by holding the button for a second and,
  where the Secure Enclave keeps the fast-unlock key, with Touch ID or the
  login password.
- **R6 Motion only shows cause and effect.** What pops springs, what arrives
  eases in, work under way shimmers. Buttons do not move or blink; one
  whose click is still waiting turns a ring. Everything stops under
  reduced motion.
- **R7 Shared controls are the default.** Views compose the kit below.
  Missing controls are implemented once in the kit, with the same tokens,
  keyboard behavior and states. A browser-default control with an accent
  color is not a finished desktop component.

## Colors

The light theme is the website's: **Paper** ground, **Surface** white for
panels, **Ink** text, **Muted** and **Faint** for secondary and tertiary
text, **Forest** for the primary action, selection and links. The dark theme
keeps the same roles on deep forest neutrals with a lighter Forest
(`#5CC497`) that carries white-on-dark contrast. The person picks system,
light or dark; the choice lives in the profile's `appearance` file and
reaches both windows through `nativeTheme` and `prefers-color-scheme`. Write
colors as the tokens (`bg-surface`, `text-muted`, `border-line`), never as
literal hex values.

- **Line** separates content; **Edge** (3:1 on white) is the boundary of
  inputs and other controls that need one; **Edge-soft** outlines secondary
  buttons, which their label already identifies.
- **Gem** amber means "waiting for you": the waiting count, the request's
  kind chip, the diamond. It is not decoration.
- **Pale** marks the selected row and filter; **Wash** is hover.
- State colors come with text and an icon. **Ok** means a named step
  completed (verified, held by the relay), **Warn** an unresolved condition
  (waiting to sync, conflict, rotation needed), **Danger** a failure or a
  destructive consequence. Reject and Cancel stay neutral.

## Typography

Inter Variable is bundled (`@fontsource-variable/inter`), with PingFang SC and
Microsoft YaHei as CJK fallbacks; `ui-monospace` carries IDs, hashes,
capabilities, packets and code. The body is 13.5px at 1.55; reading content
(notes, messages, statements to sign) stays 13.5–14px at 1.6. Titles step
from 14px (sections) to 15px (dialogs) and 17px (an open item); large numbers
(the plan, an allowance) may reach 22–26px. Metadata is 11.5–12px in Faint.

Numbers and times use tabular figures. Lists show relative times ("2 min
ago") with the full time in a tooltip. A translated label never sits in the
monospace face: Arabic loses its letter joining there, so only the value
(an ID, a revision) is monospaced.

## Layout & Spacing

- **Main window**: sidebar (236px, 64px collapsed) and a title bar (60px)
  over the view. The sidebar's top is the title bar's line, with the
  window's buttons: the sidebar toggle, back and forward, 32px each. On
  macOS the window has no title bar of its own: the traffic lights sit over
  the sidebar's vibrancy, centred on that line, and the buttons follow them
  (82px from the edge; 18px in full screen, which hides the lights). On the
  other systems the mark leads and the buttons end the line. Collapsed, the
  title bar reaches over the rail's top and takes the buttons, on macOS at
  the same place. In right-to-left text the lights stay at the top left,
  over the title bar's end, which leaves them 84px. The title bar is the
  drag region.
- **Pane views** (vault, files, channels, inbox) run edge to edge: a filter
  or list column (248–320px), the open object, and for channels a members
  drawer (256–280px); the file library adds a folder tree (220px) before
  its list, and its cloud and trash places take the list's and the object's
  room as one page. Container queries narrow them: the vault's filter rail turns
  into a menu below 860px; the file library’s places move into a dialog below
  800px, leaving room for its list and detail; the members drawer floats over the timeline below
  860px, the settings section list turns into a row of buttons below 720px.
- **Page views** (agents, signatures, plan, settings) scroll as one column of
  sections, 860px wide (1040px for the capability matrix), with tab bars or
  a section list for their parts.
- Spacing has a 4px base: 8–12px inside controls, 16–24px inside panels,
  32px between page sections. Text-button heights are 28, 32 and 40px. Icon tools are 32px squares; panel close buttons are 36px squares.

## Elevation & Shapes

Order of separation: spacing, a rule, a tint, a border, a shadow. Only
floating layers (menus, tooltips, the palette, dialogs, notices, the import
card) have the `pop` shadow. Radii: 6px pills and chips, 8px controls and
list rows, 12px panels, 16px sections and dialogs; avatars are rounded
squares, profile pictures are circles.

## Icons

One family, Remix Icon 4.6.0 (Apache-2.0, `public/licenses`), line style,
inlined in `components/glyphs.ts` so they take the text color. Names say what
an icon is for (`vault`, `conflict`, `epoch`), with the Remix name beside
each path. 18px in navigation and toolbars, 16–18px beside button text, and 20px in large buttons and section titles. Status marks remain 12–14px; empty states use 24px without a decorative frame. Panel close buttons use a 20px SVG with a tighter viewBox, giving the existing Remix cross about 14px of visible width inside a 36px hit area. Icons that point along the reading direction (`chevron-end`, `enter`,
the sidebar toggles) mirror right to left; the send icon is flipped where it
is used. Every icon-only button has an accessible name and a tooltip.

## Motion

Three curves, the springs from anda-bot's extension: `--ease-spring` for
what pops (notices, a radio dot), `--ease-spring-soft` for what arrives (the
command palette, dialogs), `--ease-calm` for what moves (the sidebar
collapsing, a collapsible opening). Menus and tooltips fade and zoom with
tw-animate-css. Work under way uses the text `shimmer` or
the `progress-run` band; time running out uses the `drain` bar (an armed or
shown secret) and the approve delay fills inside the button. Under
`prefers-reduced-motion` every animation and transition ends at once, and the
drain bars hide.

## Components

The kit lives in `src/renderer/components`; views compose it and do not
restyle a control by hand.

- **Buttons** — `buttonClass(variant, size)` and `iconButtonClass` in `ui.ts`: `primary` (one per
  area), `secondary`, `ghost` (toolbars, icon buttons), `danger` (a
  destructive step, outlined with Danger text). A button its click disabled
  turns a ring over its label while the bridge calls that click started run
  (`pending.ts`, after 150 ms); views only disable it by their `busy` flag.
- **Menus** — bits-ui DropdownMenu and ContextMenu with `menuContentClass`
  and `menuItemClass`; destructive items last, after a separator, in Danger.
  `Select` is a dropdown that checks the chosen option. `SearchSelect` is a
  searchable, closed set of choices, with labels, hints and disabled options.
- **Choices** — `Checkbox`, `Radio`, `Segmented` (one of a short ordered
  set), radio cards for types and modes, `Suggest` for free text with
  suggestions, `TagInput`, `SecretInput` (hidden while typed, with a
  generator for logins).
- **Dialogs** — `Dialog`: icon tile, title, a line of description, scrolling
  body, footer with the decision on the end side. Escape cancels. The
  new-item, edit and letter forms submit with ⌘↵.
- **Pages** — `components/page`: `Page`, `Section` (unframed title, description,
  actions, flush lists), `Notice` (info, warn, danger, ok), `Pill` (states),
  `Id` (the neutral ID chip), `Field`, `Fact` (label and value rows),
  `TabBar`.
- **Feedback** — `toast()` notices, above composers and footers; a copied
  secret's notice counts down to the clipboard being cleared. Errors that
  belong to a form stay in the form.
- **Avatars** — initials (or an ID's first two characters) on a tone that
  stays with the ID; a tone helps recognition and never means verified.

### Form control contract

Use semantic HTML inside the design system. Ordinary text inputs and
textareas already get their border, height, background, focus and disabled
styles from `app.css`; they are not unstyled just because a view contains
an `<input>`. They still need a consistent field layout and visual review.
`Field` supplies the visible label and optional help; connect help/errors
with `aria-describedby` and invalid state with `aria-invalid`. A placeholder
is an example, not the only label. For a compound control, explicitly name
its interactive element; do not nest labels or put several unrelated
controls inside one label.

| Need | Required implementation |
| --- | --- |
| Short fixed choice | `Select`; use `Segmented` for a small set worth keeping visible |
| Long list of items, files, agents or channels | `SearchSelect`, or an existing domain picker; search labels and safe metadata, show unavailable reasons |
| Free text with suggestions / multiple tags | `Suggest` / `TagInput` |
| Boolean / one-of-many choice | `Checkbox` / `Radio` within `RadioGroup.Root` |
| Text or multiline input | `TextField` / `TextArea`, with linked labels, hints and errors; numeric text uses `inputmode`, amounts use the wallet’s amount field |
| Typed password, token, private key or secret-bearing link | `SecretInput`; preserve multiline values, masking and existing clipboard boundaries |
| Standard action / icon tool | `buttonClass` / `iconButtonClass`; icon-only controls need a name and tooltip |
| Confirmation / feedback | `Dialog` / `Notice` and `toast`; explain the consequence and retain the approval-window boundary |
| Date / month / local date-time | `DateInput`: typed calendar value, themed calendar/month popover, validation and explicit local timezone for time |
| Bounded numeric choice / progress | `Slider` / `Progress`, including keyboard controls, accessible values and unknown progress |
| OS file/directory choice or system identity confirmation | Existing main-process bridge; preserve the platform/security dialog |

Do not introduce native `select`, `datalist`, checkbox/radio/range inputs,
browser date/time/color pickers or `window.alert/confirm/prompt` directly in
a view. Do not replace them with hand-written clickable divs. A new shared
control must preserve accessible semantics and keyboard navigation, using
bits-ui where suitable. Native internals are acceptable only when their
visible states are deliberately themed and verified; a wrapper alone or
`accent-color` alone does not meet this requirement. New input types and
dynamic types require inspection of every reachable type.

Extend shared components instead of overriding their borders, heights,
focus rings or colors in a view. Width, alignment and content density may
adapt to the task. Match control rows at 32px by default; bound and scroll
long menus, wrap hints and errors, and keep labels readable in narrow and
RTL layouts. Inspect hover, focus, selection, disabled, loading, error and
empty states in both themes. Reusing a component does not prove that the
resulting layout is polished.

This contract covers the main view, settings and all its child views,
dialogs, popovers and the independent approval window. The original native-control debt is resolved; see the dated
[remediation and validation record](docs/native-controls-audit.md).
The repository test now permits no view-level exceptions. The dynamic
input inside `TextField` is bounded to the five reviewed text types; this
component implementation is separately recognized by the checker.
When implementing a themed native primitive inside a new shared component,
add a specific checker rule and interaction coverage for that implementation;
do not add new view-level uses to the historical debt baseline.

### Shared form behavior

`TextField` and `TextArea` associate a persistent label with the input ID,
link hints and errors through `aria-describedby`, and mark invalid fields
with `aria-invalid`. Readonly values retain selection and use the secondary
surface. Keep copy actions separate from editable input, and let failure
leave the draft available for retry. Use `hideLabel` only when the same
field already has a persistent visible label, as in the vault field editor.

`DateInput` preserves `YYYY-MM-DD` and `YYYY-MM` calendar values. Local date
and time display a space between the parts and serialize with `T`; the
hint names the device timezone. Reject rollover dates, invalid times and
caller-provided maximums. Callers must also gate submission/export: a red
field alone must not let JavaScript normalize an invalid date into an action.
Its popup and the password generator fit the available viewport height and
scroll internally. Escape dismisses the topmost popup before its dialog;
inline editors alone handle their own cancel shortcut.

### Patterns

- **Secret field** — first click arms Show or Copy for three seconds with a
  draining bar; the second click acts. A shown secret hides after a minute.
- **Conflict** — the current version and the copy side by side, changed
  fields tinted, the body diffed line by line; both choices weigh the same.
- **Adding files** — a job card per file with its chunks done, above the
  library's list; a card for a backup or a fetch the same way. Done when
  the daemon has it encrypted; a backup is its own, explicit step with a
  confirmation that names what goes and the cloud's room.
- **Status of a file** — one icon with a tooltip, in the row and as a pill
  in the detail: on this device, backing up, backed up, only in the cloud,
  over the cloud's limit, failed. Never by color alone.
- **Message states** — sending (shimmer) until the relay stores it, then a
  check with its sequence number; refused messages stay with Retry and
  Discard.
- **Plan and names** — Membership, Names and Payments tabs; each section
  loads on its own and shows the locked state when it needs the device
  unlocked. A plan card says what buying it does now (subscribe, renew from
  the end date, upgrade for the difference, switch at the end) or why not
  and when. An order is a pill for its state with only the actions that
  state allows. Moving a name is a dialog that gives the token to copy, a
  `dmsg://handle/accept#…` link or a direct conversation; the receiver
  accepts in its own dialog and can tell the owner in a conversation, and
  the owner completes within ten minutes.
- **Approval ticket** — kind chip with the gem diamond, the requester, the
  trail (relayed hops dashed), the content, "when you approve", folded
  technical fields, then the fixed decision area: expiry line, Reject, Approve or Hold to
  approve, and after a decision the steps the daemon reports.
- **Empty and locked states** — an icon tile, one sentence, and the next
  action (log in, unlock, continue setup, new item, import).
- **Login** — one dialog wherever it starts: three numbered steps (open
  the page, log in in the browser, finish on this device) with the current
  one shimmering, the page's expiry, Open the page again and Copy link; a
  failure names its cause in a Danger notice with the daemon's message in
  mono and Retry. The gate rail's next step is a Primary button while
  logged out.

## Interaction Integrity

- Only the approval window decides; the main window can only open a request.
- A secret reaches the screen only on the second click and reaches the
  clipboard only through the main process.
- Files enter through the main process's dialogs or a drop the preload
  resolves to a path; a file a page made has none and is refused. Nothing
  of the library leaves the device unless the person backs it up or shares
  it, and deleting on one side never deletes on the other. A payer's PEM
  key enters only through the main process's dialog; a window never names
  its path.
- The idle lock follows a person's input, which the main process reports;
  what windows read on events never keeps the device unlocked.
- Never present a pending or unknown result as success.
- A preview belongs to the exact input that produced it. Editing an input
  invalidates that preview, including responses still in flight. A response
  from a closed view must not update the next view.
- Refresh and retry belong to their section; one failed read does not hide
  independent results. Keep retryable drafts on failure and give errors a
  cause and next action in the interface language.

## Accessibility & Languages

- Six catalogs (`i18n/messages`), checked against English; values avoid the
  ASCII apostrophe (French uses ’).
- Arabic lays the windows out right to left with logical properties
  (`ms-`/`me-`/`ps-`/`pe-`/`start-`/`end-`); IDs, code and numbers stay in
  `<bdi dir="ltr">` or the `font-mono` isolation in `app.css`.
- Focus is always visible (`focus-visible` rings in the focus color); the
  vault list moves with the arrow keys; dialogs and menus trap and return
  focus.
- Keep body text at 4.5:1 and control boundaries and large text at 3:1 in
  both themes when adding a token or a tint.

## Checking a Change

`pnpm check`, `pnpm test` (pure rules: approval, vault, channels, agents,
plan, policy, languages) and `pnpm test:e2e`, which runs the built app against a
mock daemon and saves screenshots in `test-results/` (light, dark, French
and Arabic). Look at the screenshots of the views a change touches.

`pnpm audit:controls` inventories native markup and direct browser-dialog
calls across the renderer; `pnpm audit:controls --json` includes ordinary
theme-styled text inputs too. `pnpm test` includes `ui-controls.test.ts`,
which rejects disallowed occurrences and direct settings inputs. The
scanner uses the Svelte/TypeScript AST, so comments and documentation
examples do not count. It cannot prove visual quality, accessible names,
runtime-generated controls or the behavior of third-party internals.

For changed forms, check keyboard-only entry, menu selection, Escape and
focus return; opening/closing while reads are pending; invalid inputs and
retry; empty and long lists; Chinese, long translated labels and Arabic
RTL; light/dark themes and the affected window's minimum supported size
(main 760×540, approval 400×560). Build before running the appropriate
Electron smoke script; credential workflows also have
`node scripts/secrets-smoke.mjs`. Use isolated fixtures for UI checks, and
report real-device, OS-dialog or accessibility checks separately if they
were not run. Documentation-only changes do not require a new visual run.


## 2026-10-24 UI simplification

Page sections use headings, aligned content and 32px spacing, without a
background card, enclosing border or static shadow. File and vault detail
fields are continuous groups. Lists keep useful row separators; panes keep
one boundary between independent scrolling areas. Inputs, choices, focus
rings, risk notices and floating layers retain their visible boundaries.
Decorative icon frames and empty-state dashed boxes are removed. The gate
rail remains fixed, with sync, connection and lock state always visible.

Icon-only tools use 32px square targets with 18px glyphs. Dialog, drawer and
history closes use 36px targets with 20px glyphs; the close path uses a tighter
18-unit viewBox. Hover and keyboard focus remain visible. A dialog doing a
wallet estimate/submission cannot be dismissed until that call settles;
locking or changing accounts still closes it and clears the sensitive view.

The wallet overview shows compact metrics, current proposals and an asset /
address summary. Transfer opens a separate dialog; closing it keeps the draft
for that wallet while the view remains mounted, and reopening requires a new
estimate. Memo is in Advanced; fees and the committed preview stay visible.
Agent ID rows keep pause, profile, delegation and credential withdrawal
available; maintenance and removal live in More actions. Membership combines
a compact term summary with usage instead of repeating a tall limits table.

Vault list summaries use the item type for notes and sensitive item kinds.
Login-like items may show a visible username or a web host, never arbitrary
field content, notes, URL passwords, query strings or fragments. Full notes
remain available in the selected item's detail.

`DMSG_DESKTOP_VISUAL_MATRIX=1 node scripts/smoke.mjs` additionally captures the
wallet and transfer dialog at 760, 1180 and 1600px, across all six languages
and both themes in representative combinations. It checks horizontal fit;
visual inspection is still needed for typography, focus and grouping.
