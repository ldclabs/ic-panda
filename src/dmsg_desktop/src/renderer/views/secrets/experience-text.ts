import { hasLabel, t } from '../../i18n/index.svelte'

export function ux(key: string): string {
  const label = `experience.${key}`
  return t(hasLabel(label) ? label : 'experience.unknown')
}
export function term(key: string) {
  const aliases: Record<string, string> = {
    snapshot_link: 'snapshot',
    snapshot_channel: 'snapshot',
    ssh: 'ssh_auth',
    granted: 'current',
    concealed: 'hidden',
    otp: 'hidden',
    text: 'field',
    url: 'field',
    email: 'field',
    date: 'field',
    month: 'field',
    phone: 'field',
    prepared: 'ready',
    needs_approval: 'awaiting',
    awaiting_user: 'awaiting',
    encrypting: 'pending',
    uploading: 'pending',
    publishing: 'pending',
    revoke_pending: 'pending',
    done: 'complete',
    finished: 'complete',
    scanning: 'running',
    lost: 'unknown'
  }
  return ux(aliases[key] ?? key)
}
