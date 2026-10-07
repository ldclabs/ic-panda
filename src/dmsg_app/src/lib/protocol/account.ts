import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { idlFactory } from '../canisters/generated/user/index.js'
import type {
  AccountMutation,
  AttestRequest,
  CreateAccount,
  DelegationAuthority,
  DeriveRootRequest,
  DeviceInput,
  RecoveryRequest
} from '../canisters/generated/user'
import { Tagged, b64, bytes, digest, unb64 } from './codec'
import { xidBytes } from './identity'
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
  'Statement',
  'AppAction'
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
const method = (name: string): IDL.FuncClass => {
  const fn = service._fields.find(([key]) => key === name)?.[1]
  if (!fn) throw new Error(`UNSUPPORTED_PROTOCOL：${name}`)
  return fn
}
const field = (record: IDL.Type, name: string): IDL.Type => {
  const child =
    record instanceof IDL.RecordClass || record instanceof IDL.VariantClass
      ? record._fields.find(([key]) => key === name)?.[1]
      : undefined
  if (!child) throw new Error(`UNSUPPORTED_PROTOCOL：${name}`)
  return child
}
const mutationType = method('mutate_account').argTypes[0]!
const commandType = field(mutationType, 'command')
const statementType = field(method('attest').argTypes[0]!, 'statement')
const recoveryRequestType = method('request_recovery').argTypes[1]!
const delegationType = field(field(commandType, 'RegisterController'), 'delegation')
export type ControlMethod =
  | 'create_account'
  | 'mutate_account'
  | 'begin_auth_binding'
  | 'request_recovery'
  | 'complete_recovery'
  | 'derive_root'
  | 'attest'
  | 'attest_app_action'
  | 'approve_authentication'
  | 'approve_application'
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

/** `dmsg/device-approval/v2` over an operation domain and its command digest. */
export function approvalMessage(
  home: Principal,
  accountId: Uint8Array | number[],
  domain: string,
  command: unknown,
  approval: AccountMutation['approval']
) {
  return digest('dmsg/device-approval/v2', [
    home.toUint8Array(),
    Uint8Array.from(accountId),
    domain,
    Uint8Array.from(approval.device_id),
    approval.security_epoch,
    approval.sequence,
    Uint8Array.from(approval.request_id),
    approval.expires_at,
    digest(domain, command)
  ])
}
export function accountApprovalMessage(home: Principal, request: AccountMutation) {
  return approvalMessage(
    home,
    request.account_id,
    'dmsg/account/v2',
    [request.expected_version, candidValue(commandType, request.command)],
    request.approval
  )
}
export const accountOperationDigest = (request: AccountMutation) =>
  digest('dmsg/account-operation/v2', candidValue(mutationType, request))

/** Serde view of a Candid statement, as the attest approval binds it. */
export const statementValue = (statement: AttestRequest['statement']) =>
  candidValue(statementType, statement)
export const ATTEST_APPROVAL_DOMAIN = 'dmsg/attest/v1'
/** Approval over the statement, checked origin and the device document signature. */
export function attestApprovalMessage(
  home: Principal,
  request: Pick<AttestRequest, 'account_id' | 'statement' | 'origin' | 'signature' | 'approval'>
) {
  return approvalMessage(
    home,
    request.account_id,
    ATTEST_APPROVAL_DOMAIN,
    [statementValue(request.statement), request.origin, Uint8Array.from(request.signature)],
    request.approval
  )
}
export const DERIVE_APPROVAL_DOMAIN = 'dmsg/derive-root/v1'
export function deriveApprovalMessage(home: Principal, request: DeriveRootRequest) {
  return approvalMessage(
    home,
    request.account_id,
    DERIVE_APPROVAL_DOMAIN,
    [request.generation, Uint8Array.from(request.transport_public_key), request.max_cycles],
    request.approval
  )
}
/** Proof of possession a replacement device signs for a login recovery request. */
export const recoveryDeviceMessage = (
  home: Principal,
  accountId: string | Uint8Array,
  request: RecoveryRequest
) =>
  digest('dmsg/recovery-device/v1', [
    home.toUint8Array(),
    typeof accountId === 'string' ? xidBytes(accountId) : accountId,
    candidValue(recoveryRequestType, request)
  ])
/** Proof of possession a self-held controller key signs for its registration. */
export const controllerPopMessage = (
  home: Principal,
  accountId: Uint8Array,
  generation: number,
  delegation: DelegationAuthority,
  supersedes: number[],
  requestId: Uint8Array
) =>
  digest('dmsg/controller-pop/v1', [
    home.toUint8Array(),
    accountId,
    generation,
    candidValue(delegationType, delegation),
    supersedes,
    requestId
  ])
/** Recipients of a root bundle: active device IDs in ascending order plus the
 * generation, which names the vetKD recovery identity. */
export function rootRecipientsDigest(deviceIds: Uint8Array[], generation: number | bigint) {
  const ids = deviceIds.map((id) => {
    ensure(id.length === 32, 'INVALID_INPUT')
    return Uint8Array.from(id)
  })
  ids.sort((a, b) => {
    for (let i = 0; i < 32; i++) if (a[i] !== b[i]) return a[i] - b[i]
    return 0
  })
  return digest('dmsg/root-recipients/1', [ids, BigInt(generation)])
}
export const rootBundleDigest = (recipientsDigest: Uint8Array, bodyDigest: Uint8Array) =>
  digest('dmsg/root-bundle-digest/2', [recipientsDigest, bodyDigest])

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
