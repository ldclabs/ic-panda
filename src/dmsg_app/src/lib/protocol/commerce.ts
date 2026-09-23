import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { idlFactory as commerceIDL } from '../canisters/generated/commerce/index.js'
import { idlFactory as membershipIDL } from '../canisters/generated/membership/index.js'
import { candidValue } from './account'
import { fromCandid } from '@dmsg/sdk'
import { b64, unb64, digest } from './codec'
import { xidBytes } from './identity'
import { ensure } from '../errors'
import type { Beneficiary } from '../canisters/generated/commerce'
const services = {
  commerce: commerceIDL({ IDL }) as IDL.ServiceClass,
  membership: membershipIDL({ IDL }) as IDL.ServiceClass
}
export type CommercePlane = keyof typeof services
export function apiMethod(plane: CommercePlane, method: string) {
  const fn = services[plane]._fields.find(([name]) => name === method)?.[1]
  ensure(fn, 'UNSUPPORTED_PROTOCOL')
  return fn
}
export const encodeCommerce = (plane: CommercePlane, method: string, args: unknown[]) =>
  b64(new Uint8Array(IDL.encode(apiMethod(plane, method).argTypes, args)))
export const decodeCommerce = (plane: CommercePlane, method: string, encoded: string) =>
  IDL.decode(apiMethod(plane, method).argTypes, unb64(encoded))
export const commerceInput = (method: string, value: unknown, index = 0) =>
  candidValue(apiMethod('commerce', method).argTypes[index], value)
export const membershipInput = (method: string, value: unknown, index = 0) =>
  candidValue(apiMethod('membership', method).argTypes[index], value)
export const beneficiary = (home: string, account: string): Beneficiary => ({
  product_id: 'dmsg',
  authority_canister: Principal.fromText(home),
  subject_schema: 'dmsg-account-v1',
  subject_bytes: xidBytes(account)
})
export const beneficiaryValue = (value: Beneficiary) => ({
  product_id: value.product_id,
  authority_canister: value.authority_canister.toUint8Array(),
  subject_schema: value.subject_schema,
  subject_bytes: Uint8Array.from(value.subject_bytes)
})

export { toCandid } from './app-action'
export function wireResult(plane: CommercePlane, method: string, value: unknown) {
  const result = apiMethod(plane, method).retTypes[0] as IDL.VariantClass
  const type = result._fields.find(([name]) => name === 'Ok')?.[1]
  ensure(type, 'UNSUPPORTED_PROTOCOL')
  return fromCandid(type, value)
}
