import { derived } from 'svelte/store'
import { locale as selectedLocale } from '$lib/i18n'

export const locale = derived(selectedLocale, (value) => new Intl.Locale(value))
