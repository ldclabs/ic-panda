// Bundled ONLY into a temporary test extension, never the shipped MV3 package.
import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { ed25519 } from '@noble/curves/ed25519.js'
import { idlFactory } from '../src/lib/canisters/generated/user/index.js'
import type { _SERVICE } from '../src/lib/canisters/generated/user'
import { readCloudSecurity, verifyCloudSecurity } from '../src/lib/services/cloud-security'
import { CloudClient, verifyCloudProfile } from '../src/lib/services/relay'
import { signCloudCommand, signCloudHttp } from '../src/lib/protocol/cloud'
import { b64, canonical, id, unb64, unhex } from '../src/lib/protocol/codec'
import { xidBytes } from '../src/lib/protocol/identity'

interface Fixture {
  test_only: boolean
  user: string
  account: string
  namespace: string
  gateway: string
  root_key_hex: string
  relay: string
  device_seed: number
}
async function run(fixture: Fixture) {
  if (!fixture.test_only) throw new Error('Test fixture required')
  const agent = await HttpAgent.create({
    host: fixture.gateway,
    shouldFetchRootKey: false,
    verifyQuerySignatures: true,
    rootKey: unhex(fixture.root_key_hex)
  })
  const actor = Actor.createActor<_SERVICE>(idlFactory, { agent, canisterId: fixture.user })
  const trust = {
    accountId: fixture.account,
    issuer: fixture.namespace + fixture.account,
    homeUser: fixture.user
  }
  const security = await readCloudSecurity(actor, agent, trust)
  const client = new CloudClient({ origin: fixture.relay, environment: 'local' })
  const readiness = await client.readiness()
  const seed = new Uint8Array(32).fill(fixture.device_seed)
  const sign = async (message: Uint8Array) => ed25519.sign(message, seed)
  const context = {
    ...trust,
    deviceId: '07'.repeat(32),
    securityEpoch: security.securityEpoch,
    requestId: id(),
    deadline: Math.min(Date.now() + 45_000, security.expiresAt)
  }
  const results: Record<string, unknown> = {
    readiness,
    published: await client.publishSecurity(security.evidence)
  }
  const profile = {
    version: 1,
    prev_hash: null,
    display_name: 'P0 local integration',
    bio: 'Signed by an enrolled test device',
    avatar_upload: null,
    links: []
  }
  const signed = await signCloudCommand(context, 'dmsg/profile/v1', profile, sign)
  const path = `/v1/accounts/${fixture.account}/profile`
  results.written = await client.post(path, signed, context, sign)
  results.retry = await client.post(path, signed, context, sign)
  const read = await client.get(path, { ...context, requestId: id() }, sign)
  results.profile = verifyCloudProfile(read, context, ed25519.getPublicKey(seed))
  const rejects = async (name: string, task: () => Promise<unknown>) => {
    try {
      await task()
      throw new Error(`Unexpected acceptance: ${name}`)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      if (message.startsWith('Unexpected acceptance:')) throw error
      results[name] = message
    }
  }
  await rejects('wrongCanister', () =>
    readCloudSecurity(actor, agent, { ...trust, homeUser: 'aaaaa-aa' })
  )
  await rejects('wrongAccount', () =>
    readCloudSecurity(actor, agent, { ...trust, accountId: '00000000000000000000' })
  )
  const snapshot = await actor.security_snapshot_batch([xidBytes(fixture.account)])
  const bundle = await actor.get_device_bundle(xidBytes(fixture.account))
  if (!('Ok' in snapshot) || !('Ok' in bundle)) throw new Error('Missing public evidence')
  await rejects('staleEvidence', () =>
    verifyCloudSecurity(snapshot.Ok, bundle.Ok, agent, trust, Date.now() + 120_000)
  )
  const modifiedBundle = structuredClone(bundle.Ok)
  modifiedBundle[1][0][1].next_sequence += 1n
  await rejects('modifiedDevice', () =>
    verifyCloudSecurity(snapshot.Ok, modifiedBundle, agent, trust)
  )
  const changed = structuredClone(security.evidence)
  const certificate = unb64(changed.certificate)
  certificate[certificate.length - 1] ^= 1
  changed.certificate = b64(certificate)
  await rejects('badCertificate', () => client.publishSecurity(changed))
  const altered = unb64(signed.cose_sign1)
  altered[altered.length - 1] ^= 1
  await rejects('tamperedCommand', () =>
    client.post(path, { cose_sign1: b64(altered) }, context, sign)
  )
  await rejects('wrongProtocol', () =>
    Promise.resolve(
      new CloudClient({
        origin: fixture.relay,
        environment: 'local',
        protocol: 'dmsg-cloud/0.1'
      })
    )
  )
  await rejects('wrongDeviceKey', () =>
    client.post(path, signed, context, async (message) =>
      ed25519.sign(message, new Uint8Array(32).fill(8))
    )
  )
  // An authenticated request cannot rewrite another account's profile.
  await rejects('wrongResource', () =>
    client.post('/v1/accounts/00000000000000000000/profile', signed, context, sign)
  )
  const originalBody = canonical(signed)
  const pop = await signCloudHttp(
    context,
    new URL(path, fixture.relay),
    'POST',
    originalBody,
    sign
  )
  const response = await fetch(new URL(path, fixture.relay), {
    method: 'POST',
    headers: { 'content-type': 'application/cbor', 'x-dmsg-pop': pop },
    body: canonical({ cose_sign1: b64(altered) })
  })
  if (response.ok) throw new Error('Tampered HTTP body accepted')
  results.tamperedBodyStatus = response.status
  const finalRead = await client.get(path, { ...context, requestId: id() }, sign)
  results.finalProfile = verifyCloudProfile(finalRead, context, ed25519.getPublicKey(seed))
  return results
}
Object.assign(window, { cloudProbe: run })
