/** A short notice in the corner of the main window. */
export interface Toast {
  id: number
  /** The notice, given the seconds it has left. */
  text: (seconds: number) => string
  /** When it goes, in Unix milliseconds. */
  until: number
  /** How long it shows, for its countdown. */
  ms: number
  /** It shows how long it has left, such as a copy leaving the clipboard. */
  countdown: boolean
  /** A button beside it, such as undoing what it reports; pressing it dismisses the notice. */
  action?: { label: string; run: () => unknown }
  testid?: string
}

export const toasts = $state<Toast[]>([])

let last = 0

/** Show a notice for `ms` (4 s by default); with `countdown`, its time left shows beside it. */
export function toast(text: (seconds: number) => string, options: { ms?: number; countdown?: boolean; action?: Toast['action']; testid?: string } = {}) {
  const id = ++last
  const ms = options.ms ?? 4000
  toasts.push({ id, text, until: Date.now() + ms, ms, countdown: options.countdown ?? false, action: options.action, testid: options.testid })
  setTimeout(() => dismiss(id), ms)
}

export function dismiss(id: number) {
  const index = toasts.findIndex((item) => item.id === id)
  if (index >= 0) toasts.splice(index, 1)
}
