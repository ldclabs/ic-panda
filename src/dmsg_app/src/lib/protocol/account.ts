import { Tagged } from 'cborg'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { idlFactory } from '../canisters/generated/user/index.js'
import type {
  AccountCommand,
  AccountMutation,
  CreateAccount,
  DeviceInput
} from '../canisters/generated/user'
import { b64, bytes, digest, unb64 } from './codec'
import { ensure } from '../errors'

// Candid encodes unit variants as { Name: null }; Rust serde encodes them as
// text. Principals and fixed byte strings must remain CBOR byte strings.
const units = new Set([
  'Administrator',
  'Member',
  'ContentSign',
  'VaultUnlock',
  'RootManage',
  'FormalApprove',
  'PaymentOffer',
  'FileAttestation',
  'Statement'
])
export function accountValue(value: unknown): unknown {
  if (value instanceof Principal) return value.toUint8Array()
  if (value instanceof Uint8Array || value === null || typeof value !== 'object') return value
  if (Array.isArray(value)) {
    if (value.length && value.every((v) => Number.isInteger(v) && v >= 0 && v <= 255))
      return Uint8Array.from(value)
    return value.map(accountValue)
  }
  const entries = Object.entries(value)
  if (entries.length === 1 && entries[0][1] === null && units.has(entries[0][0]))
    return entries[0][0]
  return Object.fromEntries(entries.map(([k, v]) => [k, accountValue(v)]))
}

const service = idlFactory({ IDL }) as IDL.ServiceClass
/** Convert using the actual Candid type; empty opt and empty vec are different. */
export function candidValue(type: IDL.Type, value: any): unknown {
  if (type instanceof IDL.RecClass) {
    const inner = type.getType()
    ensure(inner, 'UNSUPPORTED_PROTOCOL')
    return candidValue(inner, value)
  }
  if (type instanceof IDL.OptClass) {
    ensure(Array.isArray(value) && value.length <= 1, 'INVALID_INPUT')
    return value.length ? candidValue(type._type, value[0]) : null
  }
  if (type instanceof IDL.VecClass)
    return type._type.name === 'nat8'
      ? Uint8Array.from(value)
      : Array.from(value as unknown[], (v) => candidValue(type._type, v))
  if (type instanceof IDL.TupleClass)
    return type._fields.map(([, child], i) => candidValue(child, value[i]))
  if (type instanceof IDL.RecordClass)
    return Object.fromEntries(
      type._fields.map(([name, child]) => [name, candidValue(child, value[name])])
    )
  if (type instanceof IDL.VariantClass) {
    const entries = Object.entries(value)
    ensure(entries.length === 1, 'INVALID_INPUT')
    const [name, childValue] = entries[0],
      child = type._fields.find(([key]) => key === name)?.[1]
    ensure(child, 'UNSUPPORTED_PROTOCOL')
    return child.name === 'null' ? name : { [name]: candidValue(child, childValue) }
  }
  if (typeof value === 'bigint' && value > 0xffffffffffffffffn) {
    ensure(value <= (1n << 128n) - 1n && type.name === 'nat', 'INVALID_INPUT')
    let hex = value.toString(16)
    if (hex.length % 2) hex = '0' + hex
    return new Tagged(
      2,
      Uint8Array.from(hex.match(/../g)!, (b) => parseInt(b, 16))
    )
  }
  return value instanceof Principal ? value.toUint8Array() : value
}
const mutationType = service._fields.find(([name]) => name === 'mutate_account')![1]
  .argTypes[0] as IDL.RecordClass
const commandType = mutationType._fields.find(([name]) => name === 'command')![1]
export type ControlMethod =
  | 'create_account'
  | 'mutate_account'
  | 'begin_auth_binding'
  | 'request_recovery'
  | 'reconfirm_recovery'
  | 'complete_recovery'
  | 'derive_root'
  | 'sign'
export function encodeControl(method: ControlMethod, args: unknown[]) {
  const fn = service._fields.find(([name]) => name === method)?.[1]
  ensure(fn, 'UNSUPPORTED_PROTOCOL')
  return b64(new Uint8Array(IDL.encode(fn.argTypes, args)))
}
export function decodeControl(method: ControlMethod, encoded: string): unknown[] {
  const fn = service._fields.find(([name]) => name === method)?.[1]
  ensure(fn && encoded.length <= 200000, 'INVALID_INPUT')
  return IDL.decode(fn.argTypes, bytes(unb64(encoded)))
}
export function encodeControlResult(method: ControlMethod, result: unknown) {
  const fn = service._fields.find(([name]) => name === method)?.[1]
  ensure(fn, 'UNSUPPORTED_PROTOCOL')
  return b64(new Uint8Array(IDL.encode(fn.retTypes, [result])))
}
export function decodeControlResult(method: ControlMethod, encoded: string): unknown {
  const fn = service._fields.find(([name]) => name === method)?.[1]
  ensure(fn && encoded.length <= 200000, 'INVALID_INPUT')
  return IDL.decode(fn.retTypes, bytes(unb64(encoded)))[0]
}
export const createAccountMessage = (
  home: Principal,
  caller: Principal,
  request: Omit<CreateAccount, 'proof'>
) =>
  digest('dmsg/create-account/v1', [
    home.toUint8Array(),
    caller.toUint8Array(),
    accountValue(request.device),
    Uint8Array.from(request.op_id),
    request.expires_at
  ])

export function accountApprovalMessage(home: Principal, request: AccountMutation) {
  const a = request.approval
  return digest('dmsg/device-approval/v2', [
    home.toUint8Array(),
    Uint8Array.from(request.account_id),
    'dmsg/account/v2',
    Uint8Array.from(a.device_id),
    a.security_epoch,
    a.sequence,
    Uint8Array.from(a.request_id),
    a.expires_at,
    digest('dmsg/account/v2', [
      request.expected_version,
      candidValue(commandType, request.command)
    ])
  ])
}
export const accountOperationDigest = (request: AccountMutation) =>
  digest('dmsg/account-operation/v2', candidValue(mutationType, request))

export function deviceInput(
  meta: { deviceId: string; signingPublic: string; hpkePublic: string },
  role: 'Administrator' | 'Member' = 'Administrator'
): DeviceInput {
  ensure(/^[0-9a-f]{64}$/.test(meta.deviceId), 'INVALID_INPUT')
  return {
    device_id: Uint8Array.from(meta.deviceId.match(/../g)!, (x) => parseInt(x, 16)),
    signing_pub: unb64(meta.signingPublic),
    hpke_pub: unb64(meta.hpkePublic),
    role: role === 'Administrator' ? { Administrator: null } : { Member: null },
    capabilities:
      role === 'Administrator'
        ? [{ ContentSign: null }, { VaultUnlock: null }, { RootManage: null }]
        : [{ ContentSign: null }, { VaultUnlock: null }]
  }
}
export function commandName(command: AccountCommand) {
  return Object.keys(command)[0]
}
