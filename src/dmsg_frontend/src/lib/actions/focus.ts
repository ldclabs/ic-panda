import type { Action } from 'svelte/action'

/**
 * Moves focus to the first focusable control inside `node`.
 *
 * Skeleton's `focusTrap` did two things: it kept focus inside the element and
 * it focused the first field. The dialogs these forms live in are bits-ui
 * `Dialog.Content`, which already traps focus, so only the second half is
 * still ours to do.
 */
export const autoFocus: Action<HTMLElement, boolean | undefined> = (
  node,
  enabled = true
) => {
  if (enabled === false) return

  const focusable = node.querySelector<HTMLElement>(
    'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
  )
  focusable?.focus()
}
