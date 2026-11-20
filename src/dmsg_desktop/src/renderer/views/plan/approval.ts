import { bridge, call, code } from '../../rpc'
import { rememberPlain, rememberedPlain } from '../../view-memory'

/** Share a prepared approval between the checkout and its order, within the unlocked session. */
export async function reviewApproval(key: string, prepare: () => Promise<{ id: string }>, current: () => boolean) {
  const bucket = 'plan.approval-requests'
  const requests = rememberedPlain<Map<string, Promise<string>>>(bucket) ?? new Map<string, Promise<string>>()
  rememberPlain(bucket, requests)
  let request = requests.get(key)
  if (request) {
    const id = await request
    try {
      const prior = await call<{ state: string; expires_at?: number } | null>('approval/show', { id })
      if (!prior || ['done', 'rejected', 'cancelled', 'expired', 'failed', 'elsewhere'].includes(prior.state) ||
        (prior.state === 'awaiting_user' && prior.expires_at != null && prior.expires_at <= Date.now())) request = undefined
    } catch (cause) { if (code(cause) === 'NOT_FOUND') request = undefined }
    if (!current()) return false
  }
  if (!request) {
    request = prepare().then(value => value.id)
    requests.set(key, request)
  }
  let id: string
  try { id = await request }
  catch (cause) {
    if (requests.get(key) === request) requests.delete(key)
    throw cause
  }
  if (!current()) return false
  await bridge(() => window.dmsg.openApproval(id))
  return current()
}
