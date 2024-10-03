import { errMessage } from '$lib/types/result'
import { tryRun, type TryRunResult } from '$lib/utils/tryrun'
import { type ToastStore } from '@skeletonlabs/skeleton'

export { tryRun, type TryRunResult } from '$lib/utils/tryrun'

export const ErrorLogs: Error[] = []

export function toastRun<T>(
  fn: (signal: AbortSignal, abortingQue: (() => void)[]) => T | Promise<T>,
  toastStore: ToastStore,
  option?: {
    autohide?: boolean
    hideDismiss?: boolean
    background?: string
  }
): TryRunResult<T> {
  return tryRun(fn, (err: any) => {
    if (err) {
      ErrorLogs.push(err)
      if (ErrorLogs.length > 20) {
        ErrorLogs.shift()
      }

      toastStore.trigger({
        timeout: 15000,
        hideDismiss: false,
        background: 'variant-soft-error',
        message: errMessage(err),
        ...option
      })
    }
  })
}
