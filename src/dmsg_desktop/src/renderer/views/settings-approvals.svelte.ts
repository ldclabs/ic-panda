import { call, code } from '../rpc'
import { rememberPlain, rememberedPlain } from '../view-memory'

/** Keep the original approval IDs while this account is unlocked, including across Settings pages. */
export function settingsApprovals<Key extends string>(page: string, status: { account: string; session?: string }) {
  const key = `settings.approvals.${page}:${status.account}:${status.session ?? ''}`
  const existing = rememberedPlain<Partial<Record<Key, string>>>(key)
  if (existing) return existing
  const requests = $state<Partial<Record<Key, string>>>({})
  rememberPlain(key, requests)
  return requests
}

export async function refreshSettingsApprovals<Key extends string>(requests: Partial<Record<Key, string>>, active: () => boolean) {
  for (const key of Object.keys(requests) as Key[]) {
    if (!active()) return
    const id = requests[key]
    let finished: boolean
    try {
      const request = await call<{ state: string }>('approval/show', { id })
      finished = ['done', 'rejected', 'cancelled', 'expired', 'failed', 'elsewhere'].includes(request?.state)
    } catch (error) { finished = code(error) === 'NOT_FOUND' }
    if (active() && finished && requests[key] === id) delete requests[key]
  }
}
