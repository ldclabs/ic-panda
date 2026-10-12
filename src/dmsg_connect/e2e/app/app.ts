/**
 * The application page of the browser tests: a registered origin that
 * opens the bridge from a click, as connect-v1 §4.1 requires, and exposes
 * the SDK client to the test driver as `window.app`.
 */
import { connectDmsg, type DmsgConnection } from 'dmsg-sdk/connect'

const params = new URLSearchParams(location.search)
const bridgeOrigin = params.get('bridge')!
const appId = params.get('app') ?? 'token-list'
const key = await crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign', 'verify'])
const session = {
  publicKeyDer: new Uint8Array(await crypto.subtle.exportKey('spki', key.publicKey)),
  sign: async (message: Uint8Array) =>
    new Uint8Array(await crypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, key.privateKey, Uint8Array.from(message)))
}
let client: DmsgConnection | null = null

document.querySelector('#connect')!.addEventListener('click', () => {
  client = connectDmsg({ appId, session, bridgeOrigin })
})

const outcome = async (run: () => Promise<unknown>) => {
  try {
    return { value: await run() }
  } catch (error) {
    return { error: (error as { code?: string }).code ?? String(error) }
  }
}
const hex = (bytes: Uint8Array) => Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')

Object.assign(window, {
  app: {
    ready: () => outcome(async () => (await client!.ready, client!.methods)),
    open: () => client?.open ?? false,
    signDocument: (text: string, accountId: string, issuer: string) =>
      outcome(() => {
        const operationId = hex(crypto.getRandomValues(new Uint8Array(32)))
        return client!.signDocument(operationId, {
          protocol: 'dmsg-connect/1',
          method: 'signature.request',
          requestId: operationId,
          accountId,
          nonce: hex(crypto.getRandomValues(new Uint8Array(32))),
          expiresAt: String(Date.now() + 600_000),
          statement: { issuer, content: { kind: 'text', text } }
        })
      }),
    getOperation: (id: string) => outcome(() => client!.getOperation(id)),
    cancelOperation: (id: string) => outcome(() => client!.cancelOperation(id)),
    acknowledge: (id: string, digest: string) => outcome(() => client!.acknowledge(id, digest))
  }
})
document.documentElement.dataset.ready = 'true'
