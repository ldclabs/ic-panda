import { initLocale } from '$lib/i18n'
import type { LayoutLoad } from './$types'

// Locale state is scoped to this client-only SPA. Do not enable SSR without
// moving i18n state into a per-request Svelte context.
export const ssr = false
export const prerender = false

export const load: LayoutLoad = async () => {
  // Render the page before SvelteKit restores scroll or resolves a URL fragment.
  await initLocale()
  return {}
}
