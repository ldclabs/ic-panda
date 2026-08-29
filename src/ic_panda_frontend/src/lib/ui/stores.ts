import { writable, type Readable } from 'svelte/store'

export interface ModalSettings {
  type: 'component'
  title?: string
  component: {
    ref: any
    props?: Record<string, unknown>
  }
  response?: (value: any) => void
}

export interface ModalStore extends Readable<ModalSettings[]> {
  trigger: (settings: ModalSettings) => void
  close: (value?: unknown) => void
  clear: () => void
}

function createModalStore(): ModalStore {
  const { subscribe, set } = writable<ModalSettings[]>([])
  let pending: ModalSettings['response']

  /**
   * Settles the open modal's response exactly once. Callers await it (see
   * PrizeCard's QR scan), so a modal that goes away without answering has to
   * resolve with undefined rather than leave them pending forever.
   */
  const settle = (value?: unknown) => {
    const response = pending
    pending = undefined
    response?.(value)
  }

  return {
    subscribe,
    trigger: (settings) => {
      settle()
      pending = settings.response
      set([settings])
    },
    close: (value?: unknown) => {
      settle(value)
      set([])
    },
    clear: () => {
      settle()
      set([])
    }
  }
}

export interface ToastSettings {
  id?: number
  message: string
  timeout?: number
  autohide?: boolean
  hideDismiss?: boolean
  background?: string
  classes?: string
  hoverable?: boolean
}

export interface ToastStore extends Readable<ToastSettings[]> {
  trigger: (settings: ToastSettings) => number
  close: (id: number) => void
  clear: () => void
}

function createToastStore(): ToastStore {
  const { subscribe, update, set } = writable<ToastSettings[]>([])
  let nextId = 1

  const close = (id: number) =>
    update((items) => items.filter((item) => item.id !== id))

  return {
    subscribe,
    trigger(settings) {
      const id = nextId++
      const item = { ...settings, id }
      update((items) => [...items, item])
      if (settings.autohide !== false) {
        globalThis.setTimeout(() => close(id), settings.timeout ?? 5000)
      }
      return id
    },
    close,
    clear: () => set([])
  }
}

const modalStore = createModalStore()
const toastStore = createToastStore()

export const getModalStore = () => modalStore
export const getToastStore = () => toastStore
