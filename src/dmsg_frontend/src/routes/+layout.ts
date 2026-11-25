import { browser } from '$app/environment'
import { initLocale } from '$lib/i18n'
import type { LayoutLoad } from './$types'

// Dynamic app, share, profile, and login routes stay client-only; public
// marketing, legal, and static tool routes opt in to build-time English
// prerendering (single-locale static build, never multi-request server SSR).
export const ssr = false
export const prerender = false

export const load: LayoutLoad = async () => {
  // Render the page before SvelteKit restores scroll or resolves a URL fragment.
  if (browser) await initLocale()
  return {}
}
