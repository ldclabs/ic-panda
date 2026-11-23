---
name: dMsg iOS
colors:                       # Shared/Palette.xcassets, light / dark
  Forest: ['#145C45', '#70C7A3']        # tint, links, selected states
  ForestFill: ['#145C45', '#145C45']    # filled actions and own bubbles, under white labels
  Paper: ['#F7F8F4', '#0E1512']         # plain pages: workspace lists, conversation, brand screens
  PaperGrouped: ['#EEF2EB', '#000000']  # grouped pages: settings, details, forms
  Card: ['#FFFFFF', '#1A2621']          # cards the app draws itself
  Fill: ['#E8F0E9', '#233A30']          # icon tiles, quotes, selected capsules
  BadgeRed: ['#B42318', '#B42318']     # approval counts, under white labels
  AvatarForest/Sage/Amber: tones behind initials, each with an Ink pair (Android Avatar.kt)
radii: { small: 8, regular: 12, large: 18 }   # NativeTheme.radius*
type:
  row-title: body (unread or emphasis: semibold)
  row-subtitle: subheadline, secondary      # rowSubtitle()
  metadata: footnote, secondary
  badge: caption2 (counts and tiny glyphs only)
---

# dMsg iOS design

The iOS app keeps the system's skeleton: `List`/`Form`, `Section`, `LabeledContent`,
toolbars, `Menu`, `confirmationDialog`, `ContentUnavailableView` and the materials the
SDK gives them. dMsg only paints a few things itself, and those follow this file.
The diagnosis behind it is the 2026-11-23 UI quality review.

## Rules

- Colours come from `Shared/Palette.xcassets` through `Theme` (SwiftUI) or `NativeTheme`
  (UIKit and extensions). No colour literals, no `Theme.forest.opacity(…)`: a quiet tint
  is `Theme.fill`. System semantic colours (`.secondary`, `.separator`, `.red`) are fine.
- Radii are `Theme.radiusSmall` / `radius` / `radiusLarge` (or `NativeTheme.*` in
  extensions). Icon tiles are relative to their size.
- Text uses text styles, never `.font(.system(size:))` with a number. `.caption` is
  retired; metadata is `.footnote`, `caption2` is only for badges.
- Sizes that sit next to text (icon tiles, avatars, badges) use `@ScaledMetric`.
- `scripts/ios/test_ui_conventions.py` enforces the literal rules and the palette
  contrasts; `verify.sh` runs it. Do not widen its exemptions to make it pass.

## Surfaces

- `privateScreen(.plain)` puts a page on `Paper`: the three workspace lists, the
  conversation, large type and other scroll pages that read like a document.
- `privateScreen()` (grouped) puts a page on `PaperGrouped`: everything with sections.
- Grouped rows keep the system row colour. A custom `listRowBackground` loses the
  pressed highlight, so the palette pairs the page with the system row instead: light
  `#EEF2EB` against white (1.13:1), dark black against `#1C1C1E` (1.23:1).
- `Card` is for what the app draws: other people's bubbles, the composer field,
  floating notices, plan cards. On dark `Paper` it is `#1A2621` (1.18:1), as on desktop.
- Bars over scrolling content use `edgeBar` (`scopeBar` for the workspace tabs): iOS 26
  gets `safeAreaBar` and its scroll edge effect, earlier systems a page-coloured or `.bar`
  inset. The minimum stays iOS 18, so iOS 26 API goes through such helpers only.
- Floating controls use `glassSurface`: the composer field, its attach button and the
  card above it for the reply, attachments and warnings. The send button appears once
  there is something to send.
- Short sheets (filters, naming a file) open at `.medium`.

## People and conversations

- `Avatar` draws people as circles and conversations as rounded squares. Without a
  picture, the first letter sits on one of three tones picked from the account or
  conversation ID with Java's `String.hashCode`, so a person looks the same on Android.
- One's own bubbles are `ForestFill` with white text in both modes; everyone else's are
  `Card`. Quotes and attachments inside a bubble use `bubbleInset(own:)`.
- Consecutive messages from one sender form a group: 2 pt apart, the corners on the
  sender's side tighten, the portrait and name show once, the time at the end.
- A queued message slides in from the bottom; with Reduce Motion it only fades.

## Screens

- Vault fields work as in Passwords: tapping the value copies it, a hidden value has its
  own eye button, and the `⋯` menu keeps the rarer actions (website, large type, field
  reference). The header reads as one line, "Password · Synced".
- Inputs carry their label above them; the placeholder appears only when it adds
  something ("Leave empty to keep existing value"). Generating a password is the wand
  button at the end of its field.
- A count that waits on the person (approvals) is a red `CountBadge`; zero shows nothing.
- The account header is a 60 pt round `Avatar` with the name in `title3` semibold.
- Plan and usage start with `UsageSummary`: one storage bar (used, reserved for uploads,
  free) and two gauges; the exact figures follow as rows.
- The access screen centres the mark, the greeting and, for a returning account, its
  avatar and name. It has one filled action: unlock for a returning account, otherwise
  opening an existing account. Explanations there are plain footnotes; a pending
  authorization keeps its `NoticeBar`.
- Files that exist only on this device are one row at the top of Files; the backup page
  explains why in its footer.

## Words on screen

- An explanation goes in its section's footer, one paragraph per section. A toggle with
  its own explanation gets its own section, as in Settings.
- `NoticeBar` is for a problem to act on or wait for: a failure, a pending step, a risk.
- Placeholders do not repeat a field's label.

## Icons

- System actions use SF Symbols. Business objects use the desktop's Remix paths as custom
  symbols in `Shared/Glyphs.xcassets`, generated by `scripts/ios/sync-icons.py` and chosen
  through `AppGlyph`. They size with the surrounding font like SF Symbols: do not make
  them `resizable()` or give them fixed frames.
- Each symbol carries the small, medium and large scales. With the medium scale alone,
  SwiftUI's `Label` shows no icon in list rows and menus.
- The four tabs have `.fill` symbols; the tab bar picks them, as system tabs show filled
  icons. The widget compiles the same catalog.
- Within one section or menu, prefer one family.

## Motion and feedback

- State changes move or answer: favourites pop in with a selection tap, a copied field's
  menu turns into a checkmark (`contentTransition(.symbolEffect(.replace))`) and the
  clipboard gives a light tap, a failed send and a finished approval use
  `sensoryFeedback`. The conversation list animates conversations that arrive, leave or
  change read state.
- With Reduce Motion, movement and bounces become fades or nothing; `sensoryFeedback`
  follows the system's haptics setting on its own.
- A first read shows `PlaceholderRows` in the shape of the list instead of a centred
  spinner. Once rows are on screen, a refresh stays in the activity button.

## App icon

The icon set has light, dark and tinted variants. The dark and tinted ones are the
frontend's `mark.svg` geometry at the light icon's position (630 × 720 at 197, 152 on
1024) on a transparent background: the dark one in the mark's dark-mode colours, the
tinted one in white with the gap around the gem, like Android's monochrome icon.

## Checking a change

Before handing over a visual change:

1. `python3 -B -m unittest discover -s scripts/ios -p 'test_*.py'`.
2. Build the `dMsg` scheme and run the UI tests of the touched screens.
3. Run `VisualBaselineTests` before and after the change, export both result bundles
   with `scripts/ios/export-screenshots.sh <run.xcresult> <folder>` and compare: vault,
   detail, account, messages, conversation and the returning screen in light and dark,
   English and Simplified Chinese, Arabic and an accessibility text size. Put the
   touched screens next to the matching system app (Settings, Passwords, Messages,
   Files) and report what was actually run.

The checklist for those screenshots:

- Colours and radii come from the palette and `Theme`; grouped pages and cards keep at
  least 1.12:1 against each other.
- At most four type sizes per screen; list subtitles are no smaller than `subheadline`.
- Explanations sit in footers; banners are only for problems.
- One icon family per section or menu; people are round.
- Every state change animates or answers with a haptic.
