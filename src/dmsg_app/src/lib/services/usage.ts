import { controlResult, type AccountClient } from './account'
import { equal } from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import { ensure } from '../errors'

/** A month's formal-execution ledger copied into a synced commerce journal. */
export interface UsageRecord {
  format: 'dmsg-execution-usage/1'
  account: string
  month: number
  allowed: string
  charged: string
  monthRevision: string
  businessRevision: string
  leaseRevision: string
  weightPolicyVersion: string
}

/** UTC month as YYYYMM. */
export function monthUtc(at: number) {
  const d = new Date(at)
  return d.getUTCFullYear() * 100 + d.getUTCMonth() + 1
}

const before = (month: number) => (month % 100 === 1 ? month - 89 : month - 1)

/** The user home keeps an account's ledgers for this month and the last; the
 * first refresh of a month drops older ones. Before refreshing, copy the
 * finished months still there into the vault, which syncs them encrypted.
 * A journal gets a new revision only when its value changes. */
export async function archiveUsage(client: AccountClient, account: string, now = Date.now()) {
  const raw = xidBytes(account),
    last = before(monthUtc(now)),
    months = [last, before(last)]
  const results = await Promise.all(
    months.map((month) => client.user.get_execution_usage(raw, month))
  )
  for (const [i, result] of results.entries()) {
    if ('Err' in result && 'NotFound' in result.Err) continue
    const u = controlResult(result)
    ensure(
      equal(Uint8Array.from(u.account_id), raw) && u.month_utc === months[i],
      'INTEGRITY_FAILED'
    )
    const record: UsageRecord = {
      format: 'dmsg-execution-usage/1',
      account,
      month: u.month_utc,
      allowed: u.allowed_units.toString(),
      charged: u.charged_units.toString(),
      monthRevision: u.month_revision.toString(),
      businessRevision: u.business_revision.toString(),
      leaseRevision: u.lease_revision.toString(),
      weightPolicyVersion: u.weight_policy_version.toString()
    }
    await client.crypto.call(
      'commerceJournal',
      `usage:${account}:${u.month_utc}`,
      JSON.stringify(record)
    )
  }
}

/** Archived months of one account, newest first. */
export async function usageHistory(client: AccountClient, account: string) {
  return (await client.crypto.call('commerceJournals'))
    .filter((v) => v.key.startsWith(`usage:${account}:`))
    .map((v) => JSON.parse(v.value) as UsageRecord)
    .sort((a, b) => b.month - a.month)
}
