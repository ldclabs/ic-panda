---
name: dMsg Android
colors:
  light:
    page: '#EFF1ED'          # background = surface, T95
    card: '#FBFDF8'          # surfaceContainerLow, T99
    container: '#EAECE7'     # surfaceContainer, T93 (menus, rail)
    container-high: '#E4E6E1'    # dialogs, T91
    container-highest: '#DEE0DC' # other people's message bubbles, T89
    bright: '#F8FAF5'        # floating workspace bar, T98
    ink: '#191C1A'
    ink-variant: '#404943'
    outline: '#707972'
    outline-variant: '#C0C9C1'
    primary: '#145C45'       # brand forest
    primary-container: '#C0ECD7'
    secondary-container: '#D2E8DB'  # information notices
    tertiary: '#9B4500'      # gem
    error: '#BA1A1A'
    error-container: '#FFDAD6'
  dark:
    page: '#0E1512'          # T6
    card: '#161D1A'          # T10
    container: '#1A211E'     # T12
    container-high: '#242C28'    # T17
    container-highest: '#2F3633' # T22
    bright: '#333B37'        # T24
    ink: '#DDE4DE'
    ink-variant: '#BBCAC1'
    outline: '#85948C'
    outline-variant: '#3C4A43'
    primary: '#91D4B7'
    primary-container: '#264E3E'
    secondary-container: '#384B42'
    tertiary: '#FFB68E'
    error: '#FFB4AB'
    error-container: '#93000A'
typography:
  scale: Material 3 baseline on the system sans (display 57/45/36, headline 32/28/24, title 22/16/14, body 16/14/12, label 14/12/11 sp)
  ideographic: letter spacing 0 for zh, ja and ko
  code: monospace, always left to right (identifiers, fingerprints, digests)
  tabular: tnum figures (one-time codes, countdowns)
rounded:
  extraSmall: 8dp
  small: 12dp
  medium: 16dp
  large: 24dp
  extraLarge: 28dp
  full: CircleShape
spacing:
  unit: 4dp
  page-margin: 16dp below 600dp window width, 24dp from 600dp
motion:
  components: material3 1.4.0 standard motion scheme (its MotionScheme API is internal)
  spatial: 'springs, damping 0.8 / stiffness 380 (default), 0.6 / 800 (fast), 0.8 / 200 (slow)'
  effects: 'critically damped springs, stiffness 1600 (default), 3800 (fast), 800 (slow)'
---

## Brand & Style

The Android app keeps the information architecture of iOS — the same four
workspaces, names, order and flows — and takes its look and behaviour from
Android: Material 3 components, system bars, predictive back, a splash and a
themed icon. dMsg shows through colour, the mark, type weight and motion,
not by copying iOS chrome.

The base is quiet paper: pages are a low-chroma green-grey, cards are one
step lighter, and the forest green is kept for actions, selection and focus.
State colours (error, warning, verified) stay fixed and never follow the
wallpaper.

### Scope and Source of Truth

This file describes the theme as built:

| Concern | Source |
| --- | --- |
| Colour roles, `page`, `card` | `app/src/main/java/net/dmsg/android/ui/ThemeColors.kt` |
| Shapes, type, `code()`, `tabular()`, `pagePadding()`, `pageMargin()` | `ui/ThemeTokens.kt` |
| `DMsgTheme`, `OnCard` | `ui/Theme.kt` |
| Window, splash and accent colours | `res/values{,-night}/theme_colors.xml`, `styles.xml` |
| Launcher, themed icon | `res/mipmap-anydpi/ic_launcher.xml`, `res/drawable/launcher_*.xml` |

When the code and this file differ, the code wins and this file is
corrected. The desktop keeps its own design file
(`src/dmsg_desktop/DESIGN.md`); both share the forest and gem brand colours.

## Rules

- **A1 Roles, not literals.** Pages take colours from `MaterialTheme.colorScheme`,
  corners from `MaterialTheme.shapes`, type from `MaterialTheme.typography` and
  margins from `pagePadding()` / `pageMargin()`. Literal colours, corners, sp sizes,
  monospace and whole-page padding live only in the theme files.
- **A2 Page and card.** The scaffold, top bar and sticky headers use
  `colorScheme.page`; grouped rows, detail cards, empty panels and search fields
  use `colorScheme.card`. Material components that default to `surface`
  (`ListItem`, `OutlinedCard`) blend into the page; inside a card or a sheet,
  wrap them in `OnCard` so they take the card colour. `SettingsGroup` already does.
- **A3 One page margin.** 16 dp on phones, 24 dp once the window is 600 dp wide,
  on every side of a page and around its bottom actions.
- **A4 Identifiers look technical.** Account IDs, principals, fingerprints and
  digests use `code()`; one-time codes and countdowns use `tabular()`.
- **A5 Motion shows cause and effect.** Screen transitions, list placement and
  predictive back use the springs under Motion, defined once in `ThemeTokens.kt`;
  the system "remove animations" setting ends them immediately.
- **A6 The theme is provided once.** `DMsgTheme` wraps each activity; the only
  nested theme is `OnCard`.

## Colors

Roles come from HCT tonal palettes (material-color-utilities 0.4.0, 2021 tone
mapping):

| Palette | Seed | Chroma | Use |
| --- | --- | --- | --- |
| Primary | forest `#145C45`, hue 169.2 | 33.4 for accents, 24 for containers and fixed roles | actions, selection, focus |
| Secondary | hue 169.2 | 14 | information notices, tonal fills |
| Tertiary | gem `#B45309`, hue 47.9 | 56.7 | brand accent |
| Error | hue 25 | 84 | errors and warnings |
| Neutral, light | hue 162 | 4 (variant 8) | surfaces and ink |
| Neutral, dark | hue 170 | 7 (variant 11) | surfaces and ink |

Accents use tone 40 (light) and 80 (dark); containers 90 and 30; text on them
100/20 and 30/90. Light primary stays the exact brand forest.

Surface tones keep the card one step above the page in both modes:

| Role | Light | Dark | Used for |
| --- | --- | --- | --- |
| `surfaceContainerLowest` | 100 | 4 | — |
| `surfaceContainerLow` (`card`) | 99 | 10 | cards, sheets |
| `surfaceBright` | 98 | 24 | the floating workspace bar |
| `surface`, `background` (`page`) | 95 | 6 | pages, top bars |
| `surfaceContainer` | 93 | 12 | menus, navigation rail |
| `surfaceContainerHigh` | 91 | 17 | dialogs |
| `surfaceContainerHighest` | 89 | 22 | other people's message bubbles |
| `surfaceDim` | 86 | 6 | — |

`ThemeColorsTest` keeps body text and text on containers at WCAG AAA (7:1),
accents, links and the snackbar at AA (4.5:1), and outlines at 3:1 on both the
page and the card.

## Typography

The Material 3 baseline scale on the system sans. Chinese, Japanese and
Korean drop the Latin tracking (letter spacing 0), which otherwise only
pushes ideographs apart. Page titles use `titleLarge`, workspace roots
`headlineSmall`, list rows `bodyLarge` with `bodyMedium` supporting text;
`bodySmall` is for metadata, not for explanations. Aim for at most four
sizes on one screen.

## Layout & Spacing

A 4 dp grid. `pagePadding()` gives the page margin on every side;
`pagePadding(top = 0.dp)` sits under a browse header and
`bottomInset = LocalWorkspaceBarInset.current` keeps the last row clear of
the floating bar. Bottom actions and sheet content use the same margin.
Card content currently uses 16–18 dp; the component kit will fix it to one value.

## Shapes

| Token | Radius | Used for |
| --- | --- | --- |
| `extraSmall` | 8 dp | text fields, menus, snackbars |
| `small` | 12 dp | chips, thumbnails, item icons |
| `medium` | 16 dp | cards, notices, message bubbles |
| `large` | 24 dp | grouped panels, detail cards, the composer |
| `extraLarge` | 28 dp | sheets, dialogs |
| `CircleShape` | full | buttons, avatars, search fields, the workspace bar |

## Motion

material3 1.4.0 keeps `MotionScheme`, `MaterialTheme.motionScheme` and
`MaterialExpressiveTheme` internal, so its components animate with the
standard scheme and the theme cannot replace it. The app's own animations use
the Material 3 expressive spring values from the front matter: movement may
overshoot slightly, fades and colour changes never do. Define them once in
`ThemeTokens.kt` together with the first transition that uses them, and take
specs from there rather than fixed durations.

## Platform

- **Splash.** The platform splash (API 31) shows the adaptive icon on the page
  colour of the current system appearance, stays while the local engine opens
  (at most two seconds), then fades out while the icon grows slightly.
- **Night theme.** `values-night` switches the window, status and navigation bar
  appearance before the first Compose frame. The in-app appearance setting
  applies from the first frame; the splash follows the system setting.
- **Themed icon.** The adaptive icon has a monochrome layer: the mark with the
  gap around the gem cut out, so the launcher can tint it.

## Checking a Change

1. `./gradlew :app:testProductionNoneDebugUnitTest` (or the variant in use) runs
   `ThemeColorsTest` and `UiConventionsTest` with the other JVM tests.
   Do not widen `UiConventionsTest` allowances to make a change pass.
2. Compare screenshots before and after in light and dark on the emulator:
   a root workspace, a detail page, a settings group, a sheet and a dialog;
   add Chinese, Arabic (RTL) and 200% font size for layout changes.
3. Report what was run and what was not.

## Known Gaps

- `Card` call sites without colours still use the filled default
  (`surfaceContainerHighest`); the component kit will replace them with one card.
- Dialogs use the Material defaults (`surfaceContainerHigh`, 28 dp).
- Lists still end rows with chevrons and use `MoreHoriz`; screens still stack many
  notices and text buttons. These are tracked for the component and screen passes.
- Dynamic colour is not offered yet; when it is, state colours stay fixed.
