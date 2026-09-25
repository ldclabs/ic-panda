import { IDL } from '@icp-sdk/core/candid'
import { idlFactory } from '../canisters/generated/handle/index.js'
import type { HandleIntent, LegacyReservation } from '../canisters/generated/handle'
import { b64, unb64, digest } from './codec'
import { candidValue } from './account'
import { ensure } from '../errors'

const service = idlFactory({ IDL }) as IDL.ServiceClass
const claim = service._fields.find(([name]) => name === 'claim_legacy_handle')![1]
const entries = service._fields.find(([name]) => name === 'import_legacy_handles')![1]
  .argTypes[1] as IDL.VecClass<LegacyReservation>
export function canonicalHandle(value: string) {
  const name = value.toLowerCase()
  ensure(
    /^[a-z0-9][a-z0-9_]{0,19}$/.test(name),
    'INVALID_INPUT',
    '名称应为 1–20 个字母、数字或下划线，且不能以下划线开头。'
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
