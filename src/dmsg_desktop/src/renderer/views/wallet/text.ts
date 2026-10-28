import { hasLabel, t } from '../../i18n/index.svelte'

export function w(key: string): string {
  const label = `wallet.${key}`
  return hasLabel(label) ? t(label) : key
}
