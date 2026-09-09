import type { SignatureRequest } from './protocol/requests'
import { ensure } from './errors'

/** Fixed-origin R0 integration: creates a pending request, never an approval. */
export function connectDmsg(extensionId: string) {
  ensure(/^[a-p]{32}$/.test(extensionId), 'INVALID_INPUT')
  const port = chrome.runtime.connect(extensionId, { name: 'dmsg-extension/3' })
  const pending = new Map<
    string,
    { resolve: (value: unknown) => void; reject: (reason: Error) => void }
  >()
  port.onMessage.addListener((reply) => {
    const waiter = pending.get(reply.requestId)
    if (waiter) {
      pending.delete(reply.requestId)
      reply.ok ? waiter.resolve(reply) : waiter.reject(new Error(reply.error))
    } else if (!reply.ok) {
      for (const waiter of pending.values()) waiter.reject(new Error(reply.error))
      pending.clear()
    }
  })
  port.onDisconnect.addListener(() => {
    for (const waiter of pending.values())
      waiter.reject(new Error('页面连接已结束，请重新发起请求。'))
    pending.clear()
  })
  function send(message: Record<string, unknown> & { requestId: string }): Promise<unknown> {
    ensure(!pending.has(message.requestId), 'PENDING')
    return new Promise((resolve, reject) => {
      pending.set(message.requestId, { resolve, reject })
      port.postMessage(message)
    })
  }
  return {
    request: (request: SignatureRequest) => send(request),
    get: (requestId: string) =>
      send({ protocol: 'dmsg-extension/3', method: 'signature.get', requestId }),
    cancel: (requestId: string) =>
      send({ protocol: 'dmsg-extension/3', method: 'signature.cancel', requestId }),
    disconnect: () => port.disconnect()
  }
}
