import { t } from '../i18n/index.svelte'
import { toast } from './toast.svelte'

/** Copy text the window already shows, such as a user name or an invitation; secrets go through the main process instead. */
export async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    toast(() => t('copiedText'))
  } catch {
    toast(() => `${t('error')}: UNAVAILABLE`)
  }
}
