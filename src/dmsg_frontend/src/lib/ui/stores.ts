import { writable, type Readable } from 'svelte/store'

/**
 * Local replacement for Skeleton's modal and toast stores.
 *
 * The shape is deliberately the one the application already calls
 * (`trigger` / `close` / `clear`, `$modalStore[0]`), so the migration to
 * bits-ui stayed inside `lib/components/ui` instead of rippling through every
 * caller. `ModalHost` and `ToastHost` render what these hold.
 */

export interface ModalComponent {
  /** The Svelte component to render inside the dialog. */
  ref: any
  /** Props forwarded to that component. */
  props?: Record<string, unknown>
}

export interface ModalSettings {
  type?: 'component'
  title?: string
  component: ModalComponent
  /** Settled once, with whatever `close` was given. */
  response?: (value: unknown) => void
}

export interface ModalStore extends Readable<ModalSettings[]> {
  /** Queue a modal behind any that are already open. */
  trigger: (modal: ModalSettings) => void
  /** Open a modal on top, revealing the current one again when it closes. */
  triggerFirst: (modal: ModalSettings) => void
  /** Dismiss the visible modal. */
  close: (value?: unknown) => void
  clear: () => void
}

function createModalStore(): ModalStore {
  const { subscribe, update, set } = writable<ModalSettings[]>([])

  /**
   * Settles a modal's response exactly once. A modal that goes away without
   * an explicit value still has to resolve, or callers that await it would
   * stay pending forever.
   */
  const settle = (modal: ModalSettings | undefined, value?: unknown) => {
    const response = modal?.response
    if (response) {
      delete modal.response
      response(value)
    }
  }

  return {
    subscribe,
    trigger: (modal) => update((stack) => [...stack, modal]),
    triggerFirst: (modal) => update((stack) => [modal, ...stack]),
    close: (value?: unknown) =>
      update((stack) => {
        settle(stack[0], value)
        return stack.slice(1)
      }),
    clear: () => {
      update((stack) => {
        stack.forEach((modal) => settle(modal))
        return stack
      })
      set([])
    }
  }
}

export interface ToastAction {
  label: string
  response: () => void
}

export interface ToastSettings {
  id?: number
  message: string
  /** Dismiss automatically. Defaults to true. */
  autohide?: boolean
  /** Milliseconds before auto-dismissal. Defaults to 5000. */
  timeout?: number
  /** Hide the dismiss button. */
  hideDismiss?: boolean
  /** Pause auto-dismissal while hovered. */
  hoverable?: boolean
  /** A `variant-*` class naming the tone, as Skeleton used it. */
  background?: string
  /** Extra classes for the toast body. */
  classes?: string
  action?: ToastAction
  callback?: (payload: { id: number; status: 'queued' | 'closed' }) => void
}

export interface ToastStore extends Readable<ToastSettings[]> {
  trigger: (toast: ToastSettings) => number
  close: (id: number) => void
  /** Stops the auto-dismiss countdown, for `hoverable` toasts. */
  pause: (id: number) => void
  /** Restarts the countdown from the beginning. */
  resume: (id: number) => void
  clear: () => void
}

function createToastStore(): ToastStore {
  const { subscribe, update, set } = writable<ToastSettings[]>([])
  const timers = new Map<number, ReturnType<typeof setTimeout>>()
  const pending = new Map<number, ToastSettings>()
  let nextId = 1

  function clearTimer(id: number) {
    const timer = timers.get(id)
    if (timer !== undefined) {
      globalThis.clearTimeout(timer)
      timers.delete(id)
    }
  }

  function schedule(toast: ToastSettings) {
    const id = toast.id!
    clearTimer(id)
    if (toast.autohide === false) return
    timers.set(
      id,
      globalThis.setTimeout(() => close(id), toast.timeout ?? 5000)
    )
  }

  function close(id: number) {
    clearTimer(id)
    const toast = pending.get(id)
    pending.delete(id)
    update((toasts) => toasts.filter((item) => item.id !== id))
    toast?.callback?.({ id, status: 'closed' })
  }

  return {
    subscribe,
    trigger(settings) {
      const id = nextId++
      const toast: ToastSettings = { ...settings, id }
      // With no dismiss button the timer is the only way out, so it has to run.
      if (toast.hideDismiss) toast.autohide = true
      pending.set(id, toast)
      update((toasts) => [...toasts, toast])
      toast.callback?.({ id, status: 'queued' })
      schedule(toast)
      return id
    },
    close,
    pause: clearTimer,
    resume: (id) => {
      const toast = pending.get(id)
      if (toast) schedule(toast)
    },
    clear: () => {
      timers.forEach((timer) => globalThis.clearTimeout(timer))
      timers.clear()
      pending.clear()
      set([])
    }
  }
}

const modalStore = createModalStore()
const toastStore = createToastStore()

export const getModalStore = (): ModalStore => modalStore
export const getToastStore = (): ToastStore => toastStore
