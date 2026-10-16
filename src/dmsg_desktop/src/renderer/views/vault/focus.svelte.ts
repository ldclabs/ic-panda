/** An item to open in the vault, set by the command palette; the vault clears it once open. */
export const focus = $state<{ id: string | null }>({ id: null })
