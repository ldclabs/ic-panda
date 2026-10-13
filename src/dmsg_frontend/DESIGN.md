---
name: dMsg
colors:
  surface: '#F7F8F4'
  surface-dim: '#EDF1EA'
  surface-bright: '#FFFFFF'
  surface-container-lowest: '#FFFFFF'
  surface-container-low: '#F7F8F4'
  surface-container: '#F0F4EE'
  surface-container-high: '#E8F0E9'
  surface-container-highest: '#DAE3D9'
  on-surface: '#10251F'
  on-surface-variant: '#4E6257'
  inverse-surface: '#10251F'
  inverse-on-surface: '#F7F8F4'
  inverse-on-surface-variant: '#C6D5CA'
  outline: '#607566'
  outline-variant: '#DAE3D9'
  inverse-outline: '#607566'
  inverse-outline-variant: '#354D40'
  surface-tint: '#145C45'
  primary: '#145C45'
  on-primary: '#FFFFFF'
  primary-hover: '#0D4433'
  primary-container: '#E8F0E9'
  on-primary-container: '#0D4433'
  inverse-primary: '#A9D5BD'
  secondary: '#10251F'
  on-secondary: '#FFFFFF'
  secondary-container: '#EDF1EA'
  on-secondary-container: '#4E6257'
  tertiary: '#B8CDBD'
  on-tertiary: '#10251F'
  tertiary-container: '#F0F5F0'
  on-tertiary-container: '#3F5C4C'
  brand-forest: '#145C45'
  brand-jade: '#2D7960'
  error: '#B91C1C'
  on-error: '#FFFFFF'
  error-container: '#FEF2F2'
  on-error-container: '#991B1B'
  success: '#166534'
  on-success: '#FFFFFF'
  success-container: '#F0FDF4'
  on-success-container: '#166534'
  warning: '#92400E'
  on-warning: '#FFFFFF'
  warning-container: '#FFFBEB'
  on-warning-container: '#92400E'
  focus: '#145C45'
  inverse-focus: '#A9D5BD'
  background: '#F7F8F4'
  on-background: '#10251F'
  surface-variant: '#EDF1EA'
typography:
  display-hero:
    fontFamily: Inter
    fontSize: 116px
    fontWeight: '650'
    lineHeight: '0.98'
    letterSpacing: -0.066em
  display-lg:
    fontFamily: Inter
    fontSize: 60px
    fontWeight: '650'
    lineHeight: '1.16'
    letterSpacing: -0.045em
  headline-md:
    fontFamily: Inter
    fontSize: 46px
    fontWeight: '650'
    lineHeight: '1.16'
    letterSpacing: -0.045em
  title-lg:
    fontFamily: Inter
    fontSize: 32px
    fontWeight: '650'
    lineHeight: '1.2'
    letterSpacing: -0.035em
  title-md:
    fontFamily: Inter
    fontSize: 23px
    fontWeight: '650'
    lineHeight: '1.25'
    letterSpacing: -0.025em
  title-sm:
    fontFamily: Inter
    fontSize: 18px
    fontWeight: '600'
    lineHeight: '1.4'
    letterSpacing: -0.02em
  body-lg:
    fontFamily: Inter
    fontSize: 19px
    fontWeight: '400'
    lineHeight: '1.6'
    letterSpacing: '0'
  body-base:
    fontFamily: Inter
    fontSize: 16px
    fontWeight: '400'
    lineHeight: '1.65'
    letterSpacing: '0'
  body-sm:
    fontFamily: Inter
    fontSize: 14px
    fontWeight: '400'
    lineHeight: '1.6'
    letterSpacing: '0'
  caption:
    fontFamily: Inter
    fontSize: 13px
    fontWeight: '400'
    lineHeight: '1.6'
    letterSpacing: '0'
  label-caps:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: '650'
    lineHeight: '1.6'
    letterSpacing: 0.14em
  button:
    fontFamily: Inter
    fontSize: 15px
    fontWeight: '550'
    lineHeight: '1.4'
    letterSpacing: '0'
  technical:
    fontFamily: ui-monospace
    fontSize: 14px
    fontWeight: '400'
    lineHeight: '1.6'
    letterSpacing: '0'
  kicker:
    fontFamily: ui-monospace
    fontSize: 12px
    fontWeight: '500'
    lineHeight: '1.5'
    letterSpacing: 0.08em
rounded:
  sm: 4px
  DEFAULT: 8px
  md: 12px
  lg: 16px
  xl: 20px
  full: 9999px
spacing:
  unit: 4px
  xs: 4px
  sm: 8px
  md: 16px
  lg: 24px
  xl: 32px
  section-y-mobile: 56px
  section-y-tablet: 72px
  section-y-desktop: 88px
  gutter: 24px
  margin-mobile: 24px
  margin-tablet: 32px
  margin-desktop: 48px
  max-content: 1280px
---

## Brand & Style

**Your space. Your say.** / **你的空间，你来决定。**

dMsg is a private workspace and identity signing service for Web3. One local
program, `dmsg`, runs a daemon on the person's computer that holds the keys and
the approval queue; the desktop app and the command line are two ways into it,
AI agents reach it through MCP with capabilities the owner grants, and websites
ask through `connect.dmsg.net`. It helps people keep secrets, exchange encrypted
content with chosen people, and approve specific signatures. A public identity
and contact rules provide a doorway into that work without making the workspace
public. There is no browser extension.

The visual language is **quiet editorial clarity**: large, deliberate headlines;
warm, light surfaces; forest green actions; precise product examples; and enough space
to read before deciding. The product should feel like a place where work can be
kept private and permissions can be understood. Explain the task first, then the
mechanism and its boundaries.

The main audience is independent developers, project teams, maintainers, and
contributors. The voice is clear, restrained, professional, and respectful.
ICPanda DAO belongs in attribution, governance context, and the footer. Token
prices, mining rewards, panda photography, and trading motifs do not lead the
new experience.

### Scope and Source of Truth

This file defines the target design system for `dmsg_frontend`. The current
forest-green palette supersedes the earlier purple brand proposal; preserve
the selected editorial layout and interaction model when applying it. Its structure
follows the 1Pay.ing design document; dMsg retains its own palette, typography,
metaphor, and interaction rules.

The visual reference is the landing page itself
(`src/lib/components/landing/`), checked as recorded in
[LANDING_QA.md](LANDING_QA.md). Its figures are simulations that tell the
product story; they are not evidence that the demonstrated actions ran.

The tokens above consolidate the reference into reusable roles. Some are
intentional implementation refinements: stronger focus and control boundaries,
44px touch targets, a consistent radius/spacing scale, and explicit feedback
colors. They are not claims that every existing component already uses them.
The landing page adds a technical-drawing register on top of this editorial
base: numbered monospace kickers, figures framed with crop marks and captioned
“Fig. n”, a faint paper grain, and a blueprint grid on the Ink band.

The existing green theme in `src/app.css`, panda assets, and legacy routes are
compatibility implementation, not the new visual baseline. Scope style migration
to the screen being changed. Do not globally recolor legacy flows or replace
working authentication merely to make a new landing page match this file.

## Colors

**Forest Ink** (`#10251F`) carries headings and dark sections. **Paper**
(`#F7F8F4`) is a warm, nearly white page ground; **White** is a content or
confirmation surface. **Moss Slate** (`#4E6257`) carries secondary prose. These
neutrals should occupy most of the page, leaving green room to be distinctive.

**Forest Green** (`#145C45`) identifies the brand, primary action, selected
control, and light-surface links. It is a deep ink green rather than a bright
trading-terminal green. Hover deepens to `#0D4433`. **Jade** (`#2D7960`) is a
supporting brand color; **Sage** (`#B8CDBD`) is reserved for quiet secondary
accents. Use solid fills, including the current brand symbol. Do not bring back
purple gradients or replace them with luminous green gradients.

Use literal pale surfaces rather than layering arbitrary transparent colors:
`#E8F0E9` for the workspace showcase or selected group, `#F0F4EE` for small
supporting surfaces, and `#EDF1EA` for a neutral explanatory band. **Line**
(`#DAE3D9`) separates content; it is not a sufficiently strong sole boundary for
an input. Use `outline` for controls that require a visible edge.

Dark bands use Forest Ink with Paper headings, `#C6D5CA` body copy, and
`#A9D5BD` links/focus. Their separators use `#354D40`; interactive edges use
`inverse-outline`. A dark marketing band does not establish a complete dark
application theme. Validate each component before adding a theme toggle.

### State Colors and Contrast

Success green means a named operation completed, such as a completed download
check. Because green is also the brand color, the state label and icon must
distinguish completion from an ordinary selected or actionable control. It does not mean an entire request, person, or project is safe. Warning
amber indicates a specific unresolved condition; error red indicates a failure
or destructive consequence. Ordinary **Reject** and **Cancel** remain neutral
choices, not error states.

Use state text and an icon alongside color. Distinguish pending, failed,
unknown, and completed; never turn a network timeout into success or rejection.
Body text requires at least 4.5:1 contrast; large text and essential graphical
boundaries require at least 3:1. Check the rendered combinations, including
focus, hover, and tint surfaces.

White on Forest Green passes ordinary-text contrast. Use Forest Ink on pale
Sage, and dark green text on pale state surfaces. Light jade is intended for
Ink backgrounds, not white ones. Do not lower opacity on important permission,
fee, or recovery explanations.

## Typography

**Inter** carries the wordmark, narrative, navigation, buttons, and product UI.
Use the self-hosted variable font for the 550/650 weights in the token map.
Use this fallback stack:

```css
font-family: Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI',
  'PingFang SC', 'Microsoft YaHei', sans-serif;
```

**System monospace** carries file hashes, addresses, exact identifiers,
technical payloads, commands, and the landing page's kickers, figure captions
and diagram labels. Amounts use tabular numerals and retain their relevant
precision. A short address preview must have a way to inspect and copy the full
value; matching abbreviations are not proof that two addresses are identical.

The landing hero is intentionally oversized: desktop
`clamp(62px, 8.6vw, 116px)`, 650 weight, 0.98 line height, and tight tracking,
with the closing full stops in Forest Green. Use the two-line “Your space. /
Your say.” composition. At the mobile breakpoint, use `clamp(54px, 15vw, 80px)`
and check the complete heading at 320px. French, Spanish, Russian and Chinese
have their own smaller scales; this exception belongs to short marketing
headlines, not dialogs, app navigation, or safety explanations.

Landing section headings scale from 38px to 66px as a two-line statement whose
second line is softer (`#6B8A78` on Paper, Jade-mint on Ink); policy pages keep
32–46px, and product headings use 18–32px. Body copy uses 16px, with an 18px
lead and 14px supporting text. Limit long prose to roughly 60–65 characters per
line. Kickers use the 12px monospace token with 0.08em tracking and a boxed
section number (“01”); they orient the page rather than carry essential
information. Small demo labels never carry essential information.

Chinese uses the system CJK fallback, ordinary tracking, and a separately
checked headline scale. Do not force English line breaks or negative tracking
onto Chinese. Arabic is laid out right to left with logical spacing and LTR
islands for hashes, addresses, commands and diagrams; Arabic copy never uses
monospace, letter spacing or capitals, because they break letter joining.

## Layout & Spacing

A marketing page uses a centered 1280px maximum wrapper **including** horizontal
padding: 48px on desktop, 32px on tablet, 24px on mobile. Large screens may use a
1376px wrapper without increasing prose measure. Use border-box sizing.

Use a 4px spacing base. Favor 8/12px within controls, 16/24px within panels,
32/48px between groups, and 72/88/120px vertical landing section padding across
mobile/tablet/desktop (56/72/88px on policy pages). Large editorial two-column sections may use 80px between
columns. Let grouping and reading order determine spacing rather than adding a
border to every object.

The landing rhythm is:

1. Editorial hero: headline on the left, explanation and CTA on the right, then
   Fig. 1 — the gate: requests from the desktop app, the terminal, AI agents and
   websites meet the local daemon; everyday work leaves as ciphertext, and a
   signature reaches ICP only after the visitor approves it.
2. 01 Desktop & command line: Fig. 2 links a desktop window and a terminal
   through one simulated daemon, followed by three practical notes.
3. 02 AI agents: three verifiable figures, the MCP connect command, Fig. 3 (grant
   capabilities, run an agent, read the audit record) and the six protocols.
4. 03 Websites: Fig. 4, the dmsg-connect/1 sequence ending in the approval
   window, followed by four trust notes.
5. 04 Privacy (Ink band): Fig. 5 shows what the device, the cloud and ICP each
   hold for a message the visitor types, then the three boundaries.
6. 05 Trust evidence: public code, the audit record, recovery, and limitations.
7. 06 Release status with platforms and install commands marked as future, then
   07 FAQ, the closing action, and a restrained footer.

Do not compress all capabilities into the first fold. The page should establish
what dMsg does before asking the reader to interpret its infrastructure.

At approximately 1180px, the agent figure moves its audit record below. At
1000px, collapse navigation and two-column headers. Below 900px the desktop
window and terminal stack, and below 720px the gate figure turns to portrait
with sources on top. At 760px and 440px stack the remaining grids. These
are content-driven breakpoints, not device detection. Check 320px, 390px,
tablet widths, and a 1440px desktop; no horizontal overflow may hide controls.

The desktop app's full workspace can use a compact sidebar and wider data areas.
Its independent approval window must fit its own viewport. The website remains a lightweight
surface for discovery, public verification, and guidance; a narrow website is
not a promise of a full mobile vault or messenger.

## Elevation & Depth

Use depth sparingly. The priority order is **spacing and alignment → simple
rules → surface tint → border → shadow**. Keep ordinary feature explanations
on the base page surface. Use a single contained panel when the content is an
actual product object or interactive example.

Two elevation treatments have distinct jobs:

- **Product preview:** a 1px line plus `0 18px 44px -22px rgba(16, 37, 31, 0.3)`,
  for the white working figures on the page surface.
- **Dialog:** `0 24px 80px rgba(16, 37, 31, 0.20)`, above a darkened backdrop.

The sticky header uses an almost opaque Paper surface and a bottom rule. Backdrop
blur is optional for dialogs and never necessary for legibility. Do not add
card hover lifts, large glow fields, nested floating panels, or shadows to
ordinary list rows.

## Shapes & Assets

Buttons and inputs use 8px corners; product panels use 12px; dialogs and larger
showcase surfaces use 16px; 20px is reserved for a deliberately larger container.
Use 4px for small labels. Full rounding belongs to avatars and compact status
markers, not every CTA.

The brand symbol is the **Private Gate**: a stable D-shaped outer boundary and
an open internal negative space. Use an approved supplied asset. The current
asset is a raster concept, not a final vector master; do not trace it into a new
“official” logo or infer exact geometry from it. The plain D outline in the
landing figures and closing section is a diagram of the local daemon that
echoes the outer boundary; it never stands beside the wordmark as a logo.

Spell the name **dMsg**. Do not use “DMSG” as the marketing wordmark or combine
it with the historical “ICPanda Message” title in the hero. The wordmark uses
Inter 800 at approximately 26–29px beside a 34–40px symbol box;
maintain optical spacing and check the visible mark within that box.

Use one icon family, **Remix Icon**. The landing page inlines its paths
(`glyphs.ts`) so icons take the text colour; arrow icons mirror in RTL. Use
20px icons inside controls and a 24px grid for standalone symbols. Keep vault,
channel, identity, file, and signing icons distinct. Every icon-only action
needs an accessible name and an adequate hit target.

Real product examples are HTML components with selectable text and functioning
controls. Do not replace the interface with a screenshot. Use source assets for
logos and imagery; do not approximate brand art with emoji, generic shields,
CSS drawings, or mismatched icon families. Initial-based avatars are acceptable
fallback UI and must not imply identity verification.

## Motion

Motion supports **inspect → decide → see the result**. Controls use 120–200ms
color/border changes; dialog transitions, if any, use 180–240ms. Avoid scaling
or shifting an approval button beneath the pointer. Pending operations use a
clear status and retain the context the user reviewed.

No automatic signing animation, fake encryption progress, confetti, pulsing
permission prompts, or promotional urgency belongs in a sensitive flow. A
success state must follow the corresponding real result. Simulation must be
clearly identified as simulation.

On the landing page, rich motion is allowed inside working figures, which are
captioned as simulations: packets travel through the gate, ciphertext scrambles
as it leaves, a terminal types, a sealed envelope rides a sequence diagram.
Nothing in a figure approves on its own: a request waits for the visitor and
expires if ignored, and buttons never move or pulse. Headlines rise once on
load; sections fade up once as they enter. Loops run only while their figure is
on screen and the tab is visible. Elsewhere, motion stays at hover-level
details.

Honor `prefers-reduced-motion: reduce`: remove smooth scrolling and decorative
transitions, keep content visible, and preserve the actual current state. The
figures then show still scenes that remain interactive. Never
substitute a successful end state for a pending operation just to stop motion.

## Components

### Buttons and Navigation

Use one primary action per local decision area. Primary is Forest Green with
white text, hovering to `primary-hover`. Secondary has a white surface, visible
outline, and Ink text. Ghost/text actions use forest green on light surfaces and
`inverse-primary` on Ink. Destructive actions use explicit wording and the
appropriate danger treatment; refusal is normally secondary.

Primary CTA height is at least 48px. Compact and icon actions should provide a
44px touch target even when the visible icon is smaller. Disabled states must
explain their dependency when it is not obvious; loading keeps its own label
and must not look like a completed or unavailable operation. Reserve button
width to prevent label changes from moving adjacent controls.

Use a visible 3px focus ring with an offset, adjusting its token to the surface.
Use the stronger semantic focus token. Navigation is approximately 88px tall on desktop and
74px on mobile; anchor offsets must account for it. Mobile navigation exposes
its expanded state, closes on selection/Escape, and returns focus predictably.

### Working Figures

A figure is one framed, working example: crop marks at the corners, a white or
Ink surface, and a monospace caption (“Fig. n”, what it shows, and that it is a
simulation with sample data). Text inside figures is HTML, never baked into a
canvas, except decorative ciphertext; a canvas is `aria-hidden` and a
visually hidden sentence states what it shows.

Figures follow the product's real rules and names: the approval sources `gui`,
`cli`, `agent:` and `connect:`; `dmsg` commands and their outputs; `dmsg mcp`
tool names, capabilities and `FORBIDDEN` refusals; the dmsg-connect/1 moves;
second clicks before a secret is shown or copied. Only a person approves: an
agent or website in a figure can create a request and never decides one.
Approval states keep their distinct words (“Approved”, “Signature generated”,
“The site received the result”). A homepage can illustrate unlocking or signing
but must not display a simulated approval as a real proof.

### Vault and Encrypted Channels

A locked vault hides titles, filenames, tags, and content. Do not merely blur
real private text. Revealing or copying a secret is deliberate; copy feedback
says it reached the system clipboard, not that the clipboard is protected.

Channels show participants, roles, access scope, and file versions close to the
content. Distinguish draft, queued, persisted, received, and voluntarily read
states. Invitation acceptance is not vault permission. Removed membership and
future key isolation are different stages. Do not imply that a downloaded file
or disclosed secret can be remotely erased.

### Public Profiles and Contact Rules

The person is the focus; dMsg is a small host identity. Show optional public
fields, handle, and the current contact rule without implying a verified expert
or endorsed project. A handle is optional for personal vault use.

Use labeled radio choices for invitation-based, paid, and closed contact. Keep
the consequence beside the selected option. Paid request confirmation shows
recipient, asset, amount, recipient proceeds, service/network fees, deadlines,
and refund terms before approval. **Payment covers eligible delivery, not a
reply.** Do not treat payment as ongoing contact permission.

### Signature Confirmation and Verification

A formal confirmation identifies the requesting source, signing identity,
request type, exact file/content version, purpose, scope, expiration, and any
fees. Display meaningful content before technical payload details. Keep Reject
visible. Default focus must not encourage accidental approval; changed payloads
require fresh review.

Use precise results: “Approved”, “Signature generated”, and “Application
received the result” describe different events. Ordinary device-signed messages
are not automatically formal statements by a person. Logging in, unlocking a
vault, and authorizing a signature remain separate decisions.

Verification reports content consistency, mathematical signature validity,
identity binding, historical authorization evidence, and current authorization
separately. Unknown evidence remains unknown. A matching SHA-256 fingerprint
alone cannot establish authorship or validate a project.

### Dialogs, Forms, and Recovery

Use accessible dialog semantics, a label, focus trapping, Escape where safe,
and focus restoration. Long content scrolls without making the exit or primary
action unreachable. Never expose irreversible approval through an incidental
backdrop click or generic Enter submission.

Forms have persistent labels; placeholders are examples, not labels. Associate
errors with the field and explain how to recover. A native checkbox, radio, or
file input should remain operable with keyboard and touch.

Recovery information distinguishes login, device authorization, encrypted-data
recovery, and threshold signing control. Explain that offline data recovery
needs both a complete encrypted backup and the separately stored recovery code.
A checklist is not a backup, and a recovery code cannot reconstruct missing
ciphertext. Show actual backup age, missing content, and verification outcome
when those data are available; do not invent reassuring states.

### Trust Sections, FAQ, and Footer

Build trust through checkable evidence and plain explanations: public source,
release/deployment identity where available, scoped audit reports where they
exist, explicit permissions, independent recovery, and operational limitations.
Do not fabricate audit logos, uptime numbers, endorsements, or customer counts.
An open repository does not establish audit coverage or the deployed version.

Explain content privacy alongside metadata visibility, endpoint/update trust,
cloud availability, and governance boundaries. Keep these statements readable
and connect them to the actions they affect. Do not import private operational
plans, credentials, or internal implementation documents into public pages.

FAQ uses simple dividers and native disclosure semantics. The closing CTA
returns to the product's value. Footer attribution reads **Built by ICPanda
DAO**, with relevant public links. Retain the light footer from the selected
landing direction rather than automatically copying another product's dark
multi-column footer.

## Copy & Interaction Integrity

Use task language: **Click it or type it.**, **Let agents work. Keep the
keys.**, **No extension. Still your call.**, **Everything asks. Only you
approve.**, and **Trust you can examine.**
Pair claims with a concrete explanation, not a general safety badge.

Marketing copy follows the chosen present-tense product voice. That does not
justify invented technical assurances, test results, or runtime outcomes. A
simulation must identify sample data and simulated actions. Counts on the page
come from the code (for example the MCP tools of `dmsg mcp`), and install
commands are marked as working only after the first release. Production CTAs must
resolve to verified destinations and reflect installation state only when it
can actually be determined.

Never claim “100% on-chain”, “absolutely secure”, “only you can ever read any
data”, “payment guarantees a reply”, or “revocation erases all copies”. Show
clear consequences instead. TokenList and alink are integrations with their own
roles and permissions, not blanket dMsg endorsements or automatic shared
accounts.

## Design System Notes for Stitch Generation

### Language to Use

Describe dMsg as **“a quiet editorial private workspace with explicit identity
and signing controls.”** Ask for Inter, Paper and Ink, deliberate whitespace,
forest green primary actions, thin rules, and one realistic task example at a time.
Use “private space”, “chosen recipients”, “review the request”, and “inspect the
evidence”. Explain the local daemon, the desktop app and command line, agents
that only ask, and websites that connect without an extension.

Exclude panda collages, coin prices, mining, giant shields, cosmic backgrounds,
AI mascots or agents shown acting without permission, decorative code rain, and
endless feature-card grids. Do not
copy the payment-gate metaphor, blue/amber palette, Geist typography, or
protocol-heavy presentation from 1Pay.ing.

### Component Prompts

> An English dMsg landing page, 1440px wide. Paper canvas with faint grain, Ink
> text, Inter with monospace kickers and captions. An oversized two-line “Your
> space. Your say.” headline sits beside a short explanation and CTA. Below it a
> framed diagram: desktop app, terminal, AI agent and website feed a D-shaped
> local gate; ciphertext leaves for the cloud and a signature for ICP only after
> approval. Continue into a desktop window beside a terminal, an agent
> capability panel with an audit record, a website request sequence ending in
> an approval window, a dark privacy band, trust evidence, release status, FAQ
> and a closing CTA. Aligned rules and crop marks, not a card grid. No invented
> social proof.

> A dMsg signature review window at 390px wide. White surface, 16px corners,
> clear Inter typography. Show the requesting application and source, identity,
> file version, purpose, scope, and readable content. Keep Reject visible next
> to a solid forest green approval action. Technical detail is expandable, but the
> consequence of approval is always visible. Do not make the request look safe
> merely because its format can be parsed. No decorative gradient or urgency.

> A dMsg public profile with invitation-only, paid-request, and closed contact
> modes. The person is prominent and the dMsg mark is small. Use native radio
> choices, a live visitor preview, and a plain explanation that the fee covers
> delivery rather than a reply. Keep profile and rules within one clean panel;
> show fees and refund conditions before any payment confirmation.

### Incremental Iteration

Start with the task and reading order. Apply neutral surfaces, then forest green for
selection and action. Add a state color only when there is a distinct state to
explain. Reuse the nearest existing component and semantic token before adding
a new visual variant.

For each changed screen, inspect actual desktop and narrow-screen rendering,
keyboard/focus behavior, all meaningful states, text contrast, and image
quality. Test the primary path and an appropriate failure or cancellation path.
Keep demo evidence separate from production claims. This file guides design;
it does not authorize backend actions, deployments, or unrelated migrations.
