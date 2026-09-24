import { expect, it, vi } from 'vitest'
import type { HashTree } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import { HandleClient } from '../src/lib/services/handle'
import type { AccountClient } from '../src/lib/services/account'
import type {
  _SERVICE,
  LegacyReservation,
  SnapshotProgress
} from '../src/lib/canisters/generated/handle'
import { lookupCertifiedMap } from '../src/lib/services/certified'
import { utf8 } from '../src/lib/protocol/codec'

const principal = (n: number) => Principal.fromUint8Array(new Uint8Array([n, 1]))
const [nameAccount, admin, stranger] = [principal(80), principal(1), principal(2)]

function fixture(
  caller = admin,
  change: (reservation: LegacyReservation, progress: SnapshotProgress) => void = () => {}
) {
  const reservation: LegacyReservation = {
    handle: 'namedowner',
    legacy_owner: nameAccount,
    legacy_name_principal: [nameAccount],
    frozen_admins: [admin],
    quarantined: false
  }
  const progress: SnapshotProgress = {
    snapshot: [
      {
        source_canister: principal(81),
        snapshot_id: new Uint8Array(32).fill(5),
        freeze_version: 1n,
        event_tip: new Uint8Array(32).fill(7),
        count: 1n,
        entries_digest: new Uint8Array(32).fill(9)
      }
    ],
    imported: 1n,
    rolling_digest: new Uint8Array(32).fill(9),
    last_handle: ['namedowner'],
    sealed: true
  }
  change(reservation, progress)
  const registry = {
    get_legacy_reservation: vi.fn(async (name: string) => ({
      Ok: name === 'namedowner' ? ([reservation] as [LegacyReservation]) : ([] as [])
    })),
    snapshot_progress: vi.fn(async () => progress)
  }
  const client = new HandleClient(
    {} as AccountClient,
    registry as unknown as _SERVICE,
    'aaaaa-aa',
    caller
  )
  return { client, registry, progress }
}

it('reads one frozen name and the sealed snapshot with plain queries', async () => {
  const { client, registry, progress } = fixture()
  const result = await client.legacy('NAMEDOWNER')
  expect(result.reservation.handle).toBe('namedowner')
  expect(result.snapshot).toBe(progress.snapshot[0])
  expect(registry.get_legacy_reservation).toHaveBeenCalledExactlyOnceWith('namedowner')
  expect(registry.snapshot_progress).toHaveBeenCalledOnce()
})

it('requires a sealed snapshot and a record for the exact name', async () => {
  await expect(
    fixture(admin, (_, progress) => (progress.sealed = false)).client.legacy('namedowner')
  ).rejects.toMatchObject({ code: 'Pending' })
  await expect(fixture().client.legacy('missing')).rejects.toMatchObject({
    code: 'NOT_FOUND'
  })
  await expect(
    fixture(admin, (reservation) => (reservation.handle = 'othername')).client.legacy(
      'namedowner'
    )
  ).rejects.toThrow('INTEGRITY_FAILED')
})

it('keeps quarantined names, the name account and ordinary delegators out of claims', async () => {
  await expect(
    fixture(admin, (reservation) => (reservation.quarantined = true)).client.legacy(
      'namedowner'
    )
  ).rejects.toMatchObject({ code: 'Locked' })
  for (const caller of [nameAccount, stranger]) {
    await expect(fixture(caller).client.legacy('namedowner')).rejects.toMatchObject({
      code: 'Forbidden'
    })
  }
  const direct = fixture(stranger, (reservation) => {
    reservation.legacy_owner = stranger
    reservation.legacy_name_principal = []
  })
  expect((await direct.client.legacy('namedowner')).reservation.legacy_owner).toBe(stranger)
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
