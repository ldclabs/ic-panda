import { browser } from '$app/environment'
import { currentLocale } from '$lib/i18n'
import { loadLegalDocument } from '$lib/legal'
import type { PageLoad } from './$types'

export const ssr = true
export const prerender = true

export const load: PageLoad = async ({ fetch, parent }) => {
  await parent()
  try {
    return { doc: await loadLegalDocument('support', currentLocale(), fetch) }
  } catch (error) {
    if (!browser) throw error
    return { doc: null }
  }
}
