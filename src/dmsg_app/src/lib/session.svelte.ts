import { CryptoClient } from './crypto/client'
import type { Progress } from './crypto/engine'
import type { ViewData, WorkspaceMeta } from './models'
import { AUTO_LOCK_MS } from './config'
import { errorText } from './errors'

const empty = (): ViewData => ({
  meta: null,
  entries: [],
  channels: [],
  messages: [],
  profile: null,
  outbox: [],
  conflicts: [],
  imports: []
})
export class Session {
  initialized = $state(false)
  loaded = $state(false)
  unlocked = $state(false)
  lockEpoch = $state(0)
  busy = $state(false)
  message = $state('')
  error = $state('')
  progress = $state<Progress | null>(null)
  meta = $state<WorkspaceMeta | null>(null)
  data = $state<ViewData>(empty())
  private generation = 0
  private touchedAt = Date.now()
  private channel = new BroadcastChannel('dmsg-session')
  private timer: ReturnType<typeof setInterval> | null = null
  private client = new CryptoClient((progress) => {
    this.progress = progress
  })
  get crypto() {
    return this.client
  }
  async start() {
    const status = await this.client.call('status')
    this.initialized = status.exists
    this.meta = status.meta
    this.loaded = true
    this.channel.onmessage = (event) => {
      if (event.data === 'lock') void this.lock(false)
    }
    window.addEventListener('pagehide', () => {
      // Closing this page ends its dedicated worker. It must not lock a
      // different page that acquired a newer lease while this page was hidden.
      void this.lock(false)
    })
    window.addEventListener('dmsg-worker-error', () => {
      void this.lock()
      this.error = '密码进程已停止，请重新解锁。'
    })
    window.addEventListener('dmsg-lock', () => {
      void this.lock()
    })
    document.addEventListener('visibilitychange', () => {
      if (this.unlocked && Date.now() - this.touchedAt >= AUTO_LOCK_MS) void this.lock()
    })
    this.timer = setInterval(() => {
      if (!this.unlocked) return
      if (Date.now() - this.touchedAt >= AUTO_LOCK_MS) {
        void this.lock()
        return
      }
      void this.client.call('tick').catch(() => {
        void this.lock()
      })
    }, 5000)
  }
  touch() {
    if (this.unlocked) this.touchedAt = Date.now()
  }
  activate(meta: WorkspaceMeta) {
    this.meta = meta
    this.initialized = true
    this.unlocked = true
    this.touch()
  }
  async unlock(password: string) {
    const generation = this.generation
    try {
      const meta = await this.client.call('unlock', password),
        data = await this.client.call('view')
      if (generation !== this.generation) return
      this.data = data
      this.activate(data.meta ?? meta)
    } catch (error) {
      if (generation === this.generation) await this.client.lock()
      throw error
    }
  }
  async refresh() {
    if (!this.unlocked) return
    const generation = this.generation,
      data = await this.client.call('view')
    if (generation === this.generation && this.unlocked) {
      this.data = data
      this.meta = data.meta
    }
  }
  async lock(broadcast = true) {
    this.generation++
    this.lockEpoch++
    this.unlocked = false
    this.data = empty()
    this.progress = null
    this.error = ''
    this.message = ''
    this.busy = false
    window.dispatchEvent(new Event('dmsg-cleared'))
    if (broadcast) this.channel.postMessage('lock')
    await this.client.lock()
  }
  async run<T>(task: () => Promise<T>, success = ''): Promise<T | undefined> {
    if (this.busy) return
    const generation = this.generation
    this.busy = true
    this.error = ''
    this.message = ''
    this.progress = null
    this.touch()
    try {
      const result = await task()
      if (generation === this.generation) {
        this.message = success
        return result
      }
    } catch (error) {
      if (generation === this.generation) this.error = errorText(error)
    } finally {
      if (generation === this.generation) {
        this.busy = false
        this.progress = null
      }
    }
  }
}
export const session = new Session()

const downloads = new Set<string>()
export function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob),
    link = document.createElement('a')
  downloads.add(url)
  link.href = url
  link.download = filename
  link.click()
  setTimeout(() => {
    URL.revokeObjectURL(url)
    downloads.delete(url)
  }, 10000)
}
window.addEventListener('dmsg-cleared', () => {
  for (const url of downloads) URL.revokeObjectURL(url)
  downloads.clear()
})
export function formatBytes(size: number) {
  return size < 1024
    ? `${size} B`
    : size < 1024 * 1024
      ? `${(size / 1024).toFixed(1)} KiB`
      : `${(size / 1024 / 1024).toFixed(1)} MiB`
}
export function dateLabel(date: number) {
  return new Date(date).toLocaleString('zh-CN', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  })
}
export function shortId(value: string) {
  return `${value.slice(0, 8)}…${value.slice(-6)}`
}
