import { flushCipherDispatches } from './lib/services/background'
import { config } from './lib/config'
import { currentWorkspace, WorkspaceDB } from './lib/db'
import { enqueueRequest, listRequests, rejectRequest, setRequestState } from './lib/requests'
import { sameSource, trustedSource } from './lib/protocol/requests'
import { ensure, errorText } from './lib/errors'
import { canonical, hash } from './lib/protocol/codec'
import type { SourceBinding } from './lib/protocol/requests'

const sourceKey = (source: SourceBinding) =>
  `${source.origin}:${source.tabId}:${source.documentId}`
const liveSources = new Map<string, Set<chrome.runtime.Port>>()
const deliveries = new Map<string, string>()
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
    const request = (await listRequests()).find((r) => r.id === message.requestId)
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
  const requests = await listRequests()
  for (const request of requests.filter((r) => r.state === 'expired'))
    await rejectRequest(request.id, 'expired')
  const count = requests.filter((r) => r.state === 'awaiting_user').length
  await chrome.action.setBadgeBackgroundColor({ color: '#145C45' })
  await chrome.action.setBadgeText({ text: count ? String(count) : '' })
}
// Old callbacks cannot survive a service-worker restart. The application must
// establish a fresh document-bound request; never reuse a stale approval bit.
const startup = listRequests().then(async (requests) => {
  for (const request of requests.filter((r) => r.state === 'awaiting_user'))
    await rejectRequest(request.id, 'cancelled')
  await refresh()
})

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
  let source: ReturnType<typeof trustedSource>
  try {
    ensure(port.name === 'dmsg-extension/3', 'UNSUPPORTED_PROTOCOL')
    source = trustedSource(port.sender ?? {}, config.externalOrigins)
  } catch {
    port.disconnect()
    return
  }
  let connected = true
  const key = sourceKey(source),
    ports = liveSources.get(key) ?? new Set<chrome.runtime.Port>()
  ports.add(port)
  liveSources.set(key, ports)
  const owned = new Set<string>()
  let queue = startup
  port.onMessage.addListener((message: unknown) => {
    if (
      message &&
      typeof message === 'object' &&
      'method' in message &&
      message.method === 'source.reply'
    ) {
      const reply = message as { nonce?: string }
      const pending = typeof reply.nonce === 'string' ? challenges.get(reply.nonce) : null
      if (connected && pending?.port === port) pending.done(true)
      return
    }
    queue = queue.then(async () => {
      if (!connected) return
      try {
        ensure(
          message && typeof message === 'object' && JSON.stringify(message).length <= 131072,
          'INVALID_INPUT'
        )
        const input = message as Record<string, unknown>
        if (input.method === 'signature.request') {
          const record = await enqueueRequest(input, source)
          owned.add(record.id)
          if (!connected) {
            await rejectRequest(record.id, 'cancelled')
            return
          }
          port.postMessage({ ok: true, requestId: record.id, state: record.state })
          await refresh()
          // Page requests create a pending item. Only a user action in dMsg
          // opens the independent confirmation window.
        } else if (input.method === 'signature.get') {
          ensure(
            Object.keys(input).every((key) =>
              ['method', 'requestId', 'protocol'].includes(key)
            ) && input.protocol === 'dmsg-extension/3',
            'INVALID_INPUT'
          )
          const record = (await listRequests()).find((r) => r.id === input.requestId)
          ensure(record && sameSource(record.source, source), 'FORBIDDEN')
          if (record.state === 'signed' || record.state === 'returned') {
            const result = await chrome.runtime
              .sendMessage({
                type: 'dmsg-read-formal-result',
                requestId: record.id,
                digest: record.digest
              })
              .catch(() => null)
            if (
              result?.ok &&
              result.digest === record.digest &&
              result.origin === source.origin &&
              connected
            ) {
              const resultDigest = hash(canonical(result.result))
              deliveries.set(record.id, resultDigest)
              port.postMessage({
                ok: true,
                requestId: record.id,
                state: record.state,
                result: result.result,
                resultDigest
              })
            } else if (connected)
              port.postMessage({
                ok: true,
                requestId: record.id,
                state: record.state,
                requiresUnlock: true
              })
          } else port.postMessage({ ok: true, requestId: record.id, state: record.state })
        } else if (input.method === 'signature.ack') {
          const record = (await listRequests()).find((r) => r.id === input.requestId)
          ensure(
            input.protocol === 'dmsg-extension/3' &&
              record &&
              sameSource(record.source, source) &&
              deliveries.get(record.id) === input.resultDigest,
            'FORBIDDEN'
          )
          await setRequestState(record.id, 'returned')
          deliveries.delete(record.id)
        } else if (input.method === 'signature.cancel') {
          ensure(owned.has(String(input.requestId)), 'FORBIDDEN')
          await rejectRequest(String(input.requestId), 'cancelled')
          port.postMessage({ ok: true, requestId: input.requestId, state: 'cancelled' })
        } else throw new Error('UNSUPPORTED_PROTOCOL：该能力尚未通过接入验收。')
      } catch (error) {
        if (connected) port.postMessage({ ok: false, error: errorText(error) })
      }
    })
  })
  port.onDisconnect.addListener(() => {
    connected = false
    for (const pending of challenges.values()) if (pending.port === port) pending.done(false)
    ports.delete(port)
    if (!ports.size) liveSources.delete(key)
    void queue.finally(async () => {
      for (const requestId of owned) await rejectRequest(requestId, 'cancelled')
      await refresh()
    })
  })
})

let dispatching: Promise<void> | null = null
async function flushCiphertext() {
  if (dispatching || !config.relayOrigin || config.environment === 'production')
    return dispatching
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
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === 'dmsg-cipher-dispatch') void flushCiphertext()
})
void chrome.alarms.create('dmsg-cipher-dispatch', { periodInMinutes: 0.5 })
void flushCiphertext()
