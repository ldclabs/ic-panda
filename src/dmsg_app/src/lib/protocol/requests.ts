import { z } from 'zod'
import { ensure } from '../errors'
import { canonical, hash } from './codec'

const identifier = z
  .string()
  .regex(/^[0-9a-f]{64}$/)
  .refine((v) => !/^0+$/.test(v))
const fileBody = z
  .object({
    kind: z.literal('file_attestation'),
    sha256: identifier,
    size: z
      .string()
      .regex(/^(0|[1-9][0-9]*)$/)
      .refine((s) => BigInt(s) <= 104857600n),
    version: identifier,
    project: z
      .string()
      .min(1)
      .max(256)
      .refine((value) => value.trim().length > 0)
  })
  .strict()
const statementBody = z
  .object({
    kind: z.literal('statement'),
    text: z
      .string()
      .min(1)
      .max(4096)
      .refine((value) => value.trim().length > 0)
  })
  .strict()
export const requestSchema = z
  .object({
    protocol: z.literal('dmsg-extension/1'),
    method: z.literal('signature.request'),
    requestId: identifier,
    subjectId: identifier,
    audience: z
      .string()
      .min(1)
      .max(256)
      .refine((value) => value.trim().length > 0),
    nonce: identifier,
    expiresAt: z.string().regex(/^[1-9][0-9]*$/),
    body: z.discriminatedUnion('kind', [fileBody, statementBody])
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
  // Only the two structured R1 bodies are accepted. There is no raw signHash,
  // caller-provided origin, derivation path, controller or transaction method.
  ensure(
    new TextEncoder().encode(
      request.body.kind === 'statement' ? request.body.text : request.body.project
    ).length <= (request.body.kind === 'statement' ? 4096 : 256),
    'QUOTA_EXCEEDED'
  )
  const digest = hash(canonical(['dmsg/request/1', source.origin, source.documentId, request]))
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
