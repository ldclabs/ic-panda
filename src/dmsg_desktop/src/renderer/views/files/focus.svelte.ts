/** A file to show in the library, set by the vault's attachments; the Files view clears it once shown. */
export const focus = $state<{ id: string | null }>({ id: null })
