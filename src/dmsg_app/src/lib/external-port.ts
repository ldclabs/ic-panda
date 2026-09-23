import { EXTENSION_PROTOCOL } from '@dmsg/sdk'
import {
  hex,
  parseBrowserCommand,
  verifyBrowserProof,
  type BrowserCommand,
  type BrowserOperation
} from '@dmsg/sdk/browser'
import { createBrowserOperation, bindBrowserOperation } from './bridge-requests'
import { acknowledgeRequest, listRequests, openApproval, rejectRequest } from './requests'
import type { PendingRequest, SourceBinding } from './protocol/requests'
import { canonical, hash } from './protocol/codec'
import { ensure, DmsgError } from './errors'

/** Per-port nonces never survive a disconnect or service-worker restart. Operations do. */
export function externalPort(
  port: chrome.runtime.Port,
  source: SourceBinding,
  refresh: () => Promise<void>
) {
  let connected = true
  const pending = new Map<
    string,
    { command: BrowserCommand; nonce: string; timer: ReturnType<typeof setTimeout> }
  >()
  const delivered = new Map<string, string>()
  const result = (id: string, value: unknown, error?: unknown) => {
    if (connected)
      port.postMessage({
        protocol: EXTENSION_PROTOCOL,
        requestId: id,
        type: 'result',
        ok: !error,
        ...(error
          ? {
              error:
                error instanceof DmsgError
                  ? error.code
                  : error instanceof Error && /^[A-Z][A-Za-z_]+$/.test(error.message)
                    ? error.message.replace(/([a-z])([A-Z])/g, '$1_$2').toUpperCase()
                    : 'UNAVAILABLE'
            }
          : { value })
      })
  }
  async function run(command: BrowserCommand): Promise<BrowserOperation> {
    let record: PendingRequest
    if (['authenticate', 'signDocument', 'signAction', 'checkout'].includes(command.method)) {
      record = await createBrowserOperation(command, source)
      // Resume never issues a second approval. Opening this page only presents the original request.
      if (
        connected &&
        ['awaiting_user', 'authorized', 'execution_unknown', 'signed', 'returned'].includes(
          record.state
        )
      )
        await openApproval(record.id)
    } else {
      record = await bindBrowserOperation(command, source)
      if (command.method === 'openOperation') await openApproval(record.id)
      if (command.method === 'cancelOperation') {
        ensure(
          record.state === 'awaiting_user' ||
            ['cancelled', 'rejected', 'expired'].includes(record.state),
          'EXECUTION_UNKNOWN'
        )
        await rejectRequest(record.id, 'cancelled')
      } else if (command.method === 'acknowledge') {
        await acknowledgeRequest(
          record.id,
          source,
          command.resultDigest,
          delivered.get(record.id)
        )
        delivered.delete(record.id)
      }
      record = (await listRequests()).find((r) => r.id === record.id) ?? record
    }
    const value: BrowserOperation = {
      operationId: record.id,
      kind: record.kind,
      state: record.state
    }
    if (command.method === 'getOperation' && ['signed', 'returned'].includes(record.state)) {
      const reply = await chrome.runtime
        .sendMessage({
          type:
            record.kind === 'authentication'
              ? 'dmsg-read-authentication-result'
              : record.kind === 'checkout'
                ? 'dmsg-read-checkout-result'
                : 'dmsg-read-formal-result',
          requestId: record.id,
          digest: record.digest
        })
        .catch(() => null)
      if (reply?.ok && reply.digest === record.digest && reply.origin === source.origin) {
        value.result = reply.result
        value.resultDigest = hash(canonical(reply.result))
        delivered.set(record.id, value.resultDigest)
      } else value.requiresUnlock = true
    }
    await refresh()
    return value
  }
  port.onMessage.addListener((input: unknown) => {
    void (async () => {
      ensure(
        connected &&
          input &&
          typeof input === 'object' &&
          JSON.stringify(input).length <= 150000,
        'INVALID_INPUT'
      )
      const message = input as any
      if (message.method === 'source.reply') return
      ensure(
        message.protocol === EXTENSION_PROTOCOL && /^[0-9a-f]{64}$/.test(message.requestId),
        'UNSUPPORTED_PROTOCOL'
      )
      const id = message.requestId
      if (message.type === 'capabilities') {
        result(id, {
          protocol: EXTENSION_PROTOCOL,
          methods: [
            'authenticate',
            'signDocument',
            'signAction',
            'checkout',
            'getOperation',
            'openOperation',
            'cancelOperation',
            'acknowledge'
          ]
        })
        return
      }
      if (message.type === 'request') {
        ensure(pending.size < 16 && !pending.has(id), 'QUOTA_EXCEEDED')
        const command = parseBrowserCommand(message.command),
          nonce = hex(crypto.getRandomValues(new Uint8Array(32)))
        const timer = setTimeout(() => {
          pending.delete(id)
          result(id, null, new Error('EXPIRED'))
        }, 15000)
        pending.set(id, { command, nonce, timer })
        port.postMessage({
          protocol: EXTENSION_PROTOCOL,
          requestId: id,
          type: 'challenge',
          nonce,
          origin: source.origin,
          documentId: source.documentId
        })
      } else if (message.type === 'proof') {
        const waiting = pending.get(id)
        ensure(waiting, 'FORBIDDEN')
        clearTimeout(waiting.timer)
        pending.delete(id)
        await verifyBrowserProof(waiting.command, waiting.nonce, source, message.signature)
        ensure(connected, 'FORBIDDEN')
        result(id, await run(waiting.command))
      } else throw new Error('UNSUPPORTED_PROTOCOL')
    })().catch((error) => {
      const id = (input as any)?.requestId
      if (typeof id === 'string') result(id, null, error)
    })
  })
  port.onDisconnect.addListener(() => {
    connected = false
    for (const p of pending.values()) clearTimeout(p.timer)
    pending.clear()
    delivered.clear()
  })
}
