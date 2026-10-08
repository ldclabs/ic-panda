import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'
import { xidBytes } from '../src/lib/protocol/identity'
import { SigningClient } from '../src/lib/services/signing'
import { archiveUsage, usageHistory } from '../src/lib/services/usage'
import { boundEngine } from './support/engine'

beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
afterEach(() => vi.restoreAllMocks())

const now = Date.UTC(2026, 0, 3)
const row = (account: Uint8Array, month: number, allowed: bigint, charged: bigint) => ({
  Ok: {
    account_id: account,
    month_utc: month,
    allowed_units: allowed,
    charged_units: charged,
    month_revision: 1n,
    business_revision: 2n,
    lease_revision: 3n,
    weight_policy_version: 4n,
    valid_until_ms: BigInt(now + 3_600_000)
  }
})

async function fixture(ledgers: Record<number, [bigint, bigint]>) {
  const { engine, meta } = await boundEngine()
  const raw = xidBytes(meta.account.id),
    calls: string[] = []
  const user = {
    get_execution_usage: async (_: Uint8Array, month: number) => {
      calls.push(`get:${month}`)
      const ledger = ledgers[month]
      return ledger ? row(raw, month, ...ledger) : { Err: { NotFound: null } }
    },
    refresh_execution_entitlement: async () => {
      calls.push('refresh')
      return row(raw, 202601, 100n, 150n)
    }
  }
  const crypto = {
    call: (method: string, ...args: unknown[]) => (engine as any)[method](...args)
  }
  return { account: { meta, user, crypto } as any, id: meta.account.id, calls }
}

it('archives the finished months into the synced vault before a refresh drops them', async () => {
  vi.spyOn(Date, 'now').mockReturnValue(now)
  const { account, id, calls } = await fixture({ 202512: [100n, 7n], 202511: [100n, 120n] })
  const usage = await new SigningClient(account, async () => {}).usage(id)
  // January wraps to December and November; both are read before the refresh.
  expect(calls).toEqual(['get:202512', 'get:202511', 'refresh'])
  // A refund may leave the allowance below what was already charged.
  expect(usage).toMatchObject({ allowed: '100', charged: '150', remaining: '0' })
  expect(
    (await usageHistory(account, id)).map((u) => [u.month, u.charged, u.allowed])
  ).toEqual([
    [202512, '7', '100'],
    [202511, '120', '100']
  ])
  // Records leave the device only as encrypted commerce objects in the outbox.
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  const objects = (await db.db.getAll('objects')).filter((o) => o.kind === 'commerce')
  const outbox = await db.db.getAll('outbox')
  db.db.close()
  expect(objects).toHaveLength(2)
  expect(outbox.filter((j) => objects.some((o) => o.key === j.objectKey))).toHaveLength(2)
  expect(JSON.stringify([objects, outbox])).not.toContain('dmsg-execution-usage')
})

it('skips months the home no longer holds and rejects a ledger for another month', async () => {
  const { account, id, calls } = await fixture({ 202607: [10n, 1n] })
  await archiveUsage(account, id, Date.UTC(2026, 8, 30))
  expect(calls).toEqual(['get:202608', 'get:202607'])
  expect((await usageHistory(account, id)).map((u) => u.month)).toEqual([202607])
  account.user.get_execution_usage = async () => row(xidBytes(id), 202601, 10n, 1n)
  await expect(archiveUsage(account, id, Date.UTC(2026, 8, 30))).rejects.toMatchObject({
    code: 'INTEGRITY_FAILED'
  })
})
