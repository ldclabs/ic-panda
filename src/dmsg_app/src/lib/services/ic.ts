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
import { xidBytes, xidText } from '../protocol/identity'
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
/** Internet Identity login to the worker's session key. Works before the
 * workspace is unlocked, because unlocking itself needs the login. */
export async function login(
  client: CryptoClient,
  derivationOrigin: string,
  explicitTargets?: string[]
): Promise<Identity> {
  ensure(
    config.derivationOrigins.includes(derivationOrigin) && config.canisters.handle,
    'UNAVAILABLE',
    '请先在构建配置中指定名称注册表。'
  )
  const publicKey = await client.call('authPublicKey')
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
    const targets =
      explicitTargets ??
      [
        config.canisters.handle,
        config.canisters.cose,
        config.canisters.commerce,
        config.canisters.membership,
        config.canisters.payment,
        ...(await userHomes()).userHomes
      ].filter(Boolean)
    const chain = await signer.requestDelegation({
      publicKey: identity.getPublicKey(),
      maxTimeToLive: 15n * 60n * 1000000000n,
      targets: targets.map((p) => Principal.fromText(p))
    })
    // The private session key remains in the worker. The public delegation is
    // held in page memory; nothing is written to auth-client-db or storage.sync.
    return DelegationIdentity.fromDelegation(identity, chain)
  } finally {
    await signer.closeChannel()
  }
}
export async function agentFor(identity?: Identity) {
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
  return agent
}
export const userActor = (agent: HttpAgent, home: string) =>
  Actor.createActor<UserService>(userIDL, { agent, canisterId: home })
export async function services(identity?: Identity) {
  const agent = await agentFor(identity)
  const create = <T>(idl: Parameters<typeof Actor.createActor>[0], canisterId: string) =>
    canisterId ? Actor.createActor<T>(idl, { agent, canisterId }) : null
  return {
    agent,
    handle: create<HandleService>(handleIDL, config.canisters.handle),
    cose: create<CoseService>(coseIDL, config.canisters.cose),
    commerce: create<CommerceService>(commerceIDL, config.canisters.commerce),
    membership: create<MembershipService>(membershipIDL, config.canisters.membership),
    payment: create<PaymentService>(paymentIDL, config.canisters.payment)
  }
}
/** The governed home list of the handle registry, the authority on which
 * user canisters belong to this deployment. Cached briefly per page. */
let homesCache: { at: number; value: { userHomes: string[]; registrationHomes: string[] } } | null =
  null
export async function userHomes(fresh = false) {
  if (!fresh && homesCache && Date.now() - homesCache.at < 300000) return homesCache.value
  const { handle } = await services()
  ensure(handle, 'UNAVAILABLE', '请先在构建配置中指定名称注册表。')
  const cfg = await handle.get_handle_config()
  const value = {
    userHomes: cfg.user_homes.map((p) => p.toText()),
    registrationHomes: cfg.registration_homes.map((p) => p.toText())
  }
  ensure(
    value.userHomes.length > 0 &&
      value.registrationHomes.every((home) => value.userHomes.includes(home)) &&
      config.canisters.userHomes.every((home) => value.userHomes.includes(home)),
    'INTEGRITY_FAILED',
    '名称注册表的用户服务列表与构建配置不一致。'
  )
  homesCache = { at: Date.now(), value }
  return value
}
/** A build-time pinned home is trusted offline; others are checked against the registry. */
export async function assertAccountHome(home: string) {
  if (config.canisters.userHomes.includes(home)) return
  ensure((await userHomes()).userHomes.includes(home), 'FORBIDDEN', '账户所在服务不在当前列表中。')
}
/** Find the login's account across every home; any query failure is unknown,
 * never a reason to register again. */
export async function locateAccount(agent: HttpAgent) {
  const { userHomes: homes } = await userHomes()
  const found = await Promise.all(
    homes.map(async (home) => {
      const result = await userActor(agent, home).my_account()
      return result.length ? { home, account: xidText(Uint8Array.from(result[0]!)) } : null
    })
  )
  return found.find((x) => x !== null) ?? null
}
export async function registrationHome() {
  const { registrationHomes } = await userHomes(true)
  ensure(registrationHomes.length > 0, 'UNAVAILABLE', '当前没有接收新账户的用户服务。')
  return registrationHomes[Math.floor(Math.random() * registrationHomes.length)]
}
/** Identity/authorization evidence is promoted only after certificate and
 * artifact binding checks. This says nothing about external TSA trust or the
 * signer's present permissions. Retain the certificate for later auditing. */
export async function verifyExecutionReceipt(
  batch: CertifiedBatch,
  agent: HttpAgent,
  homeUser: string,
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
    homeUser,
    key,
    null
  )
  const receipt = decodeCanonical<Record<string, unknown>>(value)
  const checked = verifyDocumentArtifact(artifact)
  const matches = (actual: unknown, expected: Uint8Array) =>
    actual instanceof Uint8Array && equal(actual, expected)
  ensure(
    receipt.schema === 2 &&
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
