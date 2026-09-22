import { ed25519 } from '@noble/curves/ed25519.js'
import { z } from 'zod'
import { ensure } from '../errors'
import { b64, bytes, canonical, decodeCanonical, equal, hash, unb64, unhex } from './codec'
import { assertUri, xidBytes } from './identity'

export const CLOUD_PROTOCOL = 'dmsg-cloud/1' as const
export const CLOUD_FRESHNESS_MS = 60_000
export const CLOUD_COMMAND_PROFILE = 'application/vnd.dmsg.command+cose;v=1'
export const CLOUD_HTTP_PROFILE = 'application/vnd.dmsg.http-pop+cose;v=1'
export const cloudActions = [
  'dmsg/legacy/propose/v1',
  'dmsg/legacy/consent/v1',
  'dmsg/legacy/commit/v1',
  'dmsg/legacy/claim/v1',
  'dmsg/legacy/activate/v1',
  'dmsg/legacy/joined/v1',
  'dmsg/legacy/history/v1',
  'dmsg/profile/v1',
  'dmsg/vault/revision/v1',
  'dmsg/upload/reserve/v1',
  'dmsg/upload/finalize/v1',
  'dmsg/upload/cancel/v1',
  'dmsg/object/collect/v1',
  'dmsg/membership/notice-ack/v1',
  'dmsg/channel/reactivate/v1',
  'dmsg/channel/billing/prepare/v1',
  'dmsg/channel/billing/transfer/v1',
  'dmsg/channel/billing/cancel/v1',
  'dmsg/workflow/v1',
  'dmsg/cursor/v1',
  'dmsg/file/grant/v1',
  'dmsg/file/proxy-read/v1',
  'dmsg/export/start/v1',
  'dmsg/channel/genesis/v1',
  'dmsg/channel/control/v1',
  'dmsg/channel/lease/v1',
  'dmsg/channel/activate/v1',
  'dmsg/channel/message/v1',
  'dmsg/channel/ws-ticket/v1',
  'dmsg/channel/proxy-read/v1',
  'dmsg/inbox/policy/v1',
  'dmsg/inbox/key/v1',
  'dmsg/inbox/contact/v1',
  'dmsg/inbox/admit/v1',
  'dmsg/inbox/mark/v1'
] as const
export type CloudAction = (typeof cloudActions)[number]
const uint = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER)
const hashId = z.string().regex(/^[0-9a-f]{64}$/)
const id = hashId.refine((s) => /[1-9a-f]/.test(s))
const common = {
  protocol: z.literal(CLOUD_PROTOCOL),
  security_epoch: uint,
  request_id: id,
  deadline: uint
}
const commandSchema = z.strictObject({
  ...common,
  action: z.enum(cloudActions),
  payload: z.record(z.string(), z.unknown())
})
const httpSchema = z.strictObject({
  ...common,
  audience: z.string().max(256),
  method: z.enum(['GET', 'POST', 'PUT']),
  target: z.string().min(1).max(8192),
  body_digest: hashId
})
export const profileSchema = z.strictObject({
  version: uint.min(1),
  prev_hash: hashId.nullable(),
  display_name: z.string().max(80),
  bio: z.string().max(2000),
  avatar_upload: id.nullable(),
  links: z.array(z.string().url().max(2048)).max(10)
})
export type CloudProfile = z.infer<typeof profileSchema>
export interface CloudContext {
  accountId: string
  issuer: string
  deviceId: string
  securityEpoch: number
  requestId: string
  deadline: number
}
export type CloudSigner = (message: Uint8Array) => Promise<Uint8Array>
export interface CloudSigned {
  cose_sign1: string
}

function checkedContext(context: CloudContext) {
  xidBytes(context.accountId)
  assertUri(context.issuer)
  ensure(
    context.issuer.endsWith('/' + context.accountId) ||
      context.issuer.endsWith(':' + context.accountId),
    'INVALID_INPUT'
  )
  id.parse(context.deviceId)
  return {
    protocol: CLOUD_PROTOCOL,
    security_epoch: uint.parse(context.securityEpoch),
    request_id: id.parse(context.requestId),
    deadline: uint.parse(context.deadline)
  }
}
export function assertCloudDeadline(deadline: number, now = Date.now()) {
  ensure(
    Number.isSafeInteger(deadline) && now < deadline && deadline <= now + CLOUD_FRESHNESS_MS,
    'POLICY_STALE'
  )
}

async function signProfile(
  context: CloudContext,
  typ: string,
  body: unknown,
  sign: CloudSigner
): Promise<string> {
  const headers = canonical(
    new Map<number, unknown>([
      [1, -19],
      [2, [15, 16]],
      [3, 'application/cbor'],
      [4, unhex(context.deviceId)],
      [15, new Map([[1, context.issuer]])],
      [16, typ]
    ])
  )
  const payload = canonical(body)
  // Reject any text the current CBOR decoder cannot round-trip byte-for-byte
  // before asking a device to sign it (including leading U+FEFF).
  decodeCanonical(payload)
  const message = canonical(['Signature1', headers, new Uint8Array(), payload])
  const signature = await sign(bytes(message))
  ensure(signature.length === 64, 'INTEGRITY_FAILED')
  const encoded = canonical([headers, new Map(), payload, bytes(signature)])
  return b64(Uint8Array.from([0xd2, ...encoded]))
}

/** Ordinary device content authorization only; never a formal signing request. */
export async function signCloudCommand(
  context: CloudContext,
  action: CloudAction,
  payload: Record<string, unknown>,
  sign: CloudSigner
): Promise<CloudSigned> {
  const body = commandSchema.parse({ ...checkedContext(context), action, payload })
  assertCloudDeadline(body.deadline)
  return { cose_sign1: await signProfile(context, CLOUD_COMMAND_PROFILE, body, sign) }
}

function parseProfile(encoded: string, typ: string, max = 262144) {
  const encodedBytes = unb64(encoded, max)
  ensure(encodedBytes[0] === 0xd2, 'UNSUPPORTED_PROTOCOL')
  // The tag is consumed explicitly; the bounded codec rejects nested COSE tags.
  const envelope = decodeCanonical<unknown[]>(encodedBytes.subarray(1), max)
  ensure(Array.isArray(envelope) && envelope.length === 4, 'INTEGRITY_FAILED')
  const [protectedBytes, unprotected, payload, signature] = envelope
  ensure(
    protectedBytes instanceof Uint8Array &&
      payload instanceof Uint8Array &&
      signature instanceof Uint8Array &&
      signature.length === 64 &&
      ((unprotected instanceof Map && unprotected.size === 0) ||
        (unprotected &&
          Object.getPrototypeOf(unprotected) === Object.prototype &&
          Object.keys(unprotected).length === 0)),
    'INTEGRITY_FAILED'
  )
  const headers = decodeCanonical<Map<number, unknown>>(protectedBytes)
  ensure(
    headers instanceof Map &&
      headers.size === 6 &&
      [1, 2, 3, 4, 15, 16].every((key) => headers.has(key)) &&
      headers.get(1) === -19 &&
      headers.get(3) === 'application/cbor' &&
      headers.get(16) === typ,
    'UNSUPPORTED_PROTOCOL'
  )
  const critical = headers.get(2),
    claims = headers.get(15),
    kid = headers.get(4)
  ensure(
    Array.isArray(critical) &&
      critical.length === 2 &&
      critical.includes(15) &&
      critical.includes(16) &&
      claims instanceof Map &&
      claims.size === 1 &&
      typeof claims.get(1) === 'string' &&
      kid instanceof Uint8Array &&
      kid.length === 32,
    'UNSUPPORTED_PROTOCOL'
  )
  const issuer = assertUri(claims.get(1) as string)
  return {
    issuer,
    kid,
    signature,
    payload,
    message: canonical(['Signature1', protectedBytes, new Uint8Array(), payload])
  }
}

/** Syntax only. Call verifyCloudCommand with an authenticated device key for trust. */
export function readCloudCommand(signed: CloudSigned) {
  ensure(signed && Object.keys(signed).length === 1, 'UNSUPPORTED_PROTOCOL')
  const parsed = parseProfile(signed.cose_sign1, CLOUD_COMMAND_PROFILE)
  return { ...parsed, body: commandSchema.parse(decodeCanonical(parsed.payload)) }
}
export function verifyCloudCommand(
  signed: CloudSigned,
  expected: Pick<CloudContext, 'issuer' | 'deviceId'>,
  publicKey: Uint8Array,
  action: CloudAction
) {
  const parsed = readCloudCommand(signed)
  ensure(
    parsed.issuer === expected.issuer &&
      equal(parsed.kid, unhex(expected.deviceId)) &&
      parsed.body.action === action &&
      publicKey.length === 32,
    'AUTH_REQUIRED'
  )
  ensure(
    ed25519.verify(parsed.signature, parsed.message, publicKey, { zip215: false }),
    'AUTH_REQUIRED'
  )
  // Stored commands may be historical. Deadline freshness is a submission check,
  // not grounds for accepting/rejecting the mathematical signature of old content.
  return parsed.body
}
export function canonicalTarget(url: URL) {
  ensure(
    !url.hash &&
      !url.username &&
      !url.password &&
      !/%2f|%5c/i.test(url.pathname) &&
      !url.pathname.includes('\\'),
    'INVALID_INPUT'
  )
  const entries = [...url.searchParams.entries()]
  ensure(new Set(entries.map(([key]) => key)).size === entries.length, 'INVALID_INPUT')
  entries.sort(([a, av], [b, bv]) => (a < b ? -1 : a > b ? 1 : av < bv ? -1 : av > bv ? 1 : 0))
  const query = new URLSearchParams(entries).toString()
  return url.pathname + (query ? `?${query}` : '')
}
export async function signCloudHttp(
  context: CloudContext,
  url: URL,
  method: 'GET' | 'POST' | 'PUT',
  body: Uint8Array,
  sign: CloudSigner
) {
  const payload = httpSchema.parse({
    ...checkedContext(context),
    audience: url.origin,
    method,
    target: canonicalTarget(url),
    body_digest: hash(body)
  })
  assertCloudDeadline(payload.deadline)
  return signProfile(context, CLOUD_HTTP_PROFILE, payload, sign)
}

export function verifyCloudHttp(encoded: string, publicKey: Uint8Array) {
  const parsed = parseProfile(encoded, CLOUD_HTTP_PROFILE, 16384)
  ensure(
    publicKey.length === 32 &&
      ed25519.verify(parsed.signature, parsed.message, publicKey, { zip215: false }),
    'AUTH_REQUIRED'
  )
  return {
    issuer: parsed.issuer,
    kid: parsed.kid,
    body: httpSchema.parse(decodeCanonical(parsed.payload))
  }
}
