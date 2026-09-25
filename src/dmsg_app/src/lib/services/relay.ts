import { config, MAX_CIPHER_CHUNK } from '../config'
import { DmsgError, ensure } from '../errors'
import { bytes, canonical, equal, hash, unb64, unhex } from '../protocol/codec'
import {
  CLOUD_PROTOCOL,
  assertCloudDeadline,
  canonicalTarget,
  profileSchema,
  readCloudCommand,
  signCloudHttp,
  verifyCloudCommand,
  type CloudContext,
  type CloudSigned,
  type CloudSigner
} from '../protocol/cloud'
import type { CloudSecurityEvidence } from './cloud-security'
export { canonicalTarget } from '../protocol/cloud'

export interface RelayReadiness {
  ready: boolean
  protocol: string
  gates: string[]
  paid_contacts_enabled: boolean
}
export class RelayError extends DmsgError {
  constructor(
    code: string,
    message: string,
    readonly retryable: boolean,
    readonly requestId?: string
  ) {
    super(code, message)
  }
}
interface RelayOptions {
  origin: string
  environment: string
  protocol?: string
}

/** Explicit transport for the public cloud protocol. It never retries mutations. */
export class CloudClient {
  readonly origin: string
  constructor(private readonly options: RelayOptions) {
    ensure(['local', 'staging', 'production'].includes(options.environment), 'INVALID_INPUT')
    this.options = Object.freeze({ ...options })
    const url = new URL(options.origin)
    ensure(
      url.origin === options.origin &&
        !url.username &&
        !url.password &&
        (url.protocol === 'https:' ||
          (options.environment === 'local' &&
            url.protocol === 'http:' &&
            ['localhost', '127.0.0.1'].includes(url.hostname))),
      'INVALID_INPUT'
    )
    ensure((options.protocol ?? CLOUD_PROTOCOL) === CLOUD_PROTOCOL, 'UNSUPPORTED_PROTOCOL')
    this.origin = url.origin
  }
  private url(path: string) {
    ensure(
      path.startsWith('/') && !path.startsWith('//') && !path.includes('\\'),
      'INVALID_INPUT'
    )
    const url = new URL(path, this.origin)
    ensure(url.origin === this.origin, 'FORBIDDEN')
    canonicalTarget(url)
    return url
  }
  private async exchange(
    path: string,
    method: 'GET' | 'POST' | 'PUT',
    body?: Uint8Array,
    pop?: string,
    binary = false
  ): Promise<unknown> {
    if (path.startsWith('/v1/'))
      ensure(
        this.options.environment !== 'production',
        'UNAVAILABLE',
        '生产发布门禁尚未完成。'
      )
    const response = await fetch(this.url(path), {
      method,
      credentials: 'omit',
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(15000),
      headers: {
        ...(body
          ? { 'content-type': binary ? 'application/octet-stream' : 'application/cbor' }
          : {}),
        ...(pop ? { 'x-dmsg-pop': pop } : {})
      },
      ...(body ? { body: bytes(body) } : {})
    })
    const maximum = binary ? MAX_CIPHER_CHUNK : 2 * 1024 * 1024
    const reader = response.body?.getReader()
    ensure(reader, 'INTEGRITY_FAILED')
    const chunks: Uint8Array[] = []
    let length = 0
    try {
      while (true) {
        const part = await reader.read()
        if (part.done) break
        length += part.value.length
        if (length > maximum) {
          await reader.cancel()
          throw new DmsgError('QUOTA_EXCEEDED')
        }
        chunks.push(part.value)
      }
    } finally {
      reader.releaseLock()
    }
    const data = new Uint8Array(length)
    let offset = 0
    for (const chunk of chunks) {
      data.set(chunk, offset)
      offset += chunk.length
    }
    if (
      binary &&
      response.ok &&
      response.headers.get('content-type')?.startsWith('application/octet-stream')
    )
      return data
    const envelope = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data))
    if (!response.ok || envelope.ok !== true) {
      ensure(
        envelope.ok === false &&
          typeof envelope.error?.code === 'string' &&
          typeof envelope.error?.message === 'string' &&
          typeof envelope.error?.retryable === 'boolean',
        'INTEGRITY_FAILED'
      )
      throw new RelayError(
        envelope.error.code,
        envelope.error.message,
        envelope.error.retryable,
        envelope.request_id
      )
    }
    ensure(Object.hasOwn(envelope, 'data'), 'INTEGRITY_FAILED')
    return envelope.data
  }
  async readiness(): Promise<RelayReadiness> {
    const result = (await this.exchange('/ready', 'GET')) as RelayReadiness
    ensure(
      result.protocol === CLOUD_PROTOCOL &&
        typeof result.ready === 'boolean' &&
        Array.isArray(result.gates) &&
        result.gates.every((gate) => typeof gate === 'string') &&
        typeof result.paid_contacts_enabled === 'boolean',
      'UNSUPPORTED_PROTOCOL'
    )
    return result
  }
  async publishSecurity(evidence: CloudSecurityEvidence) {
    const body = canonical(evidence)
    ensure(body.length <= 262144, 'QUOTA_EXCEEDED')
    return this.exchange('/v1/security/evidence', 'POST', body)
  }
  async get(path: string, context: CloudContext, sign: CloudSigner) {
    context = { ...context }
    ensure(
      path.startsWith('/v1/') || /^\/public\/inboxes\/[0-9a-v]{20}\/policy$/.test(path),
      'INVALID_INPUT'
    )
    const pop = await signCloudHttp(context, this.url(path), 'GET', new Uint8Array(), sign)
    assertCloudDeadline(context.deadline)
    return this.exchange(path, 'GET', undefined, pop)
  }
  async post(path: string, signed: CloudSigned, context: CloudContext, sign: CloudSigner) {
    context = { ...context }
    ensure(path.startsWith('/v1/'), 'INVALID_INPUT')
    const command = readCloudCommand(signed)
    ensure(
      command.issuer === context.issuer &&
        equal(command.kid, unhex(context.deviceId)) &&
        command.body.request_id === context.requestId &&
        command.body.security_epoch === context.securityEpoch,
      'AUTH_REQUIRED'
    )
    assertCloudDeadline(command.body.deadline)
    const body = canonical({ cose_sign1: signed.cose_sign1 })
    ensure(body.length <= 262144, 'QUOTA_EXCEEDED')
    const pop = await signCloudHttp(context, this.url(path), 'POST', body, sign)
    assertCloudDeadline(Math.min(context.deadline, command.body.deadline))
    return this.exchange(path, 'POST', body, pop)
  }
  async postRaw(path: string, value: unknown, context: CloudContext, sign: CloudSigner) {
    ensure(
      /^\/v1\/(accounts\/[^/]+\/membership\/refresh|inboxes\/[^/]+\/orders\/[^/]+\/reconcile)$/.test(
        path
      ),
      'FORBIDDEN'
    )
    const body = canonical(value)
    ensure(body.length <= 262144, 'QUOTA_EXCEEDED')
    return this.exchange(
      path,
      'POST',
      body,
      await signCloudHttp(context, this.url(path), 'POST', body, sign)
    )
  }
  async putChunk(
    path: string,
    ciphertext: Uint8Array,
    context: CloudContext,
    sign: CloudSigner
  ) {
    context = { ...context }
    ensure(
      /^\/v1\/(accounts|channels)\/[^/]+\/uploads\/[^/]+\/chunks\/(manifest|[0-9]+)$/.test(
        path
      ),
      'INVALID_INPUT'
    )
    const body = bytes(ciphertext)
    ensure(body.length > 0 && body.length <= MAX_CIPHER_CHUNK, 'QUOTA_EXCEEDED')
    const pop = await signCloudHttp(context, this.url(path), 'PUT', body, sign)
    assertCloudDeadline(context.deadline)
    return this.exchange(path, 'PUT', body, pop, true)
  }
  async getChunk(
    path: string,
    expectedDigest: string,
    context: CloudContext,
    sign: CloudSigner
  ) {
    context = { ...context }
    ensure(
      /^\/v1\/(accounts|channels)\/[^/]+\/objects\/[^/]+\/chunks\/(manifest|[0-9]+)$/.test(
        path
      ),
      'INVALID_INPUT'
    )
    const pop = await signCloudHttp(context, this.url(path), 'GET', new Uint8Array(), sign)
    assertCloudDeadline(context.deadline)
    const body = await this.exchange(path, 'GET', undefined, pop, true)
    ensure(body instanceof Uint8Array && hash(body) === expectedDigest, 'INTEGRITY_FAILED')
    return body
  }
}

/** The caller supplies an authenticated device key; relay JSON alone grants no trust. */
export function verifyCloudProfile(
  value: unknown,
  context: CloudContext,
  publicKey: Uint8Array
) {
  ensure(value && typeof value === 'object', 'INTEGRITY_FAILED')
  const record = value as {
    signed: CloudSigned
    payload: unknown
    hash: string
    version: number
  }
  const body = verifyCloudCommand(record.signed, context, publicKey, 'dmsg/profile/v1')
  const profile = profileSchema.parse(body.payload)
  ensure(
    equal(canonical(profile), canonical(record.payload)) &&
      record.version === profile.version &&
      record.hash === hash(unb64(record.signed.cose_sign1)),
    'INTEGRITY_FAILED'
  )
  return profile
}

export function inspectRelay() {
  ensure(config.relayOrigin, 'UNAVAILABLE', '尚未配置中继 API 地址。')
  return new CloudClient({
    origin: config.relayOrigin,
    environment: config.environment,
    protocol: config.cloudProtocol
  }).readiness()
}
