import { t } from '../i18n/index.svelte'
import { toast } from './toast.svelte'

/**
 * Copy text the window already shows, such as a user name or an invitation;
 * secrets go through the main process instead. `notice: null` leaves the
 * success to the control that copied, which shows its own check.
 */
export async function copyText(text: string, notice: (() => string) | null = () => t('copiedText')): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text)
    if (notice) toast(notice)
    return true
  } catch {
    toast(() => t('copyFailed'))
    return false
  }
}

export async function copyShareLink(url: string) {
  try {
    await window.dmsg.copyShareLink(url)
    toast(() => t('copiedText'))
  } catch {
    toast(() => t('copyFailed'))
  }
}
