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
  control-height-xs: 24px
  control-height-sm: 28px
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

1. **The gate rail keeps state in view.** Locked or not, the idle lock's
   countdown, the versions waiting to sync, background online and the daemon
   connection sit at the foot of the sidebar, always in the same place.
2. **Objects first.** A view is a list beside the object it opens; new
   items, imports and transfers open in dialogs instead of sharing the page
   with the list.
3. **Passing the gate is a ritual.** The approval window reads as a ticket:
   who asks, the way the request came, what is signed or paid, what
   approving does, then the technical fields.

The plan, its diagnosis and its record are in
[docs/dmsg_desktop_design_zh.md](../../docs/dmsg_desktop_design_zh.md).

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
  equal IDs.
- **R4 Consequence first, details after.** Approvals, grants and transfers
  say in one sentence what will happen; raw fields stay folded below.
- **R5 Risk costs one more step.** Reject has the focus; Approve waits 1.2
  seconds after a request appears or changes; payments, device revocation,
  the signing policy, the recovery delay and binding or removing a login
  are approved by holding the button for a second.
- **R6 Motion only shows cause and effect.** What pops springs, what arrives
  eases in, work under way shimmers. Buttons do not move or blink, and
  everything stops under reduced motion.

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
  over the view. On macOS the window has no title bar of its own: the
  traffic lights sit over the sidebar's vibrancy, and the title bar is the
  drag region.
- **Pane views** (vault, files, channels, inbox) run edge to edge: a filter
  or list column (248–320px), the open object, and for channels a members
  drawer (256–280px); the file library adds a folder tree (220px) before
  its list, and its cloud and trash places take the list's and the object's
  room as one page. Container queries narrow them: the vault's filter rail turns
  into a menu below 860px, the members drawer floats over the timeline below
  860px, the settings section list turns into a row of buttons below 720px.
- **Page views** (agents, signatures, plan, settings) scroll as one column of
  sections, 860px wide (1040px for the capability matrix), with tab bars or
  a section list for their parts.
- Spacing has a 4px base: 8–12px inside controls, 16–24px inside panels,
  20px between sections. Control heights are 24, 28, 32 and 40px.

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
each path. 16px in navigation and controls, 20–24px for empty states and
tiles. Icons that point along the reading direction (`chevron-end`, `enter`,
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

- **Buttons** — `buttonClass(variant, size)` in `ui.ts`: `primary` (one per
  area), `secondary`, `ghost` (toolbars, icon buttons), `danger` (a
  destructive step, outlined with Danger text).
- **Menus** — bits-ui DropdownMenu and ContextMenu with `menuContentClass`
  and `menuItemClass`; destructive items last, after a separator, in Danger.
  `Select` is a dropdown that checks the chosen option.
- **Choices** — `Checkbox`, `Radio`, `Segmented` (one of a short ordered
  set), radio cards for types and modes, `Suggest` for free text with
  suggestions, `TagInput`, `SecretInput` (hidden while typed, with a
  generator for logins).
- **Dialogs** — `Dialog`: icon tile, title, a line of description, scrolling
  body, footer with the decision on the end side. Escape cancels. The
  new-item, edit and letter forms submit with ⌘↵.
- **Pages** — `components/page`: `Page`, `Section` (title, description,
  actions, flush lists), `Notice` (info, warn, danger, ok), `Pill` (states),
  `Id` (the neutral ID chip), `Field`, `Fact` (label and value rows),
  `TabBar`.
- **Feedback** — `toast()` notices, above composers and footers; a copied
  secret's notice counts down to the clipboard being cleared. Errors that
  belong to a form stay in the form.
- **Avatars** — initials (or an ID's first two characters) on a tone that
  stays with the ID; a tone helps recognition and never means verified.

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
- **Approval ticket** — kind chip with the gem diamond, the requester, the
  trail (relayed hops dashed), the content, "when you approve", folded
  technical fields, then the stub: expiry line, Reject, Approve or Hold to
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
  it, and deleting on one side never deletes on the other.
- The idle lock follows a person's input, which the main process reports;
  what windows read, on events or the status refresh (every 10 seconds
  while unlocked, for the countdown), never keeps the device unlocked.
- Never present a pending or unknown result as success.

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
policy, languages) and `pnpm test:e2e`, which runs the built app against a
mock daemon and saves screenshots in `test-results/` (light, dark, French
and Arabic). Look at the screenshots of the views a change touches.
