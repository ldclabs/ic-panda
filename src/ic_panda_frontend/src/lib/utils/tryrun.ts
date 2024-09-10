export interface TryRunResult<T> {
  abort: () => void
  finally: (onfinally?: (res: T | null) => void) => Promise<void>
}

export function tryRun<T>(
  fn: (signal: AbortSignal) => T | Promise<T>,
  onerror?: (err: any) => void
): TryRunResult<T> {
  const controller = new AbortController()
  const rt = (async () => {
    try {
      return await fn(controller.signal)
    } catch (err: any) {
      if (onerror) {
        onerror(err)
      } else {
        console.error(err)
      }
      return null
    }
  })()

  return {
    abort: () => controller.abort(),
    finally: (onfinally) => rt.then((res) => onfinally && onfinally(res))
  }
}
