import { backOut, cubicOut } from 'svelte/easing'
import type { TransitionConfig } from 'svelte/transition'

/**
 * The window's small motions (DESIGN.md, Motion), all 120–200ms. Under
 * reduced motion each one ends at once: CSS animations by the global rule in
 * `app.css`, these by asking at the moment they start.
 */
export const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches

/** `--ease-calm` in `app.css`, for the Web Animations API. */
const calm = 'cubic-bezier(0.22, 1, 0.36, 1)'

/**
 * Fades the node's content in each time `key` changes (another item in a
 * detail, another view), without remounting it. The first value shows at once.
 */
export function arrive(node: HTMLElement, key: unknown) {
  return {
    update(next: unknown) {
      if (Object.is(next, key)) return
      key = next
      if (!still()) node.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 160, easing: calm })
    }
  }
}

/** A row that joins a list already on screen fades in; in a list of `rows` past 100 it shows at once. */
export function enter(_node: Element, rows = 0): TransitionConfig {
  return { duration: still() || rows >= 100 ? 0 : 160, easing: cubicOut, css: (t) => `opacity: ${t}` }
}

/** A notice that leaves fades and drops a little. */
export function leave(_node: Element): TransitionConfig {
  return { duration: still() ? 0 : 140, easing: cubicOut, css: (t) => `opacity: ${t}; transform: translateY(${(1 - t) * 6}px)` }
}

/** A mark that changed (the favourite star, a count) pops once. */
export function pop(_node: Element): TransitionConfig {
  return { duration: still() ? 0 : 200, easing: backOut, css: (t) => `transform: scale(${0.6 + 0.4 * t})` }
}

/** Reordering a short list moves its rows; a long one, or reduced motion, jumps. */
export const flipDuration = (rows: number) => (still() || rows >= 100 ? 0 : 180)
