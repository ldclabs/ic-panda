/**
 * The plan page's rules apart from its window: what an entitlement and an
 * order's state mean for people, what may be done with an order now, what
 * an order buys, a name's form and price, and how a name operation stands.
 * Pure, so they are tested under node.
 */
import type { Label } from '../../i18n/messages/en'

export const DAY = 86_400_000

/** The plan page's parts. */
export type Tab = 'membership' | 'names' | 'payments'

export const isTab = (value: unknown): value is Tab => value === 'membership' || value === 'names' || value === 'payments'

/** Where the entitlement's source stands, for people. */
export const sourceLabels: Record<string, Label> = {
  Free: 'sourceFree',
  Active: 'sourceActive',
  Expired: 'sourceExpired',
  Closing: 'sourceClosing',
  RepairRequired: 'sourceRepair',
  Suspended: 'sourceSuspended',
  Unverifiable: 'sourceUnverifiable'
}

/** Whether the account qualifies under the membership policy, for people. */
export const eligibilityLabels: Record<string, Label> = {
  Eligible: 'eligible',
  Ineligible: 'ineligible',
  Unverifiable: 'unverifiable'
}

/** How a term is paid, for people. */
export const methodLabels: Record<string, Label> = {
  Cash: 'methodCash',
  Panda: 'methodPanda',
  Compensation: 'methodCompensation'
}

/** What a plan card's button does. */
export const offerLabels: Record<string, Label> = {
  subscribe: 'subscribe',
  renew: 'renew',
  switch: 'switchPlan',
  upgrade: 'upgrade'
}

/** Why a plan card offers nothing now; all but a queued renewal say from when (`{time}`). */
export const waitLabels: Record<string, Label> = {
  RENEWAL_QUEUED: 'renewalQueued',
  RENEWAL_WINDOW: 'renewalWindow',
  UPGRADE_CASH_ONLY: 'upgradeCashOnly',
  SWITCH_AT_RENEWAL: 'switchAtRenewal'
}

export type Tone = 'neutral' | 'forest' | 'gem' | 'warn' | 'danger' | 'ok'

/** The service's order statuses, cash and PANDA, for people. */
const statusLabels: Record<string, [Label, Tone]> = {
  Reserving: ['orderReserving', 'warn'],
  AwaitingFunding: ['orderAwaitingFunding', 'warn'],
  Applying: ['orderApplying', 'warn'],
  Applied: ['orderApplied', 'ok'],
  RefundCommitted: ['orderRefundAvailable', 'warn'],
  Closing: ['sourceClosing', 'warn'],
  Rejected: ['orderRejected', 'danger'],
  Checking: ['orderChecking', 'warn'],
  Active: ['orderApplied', 'ok'],
  Terminated: ['orderEnded', 'neutral'],
  Cancelled: ['orderCancelled', 'neutral'],
  Released: ['orderReleased', 'neutral']
}

/** A journal's stage before the service reports the order. */
const stageLabels: Record<string, [Label, Tone]> = {
  review: ['orderReview', 'neutral'],
  authorized: ['orderAuthorized', 'warn'],
  unknown: ['orderUnknown', 'warn'],
  accepted: ['orderAccepted', 'warn']
}

/** An order's state for people: the service's status once read, else the journal's stage. */
export function orderState(job: any, order?: any): { label: Label; tone: Tone } {
  const [label, tone] = (order && statusLabels[order.status]) || stageLabels[job.stage] || stageLabels.unknown
  return { label, tone }
}

/** When an approved order must reach the service: funded, or applied with the neuron. */
export const deadlineOf = (job: any): number | undefined => (job.terms?.method === 'Cash' ? job.terms.funding_deadline_ms : job.terms?.application_deadline_ms)

/** Statuses the service moves on by itself. */
const MOVING = new Set(['Reserving', 'AwaitingFunding', 'Applying', 'Checking', 'Closing'])

/** Whether an order is on its way, so the page reads it again. */
export function moving(job: any, order: any, now: number): boolean {
  if (order) return MOVING.has(order.status)
  if (job.stage === 'authorized') return now < (deadlineOf(job) ?? 0)
  return job.stage === 'unknown' || job.stage === 'accepted'
}

/** An unapproved quote may be reused only within both fixed deadlines. */
export function reusableQuote(job: any, now = Date.now()): boolean {
  return job?.stage === 'review' && Number(job.accept_by_ms) > now && Number(deadlineOf(job)) > now
}

export type OrderAction = 'pay' | 'discard' | 'cancel' | 'refund' | 'advance' | 'reconcile'

/**
 * What may be done with an order now: pay or remove a quote; remove an
 * approval whose deadline passed before it reached the service; cancel
 * what is not paid or applied yet, a renewal that has not started, or a
 * PANDA application before it applies; apply a qualified PANDA application
 * immediately; settle what is being applied; refund the first
 * purchase within its window, the order `refundable` names.
 */
export function orderActions(job: any, order: any, now: number, refundable?: string | null): OrderAction[] {
  if (job.stage === 'review') return ['pay', 'discard']
  if (!order) {
    if (job.stage !== 'authorized') return ['reconcile']
    return now >= (deadlineOf(job) ?? 0) ? ['discard'] : []
  }
  switch (order.status) {
    case 'Reserving':
    case 'AwaitingFunding':
      return ['cancel']
    case 'Checking':
      // A qualified PANDA application applies immediately. If its
      // qualification is still unknown, reconciling asks the service to try again.
      if (job.terms?.method !== 'Panda') return ['cancel']
      return order.eligibility === 'Eligible' ? ['advance', 'cancel'] : ['reconcile', 'cancel']
    case 'Applying':
      return ['reconcile']
    case 'Applied':
      if (refundable && refundable === job.id) return ['refund']
      return job.starts_at_ms > now ? ['cancel'] : []
    default:
      return []
  }
}

/** What an order buys: a plan, an upgrade to one for the rest of the term, or a storage pack of the catalog. */
export function skuOf(sku: string, catalog?: any): { kind: 'plan' | 'upgrade' | 'storage'; plan?: string; bytes?: number } {
  const plan = (id: string) =>
    (catalog?.plans ?? []).find((p: any) => String(p.plan_id).toLowerCase() === id)?.plan_id ?? id.charAt(0).toUpperCase() + id.slice(1)
  if (sku.startsWith('upgrade-')) return { kind: 'upgrade', plan: plan(sku.slice('upgrade-'.length)) }
  if (/^[0-9a-f]{64}$/.test(sku)) return { kind: 'storage', bytes: (catalog?.storage_products ?? []).find((p: any) => p.product_id === sku)?.storage_bytes }
  return { kind: 'plan', plan: plan(sku) }
}

/** Whole days left until `ends`, rounded up; 0 once it passed. */
export const daysLeft = (ends: number, now: number) => Math.max(0, Math.ceil((ends - now) / DAY))

/** A name as the registry keeps it: without `@`, in lower case. */
export const canonicalName = (text: string) => text.trim().replace(/^@/, '').toLowerCase()

/** Why text cannot be a name, or null: 1–19 ASCII letters, digits and `_`, not first. */
export function nameProblem(text: string): Label | null {
  const name = text.trim().replace(/^@/, '')
  if (!name) return null
  if (name.length > 19) return 'nameTooLong'
  if (name.startsWith('_')) return 'nameUnderscore'
  if (!/^[A-Za-z0-9_]+$/.test(name)) return 'nameCharacters'
  return null
}

/** What registering a name costs by its length, in whole PANDA; the registry's quote is what counts. */
export const PRICE_TIERS: { lengths: string; panda: number }[] = [
  { lengths: '1', panda: 1_000_000 },
  { lengths: '2', panda: 200_000 },
  { lengths: '3–4', panda: 50_000 },
  { lengths: '5–6', panda: 20_000 },
  { lengths: '7–19', panda: 100 }
]

export function namePrice(length: number): number {
  if (length <= 1) return 1_000_000
  if (length === 2) return 200_000
  if (length <= 4) return 50_000
  if (length <= 6) return 20_000
  return 100
}

/** Whether the payer's balance on `ledger` covers `most`; null when the balance is unknown. */
export function covers(payer: any, ledger: string, most: string): boolean | null {
  const row = (payer?.balances ?? []).find((b: any) => b.ledger === ledger)
  if (row?.balance == null) return null
  return BigInt(row.balance) >= BigInt(most)
}

/** What a cash checkout's payer must hold: the amount, its fee reserve and the network fee. */
export function cashTotal(terms: any): bigint {
  return BigInt(terms.amount) + BigInt(terms.fee_reserve) + BigInt(terms.network_fee)
}

/** A non-dissolving neuron's delay starts now; a dissolving one's end is fixed. */
export function neuronUnlockAt(neuron: any, now = Date.now()): number {
  return neuron.dissolve_delay_seconds != null
    ? now + Number(neuron.dissolve_delay_seconds) * 1000
    : Number(neuron.unlock_at_ms)
}

/** Only confirmed transfers count as sent; unknown outcomes need reconciliation. */
export function refundOutcome(refunds: any[]): { label: Label; error?: string } {
  const failed = refunds.find((r) => r.error || r.status === 'Rejected')
  if (failed) return { label: 'error', error: failed.error ?? failed.status }
  return { label: refunds.length > 0 && refunds.every((r) => r.status === 'Succeeded') ? 'refundSent' : 'refundPending' }
}

/** The link that opens the accept dialog with a transfer's token on the target's device. */
export const acceptLink = (token: string) => `dmsg://handle/accept#${token}`

/** A transfer token pasted alone or inside its link. */
export const tokenOf = (text: string) => text.trim().replace(/^dmsg:\/\/handle\/accept#/, '')

/** A name operation's actions, for people. */
export const nameActionLabels: Record<string, Label> = {
  Register: 'register',
  Transfer: 'transferName',
  AcceptTransfer: 'acceptName',
  ClaimLegacy: 'claim'
}

/** Who calls the registry for a name operation, for people. */
export const callerLabels: Record<string, Label> = {
  holder: 'roleHolder',
  payer: 'payer',
  login: 'roleLogin'
}

export type NameState = 'prepared' | 'offered' | 'committable' | 'waiting' | 'expired' | 'unknown' | 'done' | 'rejected'

/**
 * How a name operation stands for this account: a transfer it offers waits
 * for the target, then for this account to commit while the target's
 * acceptance lasts; one it accepted waits for the owner until it lapses.
 */
export function nameState(op: any, now: number): NameState {
  if (op.phase === 'done' || op.phase === 'rejected' || op.phase === 'unknown') return op.phase
  if (op.phase === 'accepted') return op.expired || (op.until_ms != null && op.until_ms <= now) ? 'expired' : 'waiting'
  if (op.token) return op.accepted_until_ms != null && op.accepted_until_ms > now ? 'committable' : 'offered'
  return 'prepared'
}

/** How each name state reads, and its tone. */
export const nameStateLabels: Record<NameState, [Label, Tone]> = {
  prepared: ['namePrepared', 'neutral'],
  offered: ['nameOffered', 'warn'],
  committable: ['nameCommittable', 'forest'],
  waiting: ['nameWaiting', 'warn'],
  expired: ['nameExpired', 'neutral'],
  unknown: ['nameUnknown', 'warn'],
  done: ['nameDone', 'ok'],
  rejected: ['nameRejected', 'danger']
}
