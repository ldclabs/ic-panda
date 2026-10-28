import { hasLabel, t } from '../../i18n/index.svelte'

export function workflowText(key: string): string {
  const label = `workflow.${key}`
  return hasLabel(label) ? t(label) : key
}
