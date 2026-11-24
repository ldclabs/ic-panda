/** A file or folder to show in the library; the Files view clears it once shown. */
export const focus = $state<{ id: string | null; folder: string | null }>({ id: null, folder: null })
