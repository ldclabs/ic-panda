// Locale state is scoped to this client-only SPA. Do not enable SSR without
// moving i18n state into a per-request Svelte context.
export const ssr = false
export const prerender = false
