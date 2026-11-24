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

export interface IndicatorTarget {
  /** The chosen item inside the indicator's parent, or `null` for none. */
  selector: string | null
  /** `fill` covers the item (a segmented thumb); `bottom` sits on its lower edge (a tab underline). */
  edge?: 'fill' | 'bottom'
  /** Pulls the indicator in from both sides of the item. */
  inset?: number
}

/**
 * Keeps an indicator, such as a segmented control's thumb or a tab bar's
 * underline, under the chosen item of its positioned parent. A new choice
 * slides it there through its CSS transition; the first placement, a resize
 * and reduced motion put it there at once.
 */
export function indicator(node: HTMLElement, target: IndicatorTarget) {
  const container = node.parentElement!
  let item: HTMLElement | null = null
  const resized = new ResizeObserver(() => place(false))
  const place = (glide: boolean) => {
    const next = target.selector ? container.querySelector<HTMLElement>(target.selector) : null
    if (next !== item) {
      if (item) resized.unobserve(item)
      if (next) resized.observe(next)
      if (!item) glide = false
      item = next
    }
    node.hidden = !item
    if (!item) return
    const inset = target.inset ?? 0
    const top = target.edge === 'bottom' ? item.offsetTop + item.offsetHeight - node.offsetHeight : item.offsetTop
    const jump = !glide || still()
    if (jump) node.style.transition = 'none'
    node.style.width = `${item.offsetWidth - 2 * inset}px`
    if (target.edge !== 'bottom') node.style.height = `${item.offsetHeight}px`
    node.style.transform = `translate(${item.offsetLeft + inset}px, ${top}px)`
    if (jump) {
      void node.offsetWidth
      node.style.transition = ''
    }
  }
  resized.observe(container)
  place(false)
  return {
    update(next: IndicatorTarget) {
      const moved = next.selector !== target.selector
      target = next
      place(moved)
    },
    destroy() {
      resized.disconnect()
    }
  }
}
