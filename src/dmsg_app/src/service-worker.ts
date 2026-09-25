import { externalPort } from './lib/external-port'
import { flushCipherDispatches } from './lib/services/background'
import { config } from './lib/config'
import { currentWorkspace, WorkspaceDB } from './lib/db'
import { expireRequests, getRequest } from './lib/requests'
import { trustedSource } from './lib/protocol/requests'
import { ensure } from './lib/errors'
import { hash } from './lib/protocol/codec'
import type { SourceBinding } from './lib/protocol/requests'

const sourceKey = (source: SourceBinding) =>
  `${source.origin}:${source.tabId}:${source.documentId}`
const liveSources = new Map<string, Set<chrome.runtime.Port>>()
const challenges = new Map<
  string,
  { port: chrome.runtime.Port; done: (live: boolean) => void }
>()
function challenge(port: chrome.runtime.Port) {
  const nonce = hash(crypto.getRandomValues(new Uint8Array(32)))
  return new Promise<boolean>((resolve) => {
    const timer = setTimeout(() => {
      challenges.delete(nonce)
      resolve(false)
    }, 2500)
    challenges.set(nonce, {
      port,
      done: (live) => {
        clearTimeout(timer)
        challenges.delete(nonce)
        resolve(live)
      }
    })
    try {
      port.postMessage({ method: 'source.challenge', nonce })
    } catch {
      challenges.get(nonce)?.done(false)
    }
  })
}
chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (
    message?.type !== 'dmsg-source-check' ||
    sender.id !== chrome.runtime.id ||
    !sender.url?.startsWith(chrome.runtime.getURL(''))
  )
    return
  void (async () => {
    const request = await getRequest(message.requestId)
    if (!request) return { ok: false }
    if (request.source.origin === `chrome-extension://${chrome.runtime.id}`) {
      const contexts = await chrome.runtime.getContexts({})
      return {
        ok: contexts.some(
          (c) => c.documentId === request.source.documentId && c.tabId === request.source.tabId
        )
      }
    }
    const port = liveSources.get(sourceKey(request.source))?.values().next().value
    return { ok: !!port && (await challenge(port)) }
  })().then(respond, () => respond({ ok: false }))
  return true
})

async function lockWorkspace() {
  const name = await currentWorkspace()
  if (name) {
    const db = await WorkspaceDB.open(name)
    await db.invalidate()
    db.db.close()
  }
  await chrome.action.setBadgeText({ text: '' })
}
async function refresh() {
  const count = await expireRequests()
  await chrome.action.setBadgeBackgroundColor({ color: '#145C45' })
  await chrome.action.setBadgeText({ text: count ? String(count) : '' })
}
// Operations survive a worker restart; each new connection proves its original key.
void refresh()

// No keepalive loop and no decryption keys. Alarms resume bounded metadata jobs.
chrome.runtime.onInstalled.addListener(() => {
  void lockWorkspace()
  void chrome.alarms.create('dmsg-resume', { periodInMinutes: 1 })
})
chrome.runtime.onStartup.addListener(() => {
  void chrome.alarms.create('dmsg-resume', { periodInMinutes: 1 })
  void lockWorkspace()
  void refresh()
})
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === 'dmsg-resume') void refresh()
})
chrome.runtime.onConnectExternal.addListener((port) => {
  let source: SourceBinding
  try {
    ensure(port.name === 'dmsg-extension/4', 'UNSUPPORTED_PROTOCOL')
    source = trustedSource(
      port.sender ?? {},
      config.externalOrigins,
      config.environment === 'local'
    )
  } catch {
    port.disconnect()
    return
  }
  const key = sourceKey(source),
    ports = liveSources.get(key) ?? new Set<chrome.runtime.Port>()
  ports.add(port)
  liveSources.set(key, ports)
  port.onMessage.addListener((message: unknown) => {
    if (
      message &&
      typeof message === 'object' &&
      'method' in message &&
      message.method === 'source.reply'
    ) {
      const nonce = (message as { nonce?: string }).nonce
      const pending = typeof nonce === 'string' ? challenges.get(nonce) : null
      if (pending?.port === port) pending.done(true)
    }
  })
  externalPort(port, source, refresh)
  port.onDisconnect.addListener(() => {
    for (const pending of challenges.values()) if (pending.port === port) pending.done(false)
    ports.delete(port)
    if (!ports.size) liveSources.delete(key)
  })
})

let dispatching: Promise<void> | null = null
async function flushCiphertext() {
  if (dispatching || !config.relayOrigin) return dispatching
  dispatching = (async () => {
    const name = await currentWorkspace()
    if (!name) return
    const db = await WorkspaceDB.open(name)
    try {
      await flushCipherDispatches(db, config.relayOrigin)
    } finally {
      db.db.close()
    }
  })().finally(() => {
    dispatching = null
  })
  return dispatching
}
chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (
    message?.type !== 'dmsg-flush-ciphertext' ||
    sender.id !== chrome.runtime.id ||
    !sender.url?.startsWith(chrome.runtime.getURL(''))
  )
    return
  void flushCiphertext().then(
    () => respond({ ok: true }),
    () => respond({ ok: false })
  )
  return true
})
// Failed jobs stay recorded on their rows; the next alarm retries them.
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === 'dmsg-cipher-dispatch') void flushCiphertext().catch(() => {})
})
void chrome.alarms.create('dmsg-cipher-dispatch', { periodInMinutes: 0.5 })
void flushCiphertext().catch(() => {})
