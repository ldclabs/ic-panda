// Motion helpers for the landing page. Every animation here is decorative or a
// simulation; with reduced motion the page shows the same states without it.

export function prefersReducedMotion(): boolean {
  return (
    typeof matchMedia === 'function' &&
    matchMedia('(prefers-reduced-motion: reduce)').matches
  )
}

/** Calls `change` when the node enters or leaves the viewport. */
export function watchVisibility(
  node: Element,
  change: (visible: boolean) => void,
  rootMargin = '80px'
): () => void {
  if (typeof IntersectionObserver !== 'function') {
    change(true)
    return () => {}
  }
  const observer = new IntersectionObserver(
    (entries) => change(entries.some((entry) => entry.isIntersecting)),
    { rootMargin }
  )
  observer.observe(node)
  return () => observer.disconnect()
}

/**
 * Adds `is-in` once the node scrolls into view. Pages that are hidden, or
 * that ask for reduced motion, show the content at once.
 */
export function reveal(node: HTMLElement) {
  if (prefersReducedMotion() || document.visibilityState === 'hidden') {
    node.classList.add('is-in')
    return {}
  }
  node.classList.add('reveal')
  const stop = watchVisibility(
    node,
    (visible) => {
      if (!visible) return
      node.classList.add('is-in')
      stop()
    },
    '0px 0px -8% 0px'
  )
  return { destroy: stop }
}

/**
 * Runs `frame(dt)` on animation frames while `active()` holds and the
 * document is visible; returns a function that wakes or stops the loop.
 */
export function frameLoop(
  frame: (dt: number) => void,
  active: () => boolean
): { wake: () => void; stop: () => void } {
  let handle = 0
  let last = 0
  let stopped = false
  const tick = (now: number) => {
    handle = 0
    if (stopped || !active() || document.visibilityState === 'hidden') return
    // Cap a step so a throttled tab does not jump the simulation.
    const dt = last ? Math.min((now - last) / 1000, 0.05) : 0
    last = now
    frame(dt)
    handle = requestAnimationFrame(tick)
  }
  const wake = () => {
    if (stopped || handle || !active()) return
    last = 0
    handle = requestAnimationFrame(tick)
  }
  const onVisibility = () => wake()
  document.addEventListener('visibilitychange', onVisibility)
  return {
    wake,
    stop() {
      stopped = true
      if (handle) cancelAnimationFrame(handle)
      document.removeEventListener('visibilitychange', onVisibility)
    }
  }
}

/** Splits a trailing full stop so it can take the accent colour. */
export function splitStop(text: string): [string, string] {
  const last = text.at(-1)
  return last === '.' || last === '。' ? [text.slice(0, -1), last] : [text, '']
}
