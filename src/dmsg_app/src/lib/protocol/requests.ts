import { z } from 'zod'
import { ensure } from '../errors'
import { canonical, hash, unhex } from './codec'
import { assertStatement, type DocumentStatement } from './statements'

const identifier = z
  .string()
  .regex(/^[0-9a-f]{64}$/)
  .refine((v) => !/^0+$/.test(v))
const timestamp = z
  .string()
  .max(20)
  .regex(/^-?(0|[1-9][0-9]*)$/)
  .refine(
    (v) => BigInt(v) >= -0x8000000000000000n && BigInt(v) <= 0x7fffffffffffffffn && v !== '-0'
  )
const contentSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('text'), text: z.string().min(1).max(4096) }).strict(),
  z
    .object({
      kind: z.literal('digest'),
      sha256: z.string().regex(/^[0-9a-f]{64}$/),
      contentType: z.string().max(256).optional(),
      location: z.string().max(8192).optional()
    })
    .strict()
])
export const requestSchema = z
  .object({
    protocol: z.literal('dmsg-extension/3'),
    method: z.literal('signature.request'),
    requestId: identifier,
    accountId: z.string().regex(/^[0-9a-v]{19}[0g]$/),
    nonce: identifier,
    expiresAt: z
      .string()
      .max(16)
      .regex(/^[1-9][0-9]*$/),
    statement: z
      .object({
        issuer: z.string().max(8192),
        subject: z.string().max(8192).optional(),
        issuedAt: timestamp.optional(),
        content: contentSchema
      })
      .strict()
  })
  .strict()
export type SignatureRequest = z.infer<typeof requestSchema>
export interface SourceBinding {
  origin: string
  tabId: number
  frameId: number
  documentId: string
}
export interface PendingRequest {
  id: string
  source: SourceBinding
  digest: string
  state:
    | 'awaiting_user'
    | 'rejected'
    | 'cancelled'
    | 'expired'
    | 'authorized'
    | 'execution_unknown'
    | 'signed'
    | 'returned'
  expiresAt: number
  createdAt: number
  payload: { enc: string; ciphertext: string }
}
export function trustedSource(
  sender: chrome.runtime.MessageSender,
  allowlist: readonly string[]
): SourceBinding {
  ensure(
    typeof sender.origin === 'string' &&
      sender.origin !== 'null' &&
      allowlist.includes(sender.origin),
    'FORBIDDEN',
    '此应用来源未获准接入。'
  )
  ensure(
    sender.frameId === 0 &&
      typeof sender.tab?.id === 'number' &&
      typeof sender.documentId === 'string' &&
      sender.documentLifecycle === 'active',
    'FORBIDDEN',
    '请求必须来自当前顶层页面。'
  )
  ensure(
    typeof sender.url === 'string' &&
      new URL(sender.url).origin === sender.origin &&
      new URL(sender.url).protocol === 'https:',
    'FORBIDDEN'
  )
  return {
    origin: sender.origin,
    tabId: sender.tab.id,
    frameId: 0,
    documentId: sender.documentId
  }
}
export function parseRequest(input: unknown, source: SourceBinding, now = Date.now()) {
  ensure(JSON.stringify(input).length <= 131072, 'QUOTA_EXCEEDED')
  const request = requestSchema.parse(input),
    expiresAt = Number(request.expiresAt)
  ensure(
    Number.isSafeInteger(expiresAt) && expiresAt > now && expiresAt <= now + 5 * 60 * 1000,
    'EXPIRED',
    '请求期限必须在未来 5 分钟内。'
  )
  assertStatement(requestStatement(request))
  const digest = hash(canonical(['dmsg/request/3', source.origin, source.documentId, request]))
  return { request, digest, expiresAt }
}
export const sameSource = (a: SourceBinding, b: SourceBinding) =>
  a.origin === b.origin &&
  a.tabId === b.tabId &&
  a.frameId === b.frameId &&
  a.documentId === b.documentId
export function assertRequestUnchanged(request: SignatureRequest, stored: PendingRequest) {
  const value = parseRequest(request, stored.source)
  ensure(
    value.digest === stored.digest &&
      request.requestId === stored.id &&
      stored.state === 'awaiting_user',
    'INTEGRITY_FAILED',
    '请求内容或状态已变化，请重新发起。'
  )
}

export function requestStatement(request: SignatureRequest): DocumentStatement {
  const s = request.statement
  return {
    issuer: s.issuer,
    subject: s.subject,
    issuedAt: s.issuedAt === undefined ? undefined : BigInt(s.issuedAt),
    content:
      s.content.kind === 'text' ? s.content : { ...s.content, sha256: unhex(s.content.sha256) }
  }
}
