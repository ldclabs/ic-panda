import { Principal } from '@icp-sdk/core/principal'
import { ensure } from '../errors'
const alphabet = '0123456789abcdefghijklmnopqrstuv'
export function xidText(bytes: Uint8Array): string {
  ensure(
    bytes instanceof Uint8Array && bytes.length === 12,
    'INVALID_INPUT',
    'AccountId 必须为 12 字节。'
  )
  let value = 0n
  for (const byte of bytes) value = (value << 8n) | BigInt(byte)
  value <<= 4n
  let text = ''
  for (let i = 0; i < 20; i++) {
    text = alphabet[Number(value & 31n)] + text
    value >>= 5n
  }
  return text
}
export function xidBytes(text: string): Uint8Array<ArrayBuffer> {
  ensure(/^[0-9a-v]{19}[0g]$/.test(text), 'INVALID_INPUT', '需要规范的 20 字符 Xid。')
  let value = 0n
  for (const char of text) value = (value << 5n) | BigInt(alphabet.indexOf(char))
  value >>= 4n
  const bytes = new Uint8Array(12)
  for (let i = 11; i >= 0; i--) {
    bytes[i] = Number(value & 255n)
    value >>= 8n
  }
  ensure(xidText(bytes) === text, 'INVALID_INPUT')
  return bytes
}
export function assertUri(value: string) {
  ensure(
    typeof value === 'string' &&
      value.length > 0 &&
      value.length <= 8192 &&
      /^[\x21-\x7e]+$/.test(value),
    'INVALID_INPUT',
    '需要规范的绝对 URI。'
  )
  ensure(!/%(?![0-9a-fA-F]{2})/.test(value), 'INVALID_INPUT')
  const parsed = new URL(value)
  ensure(
    parsed.href === value && !parsed.username && !parsed.password,
    'INVALID_INPUT',
    'URI 不能在签名时被改写。'
  )
  return value
}
function namespace(value: string) {
  assertUri(value)
  const parsed = new URL(value)
  ensure(
    value.length <= 8128 && /[/:]$/.test(value) && !parsed.search && !parsed.hash,
    'INVALID_INPUT'
  )
  return value
}
export const accountIssuer = (prefix: string, id: Uint8Array) =>
  assertUri(namespace(prefix) + xidText(id))
export const principalIssuer = (prefix: string, id: Principal) =>
  assertUri(namespace(prefix) + id.toText())
