import {
  Actor,
  Certificate,
  Cbor,
  HttpAgent,
  SignIdentity,
  lookup_path,
  lookupResultToBuffer,
  reconstruct,
  type Identity,
  type PublicKey,
  type Signature,
  type HashTree
} from '@icp-sdk/core/agent'
import { DelegationIdentity, Ed25519PublicKey } from '@icp-sdk/core/identity'
import { Principal } from '@icp-sdk/core/principal'
import { Signer } from '@icp-sdk/signer'
import { PostMessageTransport } from '@icp-sdk/signer/web'
import { idlFactory as userIDL } from '../canisters/generated/user/index.js'
import { idlFactory as handleIDL } from '../canisters/generated/handle/index.js'
import { idlFactory as coseIDL } from '../canisters/generated/cose/index.js'
import { idlFactory as paymentIDL } from '../canisters/generated/payment/index.js'
import type { _SERVICE as UserService, CertifiedBatch } from '../canisters/generated/user'
import type { _SERVICE as HandleService } from '../canisters/generated/handle'
import type { _SERVICE as CoseService } from '../canisters/generated/cose'
import type { _SERVICE as PaymentService } from '../canisters/generated/payment'
import type { CryptoClient } from '../crypto/client'
import { config } from '../config'
import { bytes, decodeCanonical, equal, unb64, unhex } from '../protocol/codec'
import { ensure } from '../errors'

class WorkerIdentity extends SignIdentity {
  constructor(
    private publicKey: PublicKey,
    private client: CryptoClient
  ) {
    super()
  }
  getPublicKey() {
    return this.publicKey
  }
  async sign(blob: Uint8Array): Promise<Signature> {
    return (await this.client.call('authSign', bytes(blob))) as Signature
  }
}
export async function login(
  client: CryptoClient,
  publicKey: string,
  derivationOrigin: string
): Promise<Identity> {
  ensure(
    config.derivationOrigins.includes(derivationOrigin) && config.canisters.user,
    'UNAVAILABLE',
    '请先在构建配置中指定新版用户服务。'
  )
  const identity = new WorkerIdentity(Ed25519PublicKey.fromRaw(unb64(publicKey)), client)
  const signer = new Signer({
    transport: new PostMessageTransport({
      url: 'https://id.ai/authorize',
      windowOpenerFeatures: 'width=576,height=680'
    }),
    derivationOrigin
  })
  await signer.openChannel()
  try {
    const chain = await signer.requestDelegation({
      publicKey: identity.getPublicKey(),
      maxTimeToLive: 15n * 60n * 1000000000n,
      targets: Object.values(config.canisters)
        .filter(Boolean)
        .map((p) => Principal.fromText(p))
    })
    // The private session key remains in the worker. The public delegation is
    // held in page memory; nothing is written to auth-client-db or storage.sync.
    return DelegationIdentity.fromDelegation(identity, chain)
  } finally {
    await signer.closeChannel()
  }
}
export async function services(identity?: Identity) {
  const agent = await HttpAgent.create({
    host: config.icHost,
    identity,
    verifyQuerySignatures: true,
    shouldFetchRootKey: false
  })
  if (
    config.environment === 'local' &&
    /^http:\/\/(localhost|127\.0\.0\.1)(:|\/|$)/.test(config.icHost)
  )
    await agent.fetchRootKey()
  const create = <T>(idl: Parameters<typeof Actor.createActor>[0], canisterId: string) =>
    canisterId ? Actor.createActor<T>(idl, { agent, canisterId }) : null
  return {
    agent,
    user: create<UserService>(userIDL, config.canisters.user),
    handle: create<HandleService>(handleIDL, config.canisters.handle),
    cose: create<CoseService>(coseIDL, config.canisters.cose),
    payment: create<PaymentService>(paymentIDL, config.canisters.payment)
  }
}
export function unwrap<T>(result: { Ok: T } | { Err: unknown }): T {
  if ('Ok' in result) return result.Ok
  throw new Error(
    `ICP 操作未完成：${JSON.stringify(result.Err, (_, v) => (typeof v === 'bigint' ? v.toString() : v))}`
  )
}
function leb128(data: Uint8Array): bigint {
  let value = 0n,
    shift = 0n
  ensure(data.length <= 10, 'INTEGRITY_FAILED')
  for (const byte of data) {
    value |= BigInt(byte & 127) << shift
    if (!(byte & 128)) return value
    shift += 7n
  }
  throw new Error('INTEGRITY_FAILED')
}
/** Verifies the actual public Rust contract (raw subject path), not the
 * candidate cloud security leaf. No relabeling between the two schemas. */
export async function verifySecurityBatch(
  batch: CertifiedBatch,
  agent: HttpAgent,
  subjectId: string
) {
  const expected = Principal.fromText(config.canisters.user)
  ensure(
    batch.canister.toText() === expected.toText() &&
      batch.schema === 1 &&
      batch.entries.length <= 64,
    'INTEGRITY_FAILED'
  )
  const certificate = await Certificate.create({
    certificate: bytes(Uint8Array.from(batch.certificate)),
    rootKey: agent.rootKey!,
    principal: { canisterId: expected },
    maxAgeInMinutes: 1
  })
  const time = lookupResultToBuffer(certificate.lookup_path(['time']))
  ensure(time, 'INTEGRITY_FAILED')
  const certifiedAt = Number(leb128(time) / 1000000n)
  ensure(
    Number.isSafeInteger(certifiedAt) &&
      certifiedAt <= Date.now() &&
      Date.now() < certifiedAt + 60000,
    'POLICY_STALE'
  )
  const certifiedData = lookupResultToBuffer(
    certificate.lookup_path(['canister', expected.toUint8Array(), 'certified_data'])
  )
  const entry = batch.entries.find((e) => equal(Uint8Array.from(e.key), unhex(subjectId)))
  ensure(
    entry && entry.value.length === 1 && entry.witness.length <= 262144,
    'INTEGRITY_FAILED'
  )
  const tree = Cbor.decode<HashTree>(Uint8Array.from(entry.witness))
  ensure(certifiedData && equal(await reconstruct(tree), certifiedData), 'INTEGRITY_FAILED')
  const value = lookupResultToBuffer(lookup_path([unhex(subjectId)], tree))
  ensure(value && equal(value, Uint8Array.from(entry.value[0]!)), 'INTEGRITY_FAILED')
  return {
    snapshot: decodeCanonical<Record<string, unknown>>(value),
    certifiedAt,
    expiresAt: certifiedAt + 60000
  }
}
