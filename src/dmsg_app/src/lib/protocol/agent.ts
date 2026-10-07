// Agent Delegation 1.0 events signed by the account's self-held controller keys.
// The extension builds the exact JCS text it signs; the delegation service
// parses it strictly and rejects anything else.
import { sha3_256 } from '@noble/hashes/sha3.js'
import { b64, unb64, utf8 } from './codec'
import { ensure } from '../errors'

export const DELEGATION_PROTOCOL = 'agent-delegation/1.0'
export const AGENT_ID_PREFIX = 'did:agent:'
/** Grants must expire within 366 days. */
export const MAX_GRANT_DAYS = 366
const DAY = 86_400_000

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json }

/**
 * RFC 8785 JCS. Keys sort by UTF-16 code units (JavaScript's default order) and
 * strings and finite numbers serialize as ECMAScript JSON, which is exactly JCS.
 */
export function jcs(value: Json): string {
  if (value === null || typeof value !== 'object') {
    ensure(
      typeof value !== 'number' || Number.isFinite(value),
      'INVALID_INPUT',
      'JSON numbers must be finite.'
    )
    return JSON.stringify(value)
  }
  if (Array.isArray(value)) return `[${value.map(jcs).join(',')}]`
  return `{${Object.keys(value)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${jcs(value[key]!)}`)
    .join(',')}}`
}

export function agentId(publicKey: Uint8Array): string {
  ensure(publicKey.length === 32, 'INTEGRITY_FAILED')
  return `${AGENT_ID_PREFIX}${b64(publicKey)}`
}

export function isAgentId(value: string): boolean {
  if (!value.startsWith(AGENT_ID_PREFIX)) return false
  try {
    return unb64(value.slice(AGENT_ID_PREFIX.length), 32).length === 32
  } catch {
    return false
  }
}

/** Serialized HTTPS origin, as Agent Identity Section 4.4 requires for audiences. */
export function isOrigin(value: string): boolean {
  try {
    const url = new URL(value)
    return url.protocol === 'https:' && url.origin === value
  } catch {
    return false
  }
}

export interface DelegationEvent {
  protocol: string
  type: 'delegation.grant' | 'delegation.revoke'
  actor: string
  created_at: number
  nonce: number
  payload: { [key: string]: Json }
}

export interface Envelope {
  hash: string
  event: DelegationEvent
  signature: string
}

export function eventText(event: DelegationEvent): string {
  ensure(
    Number.isSafeInteger(event.nonce) && event.nonce > 0 && Number.isSafeInteger(event.created_at),
    'INVALID_INPUT'
  )
  return jcs(event as unknown as Json)
}

export function eventHash(text: string): Uint8Array {
  return sha3_256(utf8(text))
}

/** Agent Identity clock-derived nonce: max(last + 1, created_at). */
export const nextNonce = (last: bigint, now: number) =>
  Math.max(Number(last) + 1, now)

/** `<account_id>.<128-bit random suffix>`: the prefix routes reads to the principal. */
export function delegationId(account: string): string {
  return `${account}.${b64(crypto.getRandomValues(new Uint8Array(16)))}`
}

function list(values: string[], check: (value: string) => boolean, label: string) {
  const unique = new Set(values)
  ensure(
    values.length > 0 &&
      unique.size === values.length &&
      values.every((v) => v.trim() === v && v !== '' && v !== '*' && check(v)),
    'INVALID_INPUT',
    `${label}需为不重复的明确取值，不能使用通配符。`
  )
  return values
}

export const scopes = (values: string[]) =>
  list(values, (v) => v.length <= 64, '授权范围')
export const audiences = (values: string[]) =>
  list(values, (v) => isOrigin(v) || isAgentId(v), '依赖方（HTTPS origin 或 Agent ID）')

export interface GrantInput {
  id: string
  principalId: string
  subject: string
  scopes: string[]
  audiences: string[]
  days: number
  relationship?: string
}

export function grantEvent(
  actor: string,
  nonce: number,
  input: GrantInput,
  createdAt = Date.now()
): DelegationEvent {
  ensure(isAgentId(input.subject), 'INVALID_INPUT', '被授权 agent 需为 did:agent。')
  ensure(
    Number.isInteger(input.days) && input.days >= 1 && input.days <= MAX_GRANT_DAYS,
    'INVALID_INPUT',
    `有效期需在 1 到 ${MAX_GRANT_DAYS} 天之间。`
  )
  return {
    protocol: DELEGATION_PROTOCOL,
    type: 'delegation.grant',
    actor,
    created_at: createdAt,
    nonce,
    payload: {
      id: input.id,
      principal_id: input.principalId,
      subject: input.subject,
      scopes: scopes(input.scopes),
      audiences: audiences(input.audiences),
      ...(input.relationship ? { relationship: input.relationship } : {}),
      expires_at: createdAt + input.days * DAY
    }
  }
}

export function revokeEvent(
  actor: string,
  nonce: number,
  id: string,
  principalId: string,
  createdAt = Date.now()
): DelegationEvent {
  return {
    protocol: DELEGATION_PROTOCOL,
    type: 'delegation.revoke',
    actor,
    created_at: createdAt,
    nonce,
    payload: { id, principal_id: principalId }
  }
}

export function envelope(event: DelegationEvent, hash: Uint8Array, signature: Uint8Array): Envelope {
  ensure(hash.length === 32 && signature.length === 64, 'INTEGRITY_FAILED')
  return { hash: b64(hash), event, signature: b64(signature) }
}
