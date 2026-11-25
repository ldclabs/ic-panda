import { browser } from '$app/environment'
import { initLocale, loadCatalog, preferredLocale } from '$lib/i18n'
import type { LayoutLoad } from './$types'

// Dynamic app, share, profile, and login routes stay client-only; public
// marketing, legal, and static tool routes opt in to build-time English
// prerendering (single-locale static build, never multi-request server SSR).
export const ssr = false
export const prerender = false

export const load: LayoutLoad = async () => {
  if (!browser) return { prerendered: false }
  // A prerendered page arrives with its English markup in #dmsg-app. It
  // hydrates in English, because Svelte keeps server-rendered {@html} as it
  // is, and +layout.svelte applies the reader's locale once it has mounted.
  // Other pages render once, in the reader's locale, before SvelteKit
  // restores scroll or resolves a URL fragment.
  const prerendered =
    document.querySelector('#dmsg-app > :not(script)') !== null
  if (prerendered) void loadCatalog(preferredLocale()).catch(() => {})
  else await initLocale()
  return { prerendered }
}
