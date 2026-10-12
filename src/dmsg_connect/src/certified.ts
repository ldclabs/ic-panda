/**
 * Certified reads the bridge page trusts instead of the relay: the handle
 * registry's user homes, an account's devices from its user home, and an
 * application's registration from commerce. Every value is checked against
 * an IC certificate no older than 60 seconds under the pinned root key.
 */
import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { decodeCanonical as decodeContract, validateApp, type AppRegistration } from 'dmsg-sdk'
import { DmsgError } from 'dmsg-sdk/errors'
import { verifyCertifiedLeaf } from 'dmsg-sdk/ic'
import { decodeCanonical, digest, equal } from './codec.ts'
import { ensure, fail } from './errors.ts'

const Error = IDL.Variant({
  NotFound: IDL.Null,
  Forbidden: IDL.Null,
  Locked: IDL.Null,
  Expired: IDL.Null,
  Unavailable: IDL.Text,
  InvalidInput: IDL.Text
})
const CertifiedBatch = IDL.Record({
  certificate: IDL.Vec(IDL.Nat8),
  schema: IDL.Nat16,
  entries: IDL.Vec(IDL.Record({ key: IDL.Vec(IDL.Nat8), value: IDL.Opt(IDL.Vec(IDL.Nat8)), witness: IDL.Vec(IDL.Nat8) })),
  canister: IDL.Principal
})
const Capability = IDL.Variant({
  FormalApprove: IDL.Null,
  ContentSign: IDL.Null,
  VaultUnlock: IDL.Null,
  RootManage: IDL.Null,
  PaymentOffer: IDL.Null
})
const Device = IDL.Record({
  added_at: IDL.Nat64,
  added_by: IDL.Opt(IDL.Vec(IDL.Nat8)),
  revoked_at: IDL.Opt(IDL.Nat64),
  next_sequence: IDL.Nat64,
  input: IDL.Record({
    capabilities: IDL.Vec(Capability),
    role: IDL.Variant({ Administrator: IDL.Null, Member: IDL.Null }),
    device_id: IDL.Vec(IDL.Nat8),
    hpke_pub: IDL.Vec(IDL.Nat8),
    signing_pub: IDL.Vec(IDL.Nat8)
  })
})
const VaultWriteState = IDL.Variant({ RekeyRequired: IDL.Null, Ready: IDL.Null, Uninitialized: IDL.Null })
const SecuritySnapshot = IDL.Record({
  content_root_generation: IDL.Nat64,
  account_id: IDL.Vec(IDL.Nat8),
  account_version: IDL.Nat64,
  content_root_digest: IDL.Opt(IDL.Vec(IDL.Nat8)),
  devices_root: IDL.Vec(IDL.Nat8),
  principal_updated_at: IDL.Opt(IDL.Nat64),
  schema: IDL.Nat16,
  security_epoch: IDL.Nat64,
  issuer: IDL.Text,
  home_cose: IDL.Principal,
  home_user: IDL.Principal,
  recovery_delay_ms: IDL.Nat64,
  vault_write_state: VaultWriteState,
  pending_recovery_digest: IDL.Opt(IDL.Vec(IDL.Nat8))
})
const userIdl: IDL.InterfaceFactory = () =>
  IDL.Service({
    security_snapshot_batch: IDL.Func(
      [IDL.Vec(IDL.Vec(IDL.Nat8))],
      [IDL.Variant({ Ok: CertifiedBatch, Err: Error })],
      ['query']
    ),
    get_device_bundle: IDL.Func(
      [IDL.Vec(IDL.Nat8)],
      [IDL.Variant({ Ok: IDL.Tuple(SecuritySnapshot, IDL.Vec(IDL.Tuple(IDL.Vec(IDL.Nat8), Device))), Err: Error })],
      ['query']
    )
  })
const commerceIdl: IDL.InterfaceFactory = () =>
  IDL.Service({
    integration_configuration_certificate: IDL.Func(
      [IDL.Text, IDL.Opt(IDL.Text)],
      [IDL.Variant({ Ok: CertifiedBatch, Err: Error })],
      ['query']
    )
  })
const handleIdl: IDL.InterfaceFactory = () =>
  IDL.Service({
    get_handle_config: IDL.Func(
      [],
      [
        IDL.Record({
          max_pending: IDL.Nat32,
          issuer_namespace: IDL.Text,
          ledger: IDL.Principal,
          environment: IDL.Variant({ Local: IDL.Null, Staging: IDL.Null, Production: IDL.Null }),
          ledger_fee: IDL.Nat,
          registration_homes: IDL.Vec(IDL.Principal),
          governance: IDL.Principal,
          user_homes: IDL.Vec(IDL.Principal)
        })
      ],
      ['query']
    )
  })

type Batch = {
  certificate: Uint8Array | number[]
  schema: number
  entries: { key: Uint8Array | number[]; value: [] | [Uint8Array | number[]]; witness: Uint8Array | number[] }[]
  canister: Principal
}
type Answer<T> = { Ok: T } | { Err: Record<string, unknown> }
const bytes = (v: Uint8Array | number[]) => Uint8Array.from(v)

function ok<T>(answer: Answer<T>): T {
  if ('Ok' in answer) return answer.Ok
  const code = Object.keys(answer.Err)[0]
  fail(code === 'NotFound' ? 'NOT_FOUND' : 'UNAVAILABLE', code)
}

function view(batch: Batch) {
  return {
    schema: BigInt(batch.schema),
    canister: batch.canister.toUint8Array(),
    certificate: bytes(batch.certificate),
    entries: batch.entries.map((e) => ({
      key: bytes(e.key),
      value: e.value.length ? bytes(e.value[0]!) : null,
      witness: bytes(e.witness)
    }))
  }
}

/** An account device as the certified set describes it. */
export interface CertifiedDevice {
  device_id: Uint8Array
  signing_pub: Uint8Array
  hpke_pub: Uint8Array
  capabilities: string[]
  active: boolean
}

export interface CertifiedAccount {
  issuer: string
  home: Principal
  devices: CertifiedDevice[]
  at: bigint
}

export interface IcConfig {
  host: string
  rootKey: Uint8Array
  handle: string
  commerce: string
}

export type { AppRegistration }

export class Certified {
  readonly config: IcConfig
  private agent: Promise<HttpAgent>

  constructor(config: IcConfig) {
    this.config = config
    this.agent = HttpAgent.create({
      host: config.host,
      rootKey: config.rootKey,
      shouldFetchRootKey: false,
      verifyQuerySignatures: true
    })
  }

  private async actor(idl: IDL.InterfaceFactory, canisterId: string | Principal) {
    return Actor.createActor(idl, { agent: await this.agent, canisterId })
  }

  /** The registry's user homes. */
  async userHomes(): Promise<string[]> {
    const handle = await this.actor(handleIdl, this.config.handle)
    const config = (await handle.get_handle_config()) as { user_homes: Principal[] }
    return config.user_homes.map((p) => p.toText())
  }

  /** The certified devices of `account` at `home`, which must be a registry home. */
  async account(home: string, account: Uint8Array): Promise<CertifiedAccount> {
    ensure((await this.userHomes()).includes(home), 'INTEGRITY_FAILED', 'Unknown user home')
    const user = await this.actor(userIdl, home)
    const batch = ok((await user.security_snapshot_batch([account])) as Answer<Batch>)
    const [, devices] = ok(
      (await user.get_device_bundle(account)) as Answer<[unknown, [Uint8Array | number[], CandidDevice][]]>
    )
    const homeBytes = Principal.fromText(home).toUint8Array()
    const { value, at } = await verifyCertifiedLeaf(view(batch), account, homeBytes, this.config.rootKey, BigInt(Date.now()))
    const snapshot = decodeCanonical<Record<string, unknown>>(value)
    const wire = new Map(devices.map(([id, d]) => [bytes(id), wireDevice(d)] as const))
    ensure(
      snapshot.schema === 4 &&
        snapshot.account_id instanceof Uint8Array &&
        equal(snapshot.account_id, account) &&
        snapshot.home_user instanceof Uint8Array &&
        equal(snapshot.home_user, homeBytes) &&
        snapshot.devices_root instanceof Uint8Array &&
        equal(snapshot.devices_root, digest('dmsg/devices/v1', wire)),
      'INTEGRITY_FAILED'
    )
    return {
      issuer: snapshot.issuer as string,
      home: Principal.fromText(home),
      at,
      devices: devices.map(([, d]) => ({
        device_id: bytes(d.input.device_id),
        signing_pub: bytes(d.input.signing_pub),
        hpke_pub: bytes(d.input.hpke_pub),
        capabilities: d.input.capabilities.map((c) => Object.keys(c)[0]!),
        active: d.revoked_at.length === 0
      }))
    }
  }

  /** The certified registration of `appId`. */
  async application(appId: string): Promise<AppRegistration> {
    const commerce = await this.actor(commerceIdl, this.config.commerce)
    const answer = (await commerce.integration_configuration_certificate(appId, [])) as Answer<Batch>
    if ('Err' in answer && 'NotFound' in answer.Err) fail('APP_NOT_REGISTERED')
    const batch = ok(answer)
    const { value } = await verifyCertifiedLeaf(
      view(batch),
      digest('dmsg/registration/app/v1', appId),
      Principal.fromText(this.config.commerce).toUint8Array(),
      this.config.rootKey,
      BigInt(Date.now())
    )
    let app: AppRegistration
    try {
      app = decodeContract(value) as unknown as AppRegistration
      validateApp(app)
    } catch (error) {
      if (error instanceof DmsgError) fail('INTEGRITY_FAILED', `registration: ${error.code}`)
      throw error
    }
    ensure(app.app_id === appId, 'INTEGRITY_FAILED')
    return app
  }
}

type CandidDevice = {
  added_at: bigint
  added_by: [] | [Uint8Array | number[]]
  revoked_at: [] | [bigint]
  next_sequence: bigint
  input: {
    capabilities: Record<string, null>[]
    role: Record<string, null>
    device_id: Uint8Array | number[]
    hpke_pub: Uint8Array | number[]
    signing_pub: Uint8Array | number[]
  }
}

/** A device exactly as the user home's canonical device map encodes it. */
function wireDevice(d: CandidDevice) {
  return {
    input: {
      device_id: bytes(d.input.device_id),
      signing_pub: bytes(d.input.signing_pub),
      hpke_pub: bytes(d.input.hpke_pub),
      role: Object.keys(d.input.role)[0]!,
      capabilities: d.input.capabilities.map((c) => Object.keys(c)[0]!)
    },
    added_at: d.added_at,
    added_by: d.added_by.length ? bytes(d.added_by[0]!) : null,
    revoked_at: d.revoked_at.length ? d.revoked_at[0]! : null,
    next_sequence: d.next_sequence
  }
}
