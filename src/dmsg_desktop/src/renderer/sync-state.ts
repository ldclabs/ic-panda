import type { Label } from './i18n/messages/en.ts'

/** Incoming sync and outgoing limits are separate; a failure never hides local-only work. */
export function syncState(status: any, now = Date.now()) {
  const last = status?.last_sync
  const error: string | undefined = last?.error ?? last?.upload_error ?? undefined
  const pending = Number(status?.pending_sync ?? 0)
  const paused = status?.state !== 'unlocked'
  const syncing = !!status?.syncing
  const initial = status?.sync_initialized === false
  const progress = status?.sync_progress
  const stage: Label = ({ export_start: 'syncInventory', export_page: 'syncInventory', delta_page: 'syncInventory', download: 'syncDownload', apply: 'syncVerify' } as Record<string, Label>)[progress?.stage] ?? 'syncing'
  const coolingDown = error === 'SYNC_BUSY' && Number(last?.retry_at ?? 0) > now
  const transient = ['UNAVAILABLE', 'NETWORK_UNAVAILABLE', 'EXECUTION_UNKNOWN', 'SYNC_BUSY', 'POLICY_STALE', 'RESULT_EXPIRED'].includes(error ?? '')
  const okAt: number | undefined = last?.ok_at ?? (!error ? last?.at : undefined)
  let label: Label = syncing ? initial ? 'syncInitial' : 'syncing' : paused ? 'locked' : initial ? 'syncInitialWaiting' : pending ? 'syncPending' : okAt ? 'accountSynced' : 'sync'
  if (!syncing && !paused && error) {
    label = error === 'QUOTA_EXCEEDED' ? 'syncStorageBlocked'
      : error === 'SYNC_BUSY' ? 'syncBusy'
      : ['REQUEST_TOO_LARGE', 'RESPONSE_TOO_LARGE'].includes(error) ? 'syncSizeError'
      : error === 'UNSUPPORTED_PROTOCOL' ? 'syncUnsupported'
      : transient ? 'networkUnavailable' : 'syncFailed'
  }
  return { initial, progress, stage, coolingDown, label, pending, okAt, retryAt: paused ? undefined : last?.retry_at, failed: !!error && !syncing && !paused, transient, paused }
}
