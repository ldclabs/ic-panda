// Motion helpers for the landing page. Every animation here is decorative or a
// simulation; with reduced motion the page shows the same states without it.

export function prefersReducedMotion(): boolean {
  return (
    typeof matchMedia === 'function' &&
    matchMedia('(prefers-reduced-motion: reduce)').matches
  )
}

/**
 * Fades an element up once, the first time it enters the viewport. What is
 * already on screen when the page opens, and everything under reduced
 * motion, is left alone.
 */
export function reveal(node: HTMLElement) {
  if (prefersReducedMotion() || typeof IntersectionObserver !== 'function')
    return
  if (node.getBoundingClientRect().top < innerHeight) return
  node.classList.add('reveal')
  const observer = new IntersectionObserver(
    ([entry]) => {
      if (!entry?.isIntersecting) return
      node.classList.add('revealed')
      observer.disconnect()
    },
    { rootMargin: '0px 0px -8% 0px' }
  )
  observer.observe(node)
  return { destroy: () => observer.disconnect() }
}
