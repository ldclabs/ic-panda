import { config } from './lib/config'
import { currentWorkspace, WorkspaceDB } from './lib/db'
import { enqueueRequest, listRequests, rejectRequest } from './lib/requests'
import { sameSource, trustedSource } from './lib/protocol/requests'
import { ensure, errorText } from './lib/errors'

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
  const owned = new Set<string>()
  let queue = startup
  port.onMessage.addListener((message: unknown) => {
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
          port.postMessage({ ok: true, requestId: record.id, state: record.state })
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
    void queue.finally(async () => {
      for (const requestId of owned) await rejectRequest(requestId, 'cancelled')
      await refresh()
    })
  })
})
