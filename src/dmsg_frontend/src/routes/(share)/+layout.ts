// The share page (`/s/<share_id>`) is a public, client-only page: the key
// is in the fragment, so nothing of it may be rendered on a server, and it
// shares no authentication or network setup with the app.
export const prerender = false
export const ssr = false
export const trailingSlash = 'never'
