import { errMessage } from '$lib/types/result'
import { tryRun, type TryRunResult } from '$lib/utils/tryrun'
import { type ToastStore } from '@skeletonlabs/skeleton'

export { tryRun, type TryRunResult } from '$lib/utils/tryrun'

export function toastRun<T>(
  fn: (signal: AbortSignal) => T | Promise<T>,
  toastStore: ToastStore,
  option?: {
    autohide?: boolean
    hideDismiss?: boolean
    background?: string
  }
): TryRunResult<T> {
  return tryRun(fn, (err: any) => {
    toastStore.trigger({
      autohide: true,
      hideDismiss: false,
      background: 'variant-filled-error',
      message: errMessage(err),
      ...option
    })
  })
}
