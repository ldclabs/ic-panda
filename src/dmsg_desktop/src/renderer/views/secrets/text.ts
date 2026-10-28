import { hasLabel, t } from '../../i18n/index.svelte'

export function secretText(key: string): string {
  const label = `secret.${key}`
  return hasLabel(label) ? t(label) : key
}
