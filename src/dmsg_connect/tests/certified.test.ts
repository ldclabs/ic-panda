import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import { Certified } from '../src/certified.ts'
import { canonical, digest } from '../src/codec.ts'
import type { CertifiedBatch } from 'dmsg-sdk/ic'
import { ConnectError } from '../src/errors.ts'
// Reuse the SDK synthetic authority as the public profile-reader tests do.
const fixtureUrl = new URL('../../../packages/dmsg-sdk/tests/principal-fixture.ts', import.meta.url)
const f = await import(fixtureUrl.href)

test('bridge reads certified legacy and opt-in recovery policies, rejecting missing or ambiguous flags', async t => {
  t.mock.method(Date, 'now', () => Number(f.now))
  t.mock.method(HttpAgent, 'create', async () => ({}))
  const id = new Uint8Array(32).fill(4)
  const input = { device_id: id, signing_pub: id, hpke_pub: id, role: 'Administrator', capabilities: ['VaultUnlock', 'WalletApprove'] }
  const device = { input, added_at: 0n, added_by: null, revoked_at: null, next_sequence: 0n }
  const wire = new Map([[id, device]])
  const candidDevice = { ...device, input: { ...input, role: { Administrator: null }, capabilities: [{ VaultUnlock: null }, { WalletApprove: null }] }, added_by: [], revoked_at: [] }
  let policy: Record<string, unknown> = { schema: 4 }
  const user = {
    get_device_bundle: async () => ({ Ok: [{}, [[id, candidDevice]]] }),
    security_snapshot_batch: async () => {
      const proof: CertifiedBatch = await f.certified(f.home, f.accountId, canonical({ ...policy, issuer: f.issuer, account_id: f.accountId, home_user: f.home, devices_root: digest('dmsg/devices/v1', wire) }))
      return { Ok: { ...proof, schema: Number(proof.schema), canister: Principal.fromUint8Array(proof.canister), entries: proof.entries.map(e => ({ ...e, value: e.value === null ? [] : [e.value] })) } }
    }
  }
  const home = Principal.fromUint8Array(f.home).toText()
  t.mock.method(Actor, 'createActor', (_idl: unknown, options: { canisterId: string | Principal }) => String(options.canisterId) === home ? user : { get_handle_config: async () => ({ user_homes: [Principal.fromUint8Array(f.home)] }) })
  const reader = new Certified({ host: 'http://127.0.0.1:1', rootKey: f.rootKey, handle: 'aaaaa-aa', commerce: 'aaaaa-aa' })
  for (const value of [{ schema: 4 }, { schema: 5, recovery_enabled: false }, { schema: 5, recovery_enabled: true }]) {
    policy = value
    const account = await reader.account(home, f.accountId)
    assert.equal(account.devices[0].active, true)
    assert.deepEqual(account.devices[0].capabilities, ['VaultUnlock', 'WalletApprove'])
  }
  for (const value of [{ schema: 5 }, { schema: 4, recovery_enabled: false }, { schema: 5, recovery_enabled: 'false' }]) {
    policy = value
    await assert.rejects(() => reader.account(home, f.accountId), error => error instanceof ConnectError && error.code === 'INTEGRITY_FAILED')
  }
})
