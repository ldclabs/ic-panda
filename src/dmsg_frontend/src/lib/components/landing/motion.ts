// Motion helpers for the landing page. Every animation here is decorative or a
// simulation; with reduced motion the page shows the same states without it.

export function prefersReducedMotion(): boolean {
  return (
    typeof matchMedia === 'function' &&
    matchMedia('(prefers-reduced-motion: reduce)').matches
  )
}
