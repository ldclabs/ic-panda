import { backOut, cubicOut } from 'svelte/easing'
import type { TransitionConfig } from 'svelte/transition'

/**
 * The window's small motions (DESIGN.md, Motion; docs/motion.zh.md), 120–320ms. Under
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

/** A badge that clears shrinks away, a little faster than it came. */
export function shrink(_node: Element): TransitionConfig {
  return { duration: still() ? 0 : 140, easing: cubicOut, css: (t) => `opacity: ${t}; transform: scale(${0.6 + 0.4 * t})` }
}

/**
 * A digit of a count that changed rolls in: from below when the count grew,
 * from above when it shrank. The old digit is gone at once, so every frame
 * shows the true value. `quiet` swaps it without rolling.
 */
export function roll(_node: Element, { up = true, quiet = false }: { up?: boolean; quiet?: boolean } = {}): TransitionConfig {
  return { duration: still() || quiet ? 0 : 180, easing: cubicOut, css: (t) => `opacity: ${t}; transform: translateY(${(1 - t) * (up ? 0.5 : -0.5)}em)` }
}

/** A field whose value was refused shakes once, as a Mac password field does. */
export function shake(node: Element) {
  if (still()) return
  node.animate([0, -6, 5, -3, 2, 0].map((x) => ({ transform: `translateX(${x}px)` })), { duration: 320 })
}

/**
 * A secret being shown develops from a blur; hiding it is instant and stops
 * a develop under way.
 */
export function develop(node: HTMLElement, shown: boolean) {
  let run: Animation | undefined
  return {
    update(next: boolean) {
      if (next === shown) return
      shown = next
      run?.cancel()
      if (shown && !still()) run = node.animate([{ filter: 'blur(6px)', opacity: 0.35 }, { filter: 'none', opacity: 1 }], { duration: 180, easing: calm })
    },
    destroy() {
      run?.cancel()
    }
  }
}

/**
 * What a jump landed on lights once (900ms, Warn pale), or a field just
 * saved in place (700ms, Pale), over its own background.
 */
export function flash(node: Element | null | undefined, saved = false) {
  if (!node || still()) return
  const color = getComputedStyle(document.documentElement).getPropertyValue(saved ? '--pale' : '--warn-pale').trim()
  node.animate([{ boxShadow: `inset 0 0 0 100vmax ${color}` }, { boxShadow: `inset 0 0 0 100vmax ${color}`, offset: 0.3 }, { boxShadow: 'inset 0 0 0 100vmax transparent' }], {
    duration: saved ? 700 : 900,
    easing: 'ease-out'
  })
}

/**
 * Scrolls to what a jump landed on and lights it once, waiting up to 2s for
 * a list that is still loading to show it.
 */
export function land(selector: string) {
  const found = () => {
    const node = document.querySelector(selector)
    if (!node) return false
    node.scrollIntoView({ block: 'nearest' })
    flash(node)
    return true
  }
  if (found()) return
  const watch = new MutationObserver(() => found() && stop())
  const timer = setTimeout(() => stop(), 2000)
  const stop = () => (watch.disconnect(), clearTimeout(timer))
  watch.observe(document.body, { childList: true, subtree: true })
}

/**
 * A sidebar label that opens with the sidebar waits for most of its width,
 * then fades in, on one line until it is in.
 */
export function follow(_node: Element): TransitionConfig {
  if (still()) return { duration: 0 }
  return { delay: 120, duration: 120, easing: cubicOut, css: (t) => `opacity: ${t}; white-space: nowrap` }
}

// A view that just opened shows its notices at once; one that comes later unfolds.
let opened = performance.now()
addEventListener('hashchange', () => (opened = performance.now()))

/** A notice that comes after its view opened unfolds its height and pushes what is below down. */
export function fold(node: HTMLElement): TransitionConfig {
  if (still() || performance.now() - opened < 400) return { duration: 0 }
  const height = node.offsetHeight
  const { paddingTop, paddingBottom } = getComputedStyle(node)
  const [top, bottom] = [parseFloat(paddingTop), parseFloat(paddingBottom)]
  return {
    duration: 220,
    easing: cubicOut,
    css: (t) => `overflow: hidden; height: ${t * height}px; padding-top: ${t * top}px; padding-bottom: ${t * bottom}px; opacity: ${t}`
  }
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
