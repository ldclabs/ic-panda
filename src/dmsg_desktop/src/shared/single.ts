/**
 * `work`, one run at a time: a call while it runs asks for one more run
 * after it, however many calls came. All callers receive the last run's outcome. A
 * view reloads with it on each daemon event, so a burst of events reads
 * the daemon twice at most and an older read never lands after a newer one.
 */
export function single<A extends unknown[]>(work: (...args: A) => Promise<unknown>): (...args: A) => Promise<void> {
  let running: Promise<void> | null = null
  let again = false
  let next: A
  async function drain() {
    try {
      do {
        again = false
        try {
          await work(...next)
        } catch (error) {
          if (!again) throw error
        }
      } while (again)
    } finally {
      running = null
    }
  }

  return (...args: A) => {
    next = args
    if (running) {
      again = true
      return running
    }
    // Publish the shared promise before work starts, including synchronous callbacks or throws.
    let start!: () => void
    const result = new Promise<void>((resolve, reject) => {
      start = () => { void drain().then(resolve, reject) }
    })
    running = result
    start()
    return result
  }
}
