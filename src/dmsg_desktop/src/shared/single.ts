/**
 * `work`, one run at a time: a call while it runs asks for one more run
 * after it, however many calls came, and resolves when that run ends. A
 * view reloads with it on each daemon event, so a burst of events reads
 * the daemon twice at most and an older read never lands after a newer one.
 */
export function single<A extends unknown[]>(work: (...args: A) => Promise<unknown>): (...args: A) => Promise<void> {
  let running: Promise<void> | null = null
  let again = false
  let next: A
  return (...args: A) => {
    next = args
    if (running) {
      again = true
      return running
    }
    running = (async () => {
      try {
        do {
          again = false
          await work(...next)
        } while (again)
      } finally {
        running = null
      }
    })()
    return running
  }
}
