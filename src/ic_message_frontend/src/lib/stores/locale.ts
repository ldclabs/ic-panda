import { readable } from 'svelte/store'

export const locale = readable(
  new Intl.Locale(window?.navigator.language || 'en')
)
