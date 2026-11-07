import { t } from '../i18n/index.svelte'
import { toast } from './toast.svelte'

/** Copy text the window already shows, such as a user name or an invitation; secrets go through the main process instead. */
export async function copyText(text: string, notice: () => string = () => t('copiedText')): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text)
    toast(notice)
    return true
  } catch {
    toast(() => t('copyFailed'))
    return false
  }
}
