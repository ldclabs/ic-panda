import { readFileSync } from 'node:fs'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { HttpAgent, HashTree } from '@icp-sdk/core/agent'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { HandleClient } from '../src/lib/services/handle'
import type { AccountClient } from '../src/lib/services/account'
import type {
  _SERVICE,
  CertifiedLegacyReservation
} from '../src/lib/canisters/generated/handle'
import { idlFactory } from '../src/lib/canisters/generated/handle/index.js'
import { lookupCertifiedMap } from '../src/lib/services/certified'
import { decodeCanonical, utf8 } from '../src/lib/protocol/codec'

const [root, at, registryId, owner, replies] = decodeCanonical<
  [Uint8Array, number, string, string, Uint8Array[]]
>(new Uint8Array(readFileSync(new URL('./fixtures/handle-reservation.cbor', import.meta.url))))
const service = idlFactory({ IDL }) as IDL.ServiceClass
const method = service._fields.find(
  ([name]) => name === 'get_legacy_reservation_certified'
)![1]

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(at)
})
afterEach(() => vi.useRealTimers())

function fixture(index = 0) {
  const [response] = IDL.decode(method.retTypes, replies[index]) as unknown as [
    { Ok: CertifiedLegacyReservation }
  ]
  const registry = { get_legacy_reservation_certified: vi.fn(async () => response) }
  // No network or mocked proof verification: the real BLS verifier uses this
  // local PocketIC root and the Candid reply emitted by the Rust canister.
  const agent = { rootKey: root } as HttpAgent
  const client = new HandleClient(
    { agent } as AccountClient,
    registry as unknown as _SERVICE,
    registryId,
    Principal.fromText(owner)
  )
  return { data: response.Ok, registry, client }
}

it('authenticates a single old name and frozen administrator in one query', async () => {
  const { client, registry } = fixture()
  expect((await client.legacy('NAMEDOWNER')).reservation.handle).toBe('namedowner')
  expect(registry.get_legacy_reservation_certified).toHaveBeenCalledExactlyOnceWith(
    'namedowner'
  )
})

it('rejects a changed owner and changed import progress', async () => {
  let f = fixture()
  f.data.reservation[0]!.legacy_owner = Principal.anonymous()
  await expect(f.client.legacy('namedowner')).rejects.toThrow('INTEGRITY_FAILED')
  f = fixture()
  f.data.progress.imported += 1n
  await expect(f.client.legacy('namedowner')).rejects.toThrow('INTEGRITY_FAILED')
})

it('requires an authenticated absence proof before accepting a missing reservation', async () => {
  const { client, data } = fixture()
  data.reservation = []
  await expect(client.legacy('namedowner')).rejects.toThrow('INTEGRITY_FAILED')
  await expect(fixture(1).client.legacy('missing')).rejects.toMatchObject({
    code: 'NOT_FOUND'
  })
})

it('rejects a record or proof requested under another name', async () => {
  const { client, data } = fixture()
  data.reservation[0]!.handle = 'othername'
  await expect(client.legacy('namedowner')).rejects.toThrow('INTEGRITY_FAILED')
  await expect(fixture().client.legacy('othername')).rejects.toThrow('INTEGRITY_FAILED')
})

it('rejects a forged certificate and stale certified progress', async () => {
  const { client, data } = fixture()
  const certificate = Uint8Array.from(data.proof.certificate)
  certificate[certificate.length - 1] ^= 1
  data.proof.certificate = certificate
  await expect(client.legacy('namedowner')).rejects.toThrow()
  vi.setSystemTime(at + 60001)
  await expect(fixture().client.legacy('namedowner')).rejects.toThrow('POLICY_STALE')
})

it('uses lexicographic bounds and never treats a pruned map range as absent', () => {
  const lookup = (tree: unknown, key: Uint8Array) => lookupCertifiedMap(tree as HashTree, key)
  const pruned = [4, new Uint8Array(32)]
  expect(lookup([0], utf8('anything'))).toEqual({ status: 'Absent' })
  expect(lookup(pruned, utf8('anything'))).toEqual({ status: 'Unknown' })
  // "az" is below "ba" despite its second byte being greater; a prefix also
  // sorts before its extension. Both were problematic in the SDK comparator.
  for (const [key, bound] of [
    ['az', 'ba'],
    ['a', 'ab']
  ]) {
    expect(lookup([1, [2, utf8(bound), pruned], pruned], utf8(key))).toEqual({
      status: 'Absent'
    })
    expect(lookup([1, pruned, [2, utf8(bound), pruned]], utf8(key))).toEqual({
      status: 'Unknown'
    })
  }
  expect(lookup([1, [2, utf8('aa'), pruned], [2, utf8('cc'), pruned]], utf8('bb'))).toEqual({
    status: 'Absent'
  })
  expect(lookup([1, [2, utf8('aa'), pruned], pruned], utf8('bb'))).toEqual({
    status: 'Unknown'
  })
  expect(lookup([2, utf8('aa'), [3, utf8('value')]], utf8('aa'))).toEqual({
    status: 'Found',
    value: utf8('value')
  })
})
