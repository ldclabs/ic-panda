/** The relay's bridge routes (`/v1/connect/*`), each request with a Connect PoP. */
import { b64, canonical, randomId, sha256 } from './codec.ts'
import { sign1 } from './crypto.ts'
import { ConnectError, type ConnectCode } from './errors.ts'
import { POP_TYP, PROTOCOL } from './protocol.ts'

/** The key that signs a request: a pairing, or a candidate before its grant. */
export interface Signer {
  account: Uint8Array
  pairingId: Uint8Array
  key: CryptoKey
}

/** Path plus sorted query, as the relay checks it. */
export function canonicalTarget(url: URL): string {
  const pairs = [...url.searchParams.entries()].sort(([a, b], [c, d]) =>
    a < c ? -1 : a > c ? 1 : b < d ? -1 : b > d ? 1 : 0
  )
  const query = new URLSearchParams(pairs).toString()
  return url.pathname + (query ? `?${query}` : '')
}

export class Relay {
  readonly origin: string
  constructor(origin: string) {
    this.origin = origin.replace(/\/$/, '')
  }

  async proof(signer: Signer, method: string, url: URL, body: Uint8Array): Promise<string> {
    const payload = canonical({
      protocol: PROTOCOL,
      account_id: signer.account,
      pairing_id: signer.pairingId,
      request_id: randomId(),
      deadline: Date.now() + 45_000,
      audience: this.origin,
      method,
      target: canonicalTarget(url),
      body_digest: sha256(body)
    })
    return b64(await sign1(signer.key, signer.pairingId, POP_TYP, payload))
  }

  /** One call; the relay's `{ok, data}` or a ConnectError with its code. */
  async call<T = any>(signer: Signer, method: 'GET' | 'POST', path: string, value?: unknown): Promise<T> {
    const url = new URL(path, this.origin)
    const body = value === undefined ? new Uint8Array() : canonical(value)
    const response = await fetch(url, {
      method,
      headers: {
        'content-type': 'application/cbor',
        'x-dmsg-connect-pop': await this.proof(signer, method, url, body)
      },
      ...(method === 'GET' ? {} : { body: new Uint8Array(body) })
    }).catch(() => {
      throw new ConnectError('UNAVAILABLE')
    })
    const reply = (await response.json().catch(() => null)) as { ok?: boolean; data?: T; error?: { code: string } } | null
    if (reply?.ok) return reply.data as T
    const code = reply?.error?.code ?? 'UNAVAILABLE'
    throw new ConnectError(relayCode(code), code)
  }
}

/** The relay's error codes as the page sees them. */
function relayCode(code: string): ConnectCode {
  switch (code) {
    case 'NOT_PAIRED':
    case 'NOT_FOUND':
    case 'NOT_CANCELLABLE':
    case 'QUOTA_EXCEEDED':
    case 'EXPIRED':
    case 'IDEMPOTENCY_CONFLICT':
    case 'INTEGRITY_FAILED':
    case 'FORBIDDEN':
      return code
    case 'RESULT_EXPIRED':
      return 'EXPIRED'
    case 'TOO_LARGE':
      return 'QUOTA_EXCEEDED'
    case 'INVALID_INPUT':
      return 'INVALID_INPUT'
    default:
      return 'UNAVAILABLE'
  }
}
