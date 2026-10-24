/** One connection notice per window; no account data or request parameters. */
export const network = $state<{ state: 'idle' | 'recovering' | 'offline' }>({ state: 'idle' })
