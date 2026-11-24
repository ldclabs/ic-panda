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
}

export const toasts = $state<Toast[]>([])

let last = 0

/** A burst of notices shows its latest two, so they never cover the window. */
const MAX_SHOWN = 2

/** While the pointer rests on the notices they wait; a countdown keeps running, since the clipboard clears on time. */
let pausedAt: number | null = null

/** Show a notice for `ms` (4 s by default); with `countdown`, its time left shows beside it. */
export function toast(text: (seconds: number) => string, options: { ms?: number; countdown?: boolean; action?: Toast['action']; testid?: string } = {}) {
  if (network.state !== 'idle' && networkFailure(text(0))) return
  if (temporaryFailure(text(0)) && toasts.some((toast) => toast.text(0) === text(0))) return
  while (toasts.length >= MAX_SHOWN) toasts.shift()
  const id = ++last
  const ms = options.ms ?? 4000
  toasts.push({ id, text, until: Date.now() + ms, ms, countdown: options.countdown ?? false, action: options.action, testid: options.testid })
  setTimeout(() => expire(id), ms)
}

function expire(id: number) {
  const item = toasts.find((toast) => toast.id === id)
  if (!item) return
  const held = pausedAt !== null && !item.countdown
  const left = item.until - Date.now()
  if (held || left > 0) setTimeout(() => expire(id), held ? 250 : left)
  else dismiss(id)
}

export function pauseToasts() {
  pausedAt ??= Date.now()
}

export function resumeToasts() {
  if (pausedAt === null) return
  const held = Date.now() - pausedAt
  pausedAt = null
  for (const item of toasts) if (!item.countdown) item.until += held
}

export function dismiss(id: number) {
  const index = toasts.findIndex((item) => item.id === id)
  if (index >= 0) toasts.splice(index, 1)
}
