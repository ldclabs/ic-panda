# i18n

The desktop and website offer the same 23 locales: English (`en`), Simplified
Chinese (`zh`), Traditional Chinese (`zh-Hant`), Russian (`ru`), Arabic (`ar`),
French (`fr`), Spanish (`es`), Hindi (`hi`), Indonesian (`id`), Brazilian
Portuguese (`pt-BR`), Japanese (`ja`), German (`de`), Korean (`ko`), Italian
(`it`), Turkish (`tr`), Vietnamese (`vi`), Thai (`th`), Filipino (`fil`), Malay
(`ms`), Bengali (`bn`), Urdu (`ur`), Polish (`pl`), and Dutch (`nl`). These
represent 22 languages and two Chinese scripts.

An explicit saved choice wins over the browser preference list. An explicit
Chinese script wins over region; TW/HK/MO share `zh-Hant`, while unqualified
`zh` stays Simplified. All Portuguese tags use `pt-BR`; legacy `in` maps to `id`
and `tl` maps to `fil`. Arabic and Urdu use RTL.
Unsupported preferences are skipped, with English as the final fallback.

## Usage

```svelte
<script lang="ts">
  import { t, locale } from '$lib/i18n'
</script>

<button>{$t('Confirm')}</button>
```

English source messages are keys. Add new messages to `messages/en.ts` and all
translated catalogs. Each catalog satisfies the same `Messages` type. Use
whole sentences and named `{parameters}` for values that translators may need to
reorder. Svelte escapes the returned string; do not render translations with
`{@html}`. Translate UI copy, not user content, protocol identifiers, permission
values, URLs, secrets, or token symbols.

Use the reactive `$t` store in components. Keep presentation arrays reactive
(`$derived` in runes components, `$:` in legacy components). For event-time errors
outside components use `tr`. Unknown remote error text remains unchanged.

The root layout's load function waits for the initial catalog, so the page and
its fragment targets exist when SvelteKit restores scroll. Other languages load on demand;
a failed switch leaves the previous language usable and can be retried. A failed
startup catalog falls back to English without deleting the stored preference.
`setLocale` applies document `lang` and `dir`, persists the choice when storage is
available, and protects against out-of-order downloads. `app.html` also sets
`lang` and `dir` before paint; its detection logic is checked against the registry.

`app.html` includes a default title and description for the static SPA fallback.
The root layout localizes that same description element and the Open Graph /
Twitter metadata in place, including the social card for the selected language.
Page titles can override the localized default. Do not add duplicate meta tags.
The static fallback is English for clients that do not execute JavaScript.
Regenerate the registered SVG/PNG social cards with `pnpm social-card`
from this frontend directory; their text comes from the same catalogs and their
vector scene is shared with the live Hero in `components/landing/hero-art.js`.

The locale store is a client-only SPA singleton. Root `+layout.ts` disables SSR.
Before enabling SSR, move locale state into a per-request Svelte context.

Dates and display amounts use the selected locale; payment parsing, base-unit
amounts, fees, and network calls retain their original semantics. When calling a
formatting helper from Svelte, ensure the expression also depends on `$locale`.
The compatibility store in `lib/stores/locale.ts` still exposes `Intl.Locale`.

## Translation style

Keep commands short and natural for the target language; use a consistent form
of address. Describe the UI action, not a literal English metaphor: "free space"
releases disk storage, "hold" means press and keep pressing, and "Space" is a
keyboard key. A grant is access permission, execution runs an operation, a
digest is a cryptographic hash, and root rotation replaces an encryption key.
Keep product names, URLs, protocol identifiers and interpolation arguments intact.
Quantity labels can use a noun followed by the count when a full sentence would
need plural inflection. iOS uses native `stringsdict` rules, including Polish
one/few/many/other. Rules and the `tl` alias follow
[Unicode CLDR](https://unicode.org/cldr/charts/48/supplemental/language_plural_rules.html).
Thai, Bengali and Urdu use system font fallbacks; connected scripts and combining
marks need normal letter spacing and sufficient heading line height.

## Verification

From this frontend directory:

```sh
pnpm exec vitest run src/lib/i18n
pnpm exec svelte-check
pnpm build
```

Tests cover catalog completeness, interpolation parameters, preference priority,
pre-paint detection, reactive updates, RTL, blocked storage, racing switches,
failed downloads, retry, and startup fallback. UI verification should also cover
all 23 locales, refresh persistence, mobile layouts, and preserving an edited
form while switching languages.

Source coverage tests also reject untranslated UI labels and translation calls in
clipboard payloads. Protocol prefixes such as `PRIZE:` are language-independent.
The legacy chat formats stored timestamps during rendering, rather than caching
localized date strings. RTL applies to floating menus and nested charts as well
as the document. Source messages and deferred error labels remain language-neutral
until rendered, so switching languages does not rewrite account or message data.
