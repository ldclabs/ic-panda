import type { CryptoEngine, Progress } from './engine'
import { currentWorkspace, WorkspaceDB } from '../db'
import { id } from '../protocol/codec'
import { isExtension } from '../config'

type Method = Exclude<
  {
    [K in keyof CryptoEngine]: CryptoEngine[K] extends (...args: never[]) => unknown
      ? K
      : never
  }[keyof CryptoEngine],
  'setDocumentId'
>
// Reactive UI proxies are not structured-cloneable. Rebuild only ordinary
// records/arrays while preserving native binary and File values.
function transferable(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(transferable)
  if (value && typeof value === 'object' && Object.getPrototypeOf(value) === Object.prototype)
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => [key, transferable(child)])
    )
  return value
}
export class CryptoClient {
  private worker: Worker | null = null
  private generation = id()
  private sequence = 0
  private documentId: string | undefined
  private pending = new Map<
    number,
    { resolve: (value: any) => void; reject: (error: Error) => void }
  >()
  constructor(readonly onProgress: (progress: Progress) => void = () => {}) {}
  private async prepare(method: Method) {
    if (!isExtension() || !['initialize', 'unlock', 'restore'].includes(method)) return
    const [contexts, tab, currentWindow] = await Promise.all([
      chrome.runtime.getContexts({}),
      chrome.tabs.getCurrent(),
      chrome.windows.getCurrent()
    ])
    const candidates = contexts.filter((context) => {
      if (!context.documentUrl || new URL(context.documentUrl).pathname !== location.pathname)
        return false
      return tab?.id !== undefined
        ? context.tabId === tab.id
        : context.windowId === currentWindow.id
    })
    if (candidates.length === 1) this.documentId = candidates[0].documentId
    // pagehide I/O can be interrupted by a reload. A browser-confirmed missing
    // document permits releasing its old lease immediately; live owners still
    // require their existing lease or an explicit lock. This needs no SW state.
    const name = await currentWorkspace()
    if (name) {
      const db = await WorkspaceDB.open(name)
      try {
        const lease = await db.db.get('meta', 'crypto-owner')
        if (
          lease?.documentId &&
          !contexts.some((context) => context.documentId === lease.documentId)
        )
          await db.release(lease)
      } finally {
        db.db.close()
      }
    }
  }
  async call<K extends Method>(
    method: K,
    ...args: Parameters<CryptoEngine[K]>
  ): Promise<Awaited<ReturnType<CryptoEngine[K]>>> {
    const generation = this.generation
    await this.prepare(method)
    if (generation !== this.generation) throw new Error('LOCKED：会话已结束。')
    if (!this.worker) {
      this.worker = new Worker(new URL('../../crypto-worker.ts', import.meta.url), {
        type: 'module'
      })
      this.worker.onmessage = (event) => {
        const data = event.data
        if (data.generation !== this.generation) return
        if (data.type === 'progress') {
          this.onProgress(data.progress)
          return
        }
        const pending = this.pending.get(data.id)
        if (!pending) return
        this.pending.delete(data.id)
        if (data.ok) pending.resolve(data.value)
        else pending.reject(new Error(data.error))
      }
      this.worker.onerror = () => {
        this.terminate()
        window.dispatchEvent(new Event('dmsg-worker-error'))
      }
    }
    const requestId = ++this.sequence
    return new Promise<Awaited<ReturnType<CryptoEngine[K]>>>((resolve, reject) => {
      this.pending.set(requestId, { resolve, reject })
      try {
        this.worker!.postMessage({
          id: requestId,
          generation: this.generation,
          documentId: this.documentId,
          method,
          args: transferable(args)
        })
      } catch (error) {
        this.pending.delete(requestId)
        reject(error)
      }
    })
  }
  terminate() {
    this.generation = id()
    this.worker?.terminate()
    this.worker = null
    for (const waiter of this.pending.values())
      waiter.reject(new Error('LOCKED：会话已结束。'))
    this.pending.clear()
  }
  async lock() {
    // Drop the worker before touching IDB: slow I/O cannot extend decryption.
    const owner = this.generation
    this.terminate()
    const name = await currentWorkspace()
    if (name) {
      const db = await WorkspaceDB.open(name),
        lease = await db.db.get('meta', 'crypto-owner')
      if (lease?.owner === owner) await db.release(lease)
      db.db.close()
    }
  }
}
