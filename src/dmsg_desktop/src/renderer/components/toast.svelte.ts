import { temporaryFailure, networkFailure } from '../failure'
import { network } from '../network.svelte'

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
  timer?: ReturnType<typeof setTimeout>
  remaining?: number
}

export const toasts = $state<Toast[]>([])

let last = 0

/** A burst of notices shows its latest two, so they never cover the window. */
const MAX_SHOWN = 2

/** Show a notice for `ms` (4 s by default); with `countdown`, its time left shows beside it. */
export function toast(text: (seconds: number) => string, options: { ms?: number; countdown?: boolean; action?: Toast['action']; testid?: string } = {}) {
  const message = text(0)
  if (!message || network.state !== 'idle' && networkFailure(message)) return
  if (temporaryFailure(message) && toasts.some((toast) => toast.text(0) === message)) return
  while (toasts.length >= MAX_SHOWN) dismiss(toasts[0].id)
  const id = ++last
  const ms = options.ms ?? 4000
  const item: Toast = { id, text, until: 0, ms, countdown: options.countdown ?? false, action: options.action, testid: options.testid }
  schedule(item, ms)
  toasts.push(item)
}

function schedule(item: Toast, ms: number) {
  const id = item.id
  item.until = Date.now() + ms
  item.timer = setTimeout(() => dismiss(id), ms)
}

/** Clipboard notices keep their real clearing time while hovered or focused. */
export function pauseToast(item: Toast) {
  if (item.countdown || item.timer === undefined) return
  clearTimeout(item.timer)
  item.timer = undefined
  item.remaining = Math.max(0, item.until - Date.now())
}

export function resumeToast(item: Toast) {
  if (item.remaining === undefined) return
  schedule(item, item.remaining)
  item.remaining = undefined
}

export function dismiss(id: number) {
  const index = toasts.findIndex((item) => item.id === id)
  if (index < 0) return false
  const item = toasts[index]
  clearTimeout(item.timer)
  item.timer = item.remaining = undefined
  toasts.splice(index, 1)
  return true
}
