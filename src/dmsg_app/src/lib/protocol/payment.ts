import { IDL } from '@icp-sdk/core/candid'
import { Tagged } from 'cborg'
import { Principal } from '@icp-sdk/core/principal'
import { idlFactory } from '../canisters/generated/payment/index.js'
import { candidValue } from './account'
import { b64, unb64, unhex, digest, utf8 } from './codec'
import { xidBytes } from './identity'
import { ensure } from '../errors'
export function paymentAmount(value: unknown): bigint {
  if (value instanceof Tagged) {
    ensure(
      value.tag === 2 &&
        value.value instanceof Uint8Array &&
        value.value.length > 8 &&
        value.value.length <= 16 &&
        value.value[0] !== 0,
      'INTEGRITY_FAILED'
    )
    let amount = 0n
    for (const byte of value.value) amount = (amount << 8n) | BigInt(byte)
    return amount
  }
  ensure(
    typeof value === 'bigint' || (typeof value === 'number' && Number.isSafeInteger(value)),
    'INTEGRITY_FAILED'
  )
  const amount = BigInt(value)
  ensure(amount >= 0n && amount <= 0xffffffffffffffffn, 'INTEGRITY_FAILED')
  return amount
}
import type {
  OpenEscrow,
  PaymentOffer,
  Quote,
  SignedReceipt
} from '../canisters/generated/payment'
const service = idlFactory({ IDL }) as IDL.ServiceClass
export const paymentMethod = (method: string) => {
  const fn = service._fields.find(([name]) => name === method)?.[1]
  ensure(fn, 'UNSUPPORTED_PROTOCOL')
  return fn
}
const openType = paymentMethod('open_escrow').argTypes[0] as IDL.RecordClass
const offerWrapper = openType._fields.find(([name]) => name === 'offer')![1] as IDL.RecordClass
const offerType = offerWrapper._fields.find(([name]) => name === 'offer')![1]
const quoteType = openType._fields.find(([name]) => name === 'quote')![1]
const receiptType = (
  paymentMethod('finalize_receipt').argTypes[0] as IDL.RecordClass
)._fields.find(([name]) => name === 'receipt')![1]
export const offerDigest = (offer: PaymentOffer) =>
  digest('dmsg/payment-offer/v1', candidValue(offerType, offer))
export const deliveryQuoteDigest = (quote: Quote) =>
  digest('dmsg/quote/v2', candidValue(quoteType, quote))
export const admissionDigest = (receipt: SignedReceipt['receipt']) =>
  digest('dmsg/admission-receipt/v2', candidValue(receiptType, receipt))
export function encodePayment(method: string, args: unknown[]) {
  return b64(new Uint8Array(IDL.encode(paymentMethod(method).argTypes, args)))
}
export function decodePayment(method: string, args: string) {
  return IDL.decode(paymentMethod(method).argTypes, unb64(args)) as unknown[]
}
function fromWire(type: IDL.Type, value: any, name = ''): any {
  if (type instanceof IDL.OptClass)
    return value === null || value === undefined ? [] : [fromWire(type._type, value, name)]
  if (type instanceof IDL.VecClass) {
    if (type._type.name === 'nat8')
      return name === 'account_id'
        ? xidBytes(value)
        : name.includes('signature')
          ? unb64(value)
          : unhex(value)
    return value.map((v: unknown) => fromWire(type._type, v))
  }
  if (type instanceof IDL.RecordClass)
    return Object.fromEntries(
      type._fields.map(([field, child]) => [field, fromWire(child, value[field], field)])
    )
  if (type.name === 'principal') return Principal.fromText(value)
  if (['nat', 'nat64', 'int64'].includes(type.name)) return BigInt(value)
  return value
}
export function offerFromWire(value: any): OpenEscrow['offer'] {
  return fromWire(offerWrapper, value)
}
export function quoteFromWire(value: any): Quote {
  return fromWire(quoteType, value)
}
export function receiptFromWire(value: any): SignedReceipt {
  return fromWire(paymentMethod('finalize_receipt').argTypes[0]!, value)
}
export const paymentLeafKey = (kind: 'signer' | 'fee', version: bigint) => {
  const suffix = new Uint8Array(8)
  new DataView(suffix.buffer).setBigUint64(0, version)
  return Uint8Array.from([...utf8(kind + '/'), ...suffix])
}
