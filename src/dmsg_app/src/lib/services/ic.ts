import { certifiedValue } from './certified'
import {
  Actor,
  HttpAgent,
  SignIdentity,
  type Identity,
  type PublicKey,
  type Signature
} from '@icp-sdk/core/agent'
import { DelegationIdentity, Ed25519PublicKey } from '@icp-sdk/core/identity'
import { Principal } from '@icp-sdk/core/principal'
import { Signer } from '@icp-sdk/signer'
import { PostMessageTransport } from '@icp-sdk/signer/web'
import { idlFactory as userIDL } from '../canisters/generated/user/index.js'
import { idlFactory as handleIDL } from '../canisters/generated/handle/index.js'
import { idlFactory as coseIDL } from '../canisters/generated/cose/index.js'
import { idlFactory as commerceIDL } from '../canisters/generated/commerce/index.js'
import { idlFactory as membershipIDL } from '../canisters/generated/membership/index.js'
import type { _SERVICE as CommerceService } from '../canisters/generated/commerce'
import type { _SERVICE as MembershipService } from '../canisters/generated/membership'
import { idlFactory as paymentIDL } from '../canisters/generated/payment/index.js'
import type { _SERVICE as UserService, CertifiedBatch } from '../canisters/generated/user'
import type { _SERVICE as HandleService } from '../canisters/generated/handle'
import type { _SERVICE as CoseService } from '../canisters/generated/cose'
import type { _SERVICE as PaymentService } from '../canisters/generated/payment'
import type { CryptoClient } from '../crypto/client'
import { config } from '../config'
import { bytes, decodeCanonical, equal, unb64, unhex, utf8, hash } from '../protocol/codec'
import { ensure } from '../errors'
import { xidBytes } from '../protocol/identity'
import { verifyDocumentArtifact, type Artifact } from '../protocol/statements'

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
  derivationOrigin: string,
  explicitTargets?: string[]
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
      targets: (explicitTargets ?? Object.values(config.canisters))
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
    commerce: create<CommerceService>(commerceIDL, config.canisters.commerce),
    membership: create<MembershipService>(membershipIDL, config.canisters.membership),
    payment: create<PaymentService>(paymentIDL, config.canisters.payment)
  }
}
export function unwrap<T>(result: { Ok: T } | { Err: unknown }): T {
  if ('Ok' in result) return result.Ok
  throw new Error(
    `ICP 操作未完成：${JSON.stringify(result.Err, (_, v) => (typeof v === 'bigint' ? v.toString() : v))}`
  )
}
export async function verifySecurityBatch(
  batch: CertifiedBatch,
  agent: HttpAgent,
  accountId: string,
  issuer: string
) {
  const account = xidBytes(accountId)
  const { value, certifiedAt, expiresAt } = await certifiedValue(
    batch,
    agent,
    config.canisters.user,
    account
  )
  const current = Date.now()
  ensure(certifiedAt <= current && current < expiresAt, 'POLICY_STALE')
  const snapshot = decodeCanonical<Record<string, unknown>>(value)
  ensure(
    snapshot.schema === 2 &&
      snapshot.account_id instanceof Uint8Array &&
      equal(snapshot.account_id, account) &&
      snapshot.issuer === issuer,
    'INTEGRITY_FAILED'
  )
  return { snapshot, certifiedAt, expiresAt }
}
/** Identity/authorization evidence is promoted only after certificate and
 * artifact binding checks. This says nothing about external TSA trust or the
 * signer's present permissions. Retain the certificate for later auditing. */
export async function verifyExecutionReceipt(
  batch: CertifiedBatch,
  agent: HttpAgent,
  accountId: string,
  requestId: string,
  issuer: string,
  artifact: Artifact
) {
  const account = xidBytes(accountId),
    request = unhex(requestId)
  ensure(request.length === 32, 'INVALID_INPUT')
  const key = Uint8Array.from([...utf8('execution/'), ...account, ...request])
  const { value, certifiedAt, expiresAt } = await certifiedValue(
    batch,
    agent,
    config.canisters.user,
    key,
    null
  )
  const receipt = decodeCanonical<Record<string, unknown>>(value)
  const checked = verifyDocumentArtifact(artifact)
  const matches = (actual: unknown, expected: Uint8Array) =>
    actual instanceof Uint8Array && equal(actual, expected)
  ensure(
    receipt.schema === 1 &&
      receipt.status === 'Completed' &&
      receipt.issuer === issuer &&
      checked.statement.issuer === issuer &&
      matches(receipt.account_id, account) &&
      matches(receipt.request_id, request) &&
      matches(receipt.to_be_signed_digest, unhex(hash(checked.toBeSigned))) &&
      matches(receipt.public_key_fingerprint, checked.keyFingerprint) &&
      matches(receipt.signature_digest, unhex(hash(checked.signature))),
    'INTEGRITY_FAILED'
  )
  return {
    receipt,
    certifiedAt,
    expiresAt,
    verification: {
      ...checked,
      checks: {
        ...checked.checks,
        issuerBinding: 'verified',
        authorization: 'verified'
      } as const
    }
  }
}
