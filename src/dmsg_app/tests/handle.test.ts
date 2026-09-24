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
import { decodeClaim, legacyClaimDigest } from '../src/lib/protocol/handle'
import { xidText } from '../src/lib/protocol/identity'

const principal = (n: number) => Principal.fromUint8Array(new Uint8Array([n, 1]))
const [nameAccount, admin, stranger] = [principal(80), principal(1), principal(2)]
const accountBytes = new Uint8Array(12).fill(1)

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
  const storage = new Map<string, string>()
  const account = {
    meta: { account: { id: xidText(accountBytes) } },
    crypto: {
      call: async (method: string, key: string, value?: string) => {
        if (method === 'controlPut') storage.set(key, value!)
        return storage.get(key) ?? null
      }
    },
    pending: vi.fn(async () => null),
    mutate: vi.fn(async () => {})
  }
  const registry = {
    get_legacy_reservation: vi.fn(async (name: string) => ({
      Ok: name === 'namedowner' ? ([reservation] as [LegacyReservation]) : ([] as [])
    })),
    snapshot_progress: vi.fn(async () => progress),
    claim_legacy_handle: vi.fn<_SERVICE['claim_legacy_handle']>().mockResolvedValue({
      Ok: {
        handle: reservation.handle,
        owner_account: accountBytes,
        version: 1n,
        event_tip: new Uint8Array(32).fill(8)
      }
    })
  }
  const client = new HandleClient(
    account as unknown as AccountClient,
    registry as unknown as _SERVICE,
    'aaaaa-aa',
    caller
  )
  return { client, registry, progress, reservation, account }
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

it.each(['snapshot', 'record'] as const)(
  'can prepare a fresh claim after the canister rejects a forged %s query',
  async (forged) => {
    const { client, registry, progress, reservation, account } = fixture()
    vi.spyOn(client, 'ownership').mockResolvedValue(null)
    const snapshot = Uint8Array.from(progress.snapshot[0]!.snapshot_id)
    if (forged === 'snapshot') {
      registry.snapshot_progress.mockResolvedValueOnce({
        ...progress,
        snapshot: [{ ...progress.snapshot[0]!, snapshot_id: new Uint8Array(32).fill(6) }]
      })
    } else {
      registry.get_legacy_reservation.mockResolvedValueOnce({
        Ok: [{ ...reservation, frozen_admins: [admin, stranger] }]
      })
    }
    const { job: original } = await client.prepare('namedowner')
    const error = forged === 'snapshot' ? 'VersionConflict' : 'IntegrityFailed'
    registry.claim_legacy_handle.mockResolvedValueOnce({
      Err: forged === 'snapshot' ? { VersionConflict: null } : { IntegrityFailed: null }
    })
    await expect(client.run()).rejects.toMatchObject({ code: error })
    expect((await client.job())?.phase).toBe('rejected')
    await expect(client.run()).rejects.toMatchObject({ code: 'VersionConflict' })
    expect(registry.claim_legacy_handle).toHaveBeenCalledTimes(1)

    // Reload the persisted rejection, then fetch fresh records and authorize a
    // new intent instead of resubmitting the poisoned immutable arguments.
    const reconnected = new HandleClient(
      client.account,
      client.registry,
      client.registryId,
      client.oldCaller
    )
    vi.spyOn(reconnected, 'ownership').mockResolvedValue(null)
    const { job: fresh } = await reconnected.prepare('namedowner')
    const [intent, freshSnapshot] = decodeClaim(fresh.args)
    expect(intent.op_id).not.toEqual(decodeClaim(original.args)[0].op_id)
    expect(freshSnapshot).toEqual(snapshot)
    expect(intent.terms_digest).toEqual(legacyClaimDigest(snapshot, reservation, accountBytes))
    expect((await reconnected.run()).phase).toBe('claimed')
    expect(account.mutate).toHaveBeenLastCalledWith(fresh.account, {
      AuthorizeHandle: { intent }
    })
    expect(registry.claim_legacy_handle).toHaveBeenLastCalledWith(intent, freshSnapshot)
  }
)

it.each(['transport', 'execution'] as const)(
  'keeps the exact claim for retry when its %s result is unknown',
  async (failure) => {
    const { client, registry } = fixture()
    vi.spyOn(client, 'ownership').mockResolvedValue(null)
    const { job: original } = await client.prepare('namedowner')
    if (failure === 'transport') {
      registry.claim_legacy_handle.mockRejectedValueOnce(new Error('response lost'))
    } else {
      registry.claim_legacy_handle.mockResolvedValueOnce({ Err: { ExecutionUnknown: null } })
    }
    await expect(client.run()).rejects.toThrow()
    expect(await client.job()).toEqual({ ...original, phase: 'unknown' })
    await expect(client.prepare('namedowner')).rejects.toMatchObject({ code: 'Pending' })
    expect((await client.run()).phase).toBe('claimed')
    expect(registry.claim_legacy_handle.mock.calls[1]).toEqual(
      registry.claim_legacy_handle.mock.calls[0]
    )
  }
)

it('resolves a prepared claim on chain even if advisory queries become unavailable', async () => {
  const { client, registry } = fixture()
  vi.spyOn(client, 'ownership').mockResolvedValue(null)
  await client.prepare('namedowner')
  registry.get_legacy_reservation.mockRejectedValue(new Error('query unavailable'))
  registry.snapshot_progress.mockRejectedValue(new Error('query unavailable'))
  registry.claim_legacy_handle.mockResolvedValueOnce({ Err: { Forbidden: null } })
  await expect(client.run()).rejects.toMatchObject({ code: 'Forbidden' })
  expect((await client.job())?.phase).toBe('rejected')
  expect(registry.get_legacy_reservation).toHaveBeenCalledTimes(1)
  expect(registry.snapshot_progress).toHaveBeenCalledTimes(1)
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
