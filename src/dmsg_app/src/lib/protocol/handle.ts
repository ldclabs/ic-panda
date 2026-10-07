import { IDL } from '@icp-sdk/core/candid'
import { idlFactory } from '../canisters/generated/handle/index.js'
import type { HandleIntent, LegacyReservation } from '../canisters/generated/handle'
import { b64, unb64, digest } from './codec'
import { candidValue } from './account'
import { ensure } from '../errors'

const service = idlFactory({ IDL }) as IDL.ServiceClass
export const handlePrice = (name: string) =>
  BigInt(
    name.length === 1
      ? 1000000
      : name.length === 2
        ? 200000
        : name.length <= 4
          ? 50000
          : name.length <= 6
            ? 20000
            : 100
  ) * 100000000n
export function encodeHandle(method: string, args: unknown[]) {
  const fn = service._fields.find(([name]) => name === method)![1]
  return b64(new Uint8Array(IDL.encode(fn.argTypes, args)))
}
export function decodeHandle(method: string, encoded: string): any[] {
  return IDL.decode(
    service._fields.find(([name]) => name === method)![1].argTypes,
    unb64(encoded)
  )
}
const claim = service._fields.find(([name]) => name === 'claim_legacy_handle')![1]
const entries = service._fields.find(([name]) => name === 'import_legacy_handles')![1]
  .argTypes[2] as IDL.VecClass<LegacyReservation>
// Certified-tree labels above a canonical name: one [0] or [1] per leading bit
// of digest("dmsg/handle-bucket/v1", name), 20 bits, most significant first.
export const handleBucketPath = (name: string) => {
  const d = digest('dmsg/handle-bucket/v1', name)
  return Array.from({ length: 20 }, (_, i) => Uint8Array.of((d[i >> 3] >> (7 - (i & 7))) & 1))
}
export function canonicalHandle(value: string) {
  const name = value.toLowerCase()
  ensure(
    /^[a-z0-9][a-z0-9_]{0,18}$/.test(name),
    'INVALID_INPUT',
    '名称应为 1–19 个字母、数字或下划线，且不能以下划线开头。'
  )
  return name
}
const reservationValue = (value: LegacyReservation) => candidValue(entries._type, value)
export const legacyEntryDigest = (previous: Uint8Array, value: LegacyReservation) =>
  digest('dmsg/legacy-entry/v1', [previous, reservationValue(value)])
export const legacyClaimDigest = (
  snapshot: Uint8Array,
  value: LegacyReservation,
  account: Uint8Array
) => digest('dmsg/legacy-claim/v1', [snapshot, reservationValue(value), account])
export const encodeClaim = (intent: HandleIntent, snapshot: Uint8Array) =>
  b64(new Uint8Array(IDL.encode(claim.argTypes, [intent, snapshot])))
export const decodeClaim = (encoded: string) =>
  IDL.decode(claim.argTypes, unb64(encoded)) as unknown as [HandleIntent, Uint8Array]
