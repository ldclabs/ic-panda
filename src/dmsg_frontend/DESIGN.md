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
    lineHeight: '1.04'
    letterSpacing: -0.067em
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

dMsg is a private workspace and identity signing service for Web3, centered on
its Chrome extension. It helps people keep secrets, exchange encrypted content
with chosen people, and approve specific signatures. A public identity and
contact rules provide a doorway into that work without making the workspace
public.

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

The local visual reference is the selected editorial direction implemented in
[the interactive landing prototype](prototypes/landing/index.html). That
prototype establishes composition and product storytelling; it is not evidence
that its demonstrated backend actions exist. Its checks are recorded in
[design QA](prototypes/landing/design-qa.md).

The tokens above consolidate the reference into reusable roles. Some are
intentional implementation refinements: stronger focus and control boundaries,
44px touch targets, a consistent radius/spacing scale, and explicit feedback
colors. They are not claims that every existing component already uses them.
The prototype's 14px showcase radius and 57/90px section padding may be
normalized to this scale during integration.

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

**Inter** carries the wordmark prototype, narrative, navigation, buttons, and
product UI. Use the variable font for the 550/650 weights in the token map.
Self-host licensed fonts for production; the standalone prototype embeds Inter.
Use this fallback stack:

```css
font-family: Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI',
  'PingFang SC', 'Microsoft YaHei', sans-serif;
```

**System monospace** carries file hashes, addresses, exact identifiers, and
technical payloads. Amounts use tabular numerals and retain their relevant
precision. A short address preview must have a way to inspect and copy the full
value; matching abbreviations are not proof that two addresses are identical.

The landing hero is intentionally oversized: desktop
`clamp(70px, 8.3vw, 116px)`, 650 weight, 1.04 line height, and tight tracking.
Use the two-line “Your space. / Your say.” composition. At the mobile breakpoint,
use `clamp(60px, 10.5vw, 80px)` and check the complete heading at 320px. This
exception belongs to short English marketing headlines, not dialogs, app
navigation, translations, or safety explanations.

Section headings scale from 32px to 46px; product headings use 18–32px. Body
copy uses 16px, with a 19px lead and 14px supporting text. Limit long prose to
roughly 60–65 characters per line. Uppercase eyebrows use 12px Inter with
0.14em tracking; they orient the page rather than carry essential information.
Avoid reproducing the prototype's smallest demo labels as important UI text.

Chinese uses the system CJK fallback, ordinary tracking, and a separately
checked headline scale (approximately 34–44px for mobile marketing). Do not
force English line breaks or negative tracking onto Chinese. Where RTL locales
are introduced, use logical spacing, correct reading order, and LTR islands for
hashes and addresses; their support must be tested rather than inferred.

## Layout & Spacing

A marketing page uses a centered 1280px maximum wrapper **including** horizontal
padding: 48px on desktop, 32px on tablet, 24px on mobile. Large screens may use a
1376px wrapper without increasing prose measure. Use border-box sizing.

Use a 4px spacing base. Favor 8/12px within controls, 16/24px within panels,
32/48px between groups, and 56/72/88px vertical section padding across
mobile/tablet/desktop. Large editorial two-column sections may use 80px between
columns. Let grouping and reading order determine spacing rather than adding a
border to every object.

The selected landing rhythm is:

1. Editorial hero: headline above; CTA on the left and concise explanation on
   the right below it.
2. Numbered capability tabs: vault, encrypted collaboration, explicit signing.
3. A pale task surface with one useful product example, followed by brief
   practical benefits.
4. Public contact rules with an example profile and visible consequences.
5. A full-width Ink privacy section explaining device, cloud, and ICP roles.
6. Trust evidence: public code, separate permissions, recovery, and limitations.
7. Cross-application request/review/result sequence with scoped integrations.
8. FAQ, closing action, and restrained footer.

Do not compress all capabilities into the first fold. The page should establish
what dMsg does before asking the reader to interpret its infrastructure.

At approximately 1000px, reduce gaps and simplify nested columns. At 760px,
collapse navigation and major two-column sections. At 440px, stack the contact
profile and rules, and turn the developer sequence into vertical steps. These
are content-driven breakpoints, not device detection. Check 320px, 390px,
tablet widths, and a 1440px desktop; no horizontal overflow may hide controls.

The extension's full workspace can use a compact sidebar and wider data areas.
A side panel and independent confirmation window must fit their own viewport
without shrinking a desktop screenshot. The website remains a lightweight
surface for discovery, public verification, and guidance; a narrow website is
not a promise of a full mobile vault or messenger.

## Elevation & Depth

Use depth sparingly. The priority order is **spacing and alignment → simple
rules → surface tint → border → shadow**. Keep ordinary feature explanations
on the base page surface. Use a single contained panel when the content is an
actual product object or interactive example.

Two elevation treatments have distinct jobs:

- **Product preview:** `0 8px 20px rgba(23, 61, 41, 0.06)`, for the white
  interactive workspace example on its pale surround.
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
prototype contains a generated raster concept, not a final vector master; do
not trace it into a new “official” logo or infer exact geometry from it. Provide
light, dark, and small-size assets from the same approved master when available.

Spell the name **dMsg**. Do not use “DMSG” as the marketing wordmark or combine
it with the historical “ICPanda Message” title in the hero. The prototype
wordmark uses Inter 800 at approximately 26–29px beside a 34–40px symbol box;
maintain optical spacing and check the visible mark within that box.

Use a consistent line-icon family. The prototype uses **Remix Icon**; reuse
matching existing icon components or licensed source icons in production. Use
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
success state must follow the corresponding real result. Prototype simulation
must be clearly identified as simulation.

Honor `prefers-reduced-motion: reduce`: remove smooth scrolling and decorative
transitions, keep content visible, and preserve the actual current state. Never
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
Use the stronger semantic focus token; the prototype's smaller icon targets
are not the final accessibility standard. Navigation is approximately 88px tall on desktop and
74px on mobile; anchor offsets must account for it. Mobile navigation exposes
its expanded state, closes on selection/Escape, and returns focus predictably.

### Capability Tabs and Workspace Examples

Tabs use a numbered label, a shared baseline, and a 2px active forest green rule. Only
one task panel is visible at a time. Implement tab/tabpanel relationships,
roving focus, arrow-key navigation, and Home/End. On mobile, place the number
above the label instead of overflowing the row.

Each panel explains one task and shows an actual item or request. Demonstrations
use safe sample data and visibly identify their scope. A homepage can illustrate
unlocking or signing but must not display a simulated approval as a real proof.

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

Use task language: **Keep it private**, **Share with intent**, **Sign with
clarity**, **Be reachable. On your terms.**, and **Trust you can examine.**
Pair claims with a concrete explanation, not a general safety badge.

Marketing copy follows the chosen present-tense product voice. That does not
justify invented technical assurances, test results, or runtime outcomes. A
prototype must identify sample data and simulated actions. Production CTAs must
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
evidence”. Keep the Chrome extension central to the product explanation.

Exclude panda collages, coin prices, mining, giant shields, cosmic backgrounds,
AI assistants, decorative code rain, and endless feature-card grids. Do not
copy the payment-gate metaphor, blue/amber palette, Geist typography, or
protocol-heavy presentation from 1Pay.ing.

### Component Prompts

> An English dMsg landing page, 1440px wide. Paper canvas, Ink text, Inter.
> An oversized two-line “Your space. Your say.” headline sits above a left CTA
> and a right explanation of the private workspace. Below it, numbered tabs
> reveal one vault, channel, or signature task. Continue into contact rules,
> a dark privacy section, inspectable trust evidence, integrations, FAQ, and
> a closing CTA. Use generous whitespace and aligned rules, not a card grid.
> Explain cloud metadata and recovery boundaries. No invented social proof.

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
