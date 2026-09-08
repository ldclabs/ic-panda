# i18n

This frontend follows the locale registry and detection policy in
`token-listing/packages/token-list-app/src/lib/i18n`: English (`en`), Simplified
Chinese (`zh`), Russian (`ru`), Arabic (`ar`), French (`fr`), and Spanish (`es`).
An explicit saved choice wins over the browser preference list. Chinese variants
resolve to Simplified Chinese; unsupported preferences fall back to English.

## Usage

```svelte
<script lang="ts">
  import { t, locale } from '$lib/i18n'
</script>

<button>{$t('Confirm')}</button>
```

English source messages are keys. Add new messages to `messages/en.ts` and all
five translated catalogs. Each catalog satisfies the same `Messages` type. Use
whole sentences and named `{parameters}` for values that translators may need to
reorder. Svelte escapes the returned string; do not render translations with
`{@html}`. Translate UI copy, not user content, protocol identifiers, permission
values, URLs, secrets, or token symbols.

Use the reactive `$t` store in components. Keep presentation arrays reactive
(`$derived` in runes components, `$:` in legacy components). For event-time errors
outside components use `tr`. Unknown remote error text remains unchanged.

The root layout waits for the initial catalog. Other languages load on demand;
a failed switch leaves the previous language usable and can be retried. A failed
startup catalog falls back to English without deleting the stored preference.
`setLocale` applies document `lang` and `dir`, persists the choice when storage is
available, and protects against out-of-order downloads. `app.html` also sets
`lang` and `dir` before paint; its detection logic is checked against the registry.

The locale store is a client-only SPA singleton. Root `+layout.ts` disables SSR.
Before enabling SSR, move locale state into a per-request Svelte context.

Dates and display amounts use the selected locale; payment parsing, base-unit
amounts, fees, and network calls retain their original semantics. When calling a
formatting helper from Svelte, ensure the expression also depends on `$locale`.
The compatibility store in `lib/stores/locale.ts` still exposes `Intl.Locale`.

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
all six languages, refresh persistence, mobile layouts, and preserving an edited
form while switching languages.

Source coverage tests also reject untranslated UI labels and translation calls in
clipboard payloads. Protocol prefixes such as `PRIZE:` are language-independent.
The legacy chat formats stored timestamps during rendering, rather than caching
localized date strings. RTL applies to floating menus and nested charts as well
as the document. Source messages and deferred error labels remain language-neutral
until rendered, so switching languages does not rewrite account or message data.
