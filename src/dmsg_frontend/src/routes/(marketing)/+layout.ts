// The IC asset canister uses index.html as the SPA fallback for existing
// /_/channel and /handle links. Keep that fallback; isolate marketing from
// legacy authentication and network initialization through this route group.
export const prerender = false
export const ssr = false
export const trailingSlash = 'never'
