import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { idlFactory as commerceIDL } from '../canisters/generated/commerce/index.js'
import { idlFactory as membershipIDL } from '../canisters/generated/membership/index.js'
import { candidValue } from './account'
import { b64, unb64, digest } from './codec'
import { xidBytes } from './identity'
import { ensure } from '../errors'
import type {
  Beneficiary,
  OrderQuote,
  MembershipIntent,
  ClaimRequest
} from '../canisters/generated/commerce'
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
export function quoteValue(quote: OrderQuote) {
  return (commerceInput('open_order', { quote, authorization: emptyIntent(quote) }) as any)
    .quote
}
function emptyIntent(quote: OrderQuote): MembershipIntent {
  return {
    actor: quote.request.payer.owner,
    beneficiary: quote.request.beneficiary,
    application_id: quote.request.op_id,
    environment: { Local: null },
    service_canister: quote.home_commerce,
    action_digest: new Uint8Array(32),
    nonce: new Uint8Array(32),
    valid_until_ms: 0n
  }
}
export const quoteDigest = (quote: OrderQuote) =>
  digest('dmsg/commerce/order/v1', quoteValue(quote))
export const orderId = (quote: OrderQuote) =>
  digest('dmsg/commerce/order-id/v1', [
    quote.home_commerce.toUint8Array(),
    quote.request.payer.owner.toUint8Array(),
    Uint8Array.from(quote.request.op_id)
  ])
export const claimDigest = (request: ClaimRequest) => {
  const value = membershipInput('request_claim', request) as any
  return digest('membership/claim-action/v1', [
    value.neuron_id,
    value.policy_version,
    value.benefit_id,
    value.expected_business_revision,
    value.term,
    value.change
  ])
}
export const claimId = (service: string, intent: MembershipIntent) =>
  digest('membership/claim-id/v1', [
    Principal.fromText(service).toUint8Array(),
    intent.actor.toUint8Array(),
    Uint8Array.from(intent.application_id)
  ])
