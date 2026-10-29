/** Content-free notification contract. Business actions remain in their original views. */
import { validPrincipal } from './principal.ts'

export const noticeCategories = ['security', 'action', 'message', 'service'] as const
export type NoticeCategory = typeof noticeCategories[number]
export type NoticeTarget =
  | { kind: 'channel' | 'approval' | 'recovery' | 'agent_mail' | 'task' | 'plan' | 'service'; id: string }
  | { kind: 'inbox'; id: string | null }
  | { kind: 'summary'; id: NoticeCategory }
  | { kind: 'wallet'; id: { wallet: string; proposal: string } }

export interface NoticeItem {
  id: string
  account: string
  target: NoticeTarget
  version: string
  open: boolean
  actionable: boolean
  unread: number | null
  first_at: number
  updated_at: number
  deadline: number | null
  observed: string
  notified: string
  stage: number
  snoozed_until: number | null
  muted: boolean
  muted_until: number | null
}

export interface NoticePlan {
  system_id: string
  delivery_id: string
  account: string
  category: NoticeCategory
  target: NoticeTarget
  covered: { id: string; version: string; stage: number }[]
  count: number | null
  sound: boolean
  in_app: boolean
  update: boolean
  attempted_at: number
}

export interface NoticeBatch {
  plans: NoticePlan[]
  remove: string[]
  next_due: number | null
  history?: NoticePlan[]
}

export interface NoticeSettings {
  revision: number
  enabled: boolean
  categories: Record<NoticeCategory, { enabled: boolean; sound: boolean }>
  merge_devices: boolean
  agent_mail: Record<string, boolean>
  remote_revision: number
}

export interface GlobalNoticeSettings {
  revision: number
  enabled: boolean
  paused_until: number | null
  quiet_hours: { start: number; end: number; timezone: string } | null
  security_exception: boolean
  badge: 'all' | 'actions' | 'off'
}

export interface NoticePreferences {
  global: GlobalNoticeSettings
  account: NoticeSettings | null
  account_id: string | null
  remote_supported: boolean
}

export interface NoticeStatus {
  counts: { actionable: number; unread: number; unverified: boolean }
  accounts: { account: string; enabled: boolean; pending_sync: boolean }[]
  badge: number
  enabled: boolean
}

export interface NoticePermission {
  supported: boolean
  permission: 'system_managed' | 'unsupported'
  settings_available: boolean
  last_result?: 'submitted' | 'shown' | 'failed'
}

export const isNoticeHash = (value: unknown): value is string => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value)
export const isNoticeAccount = (value: unknown): value is string => typeof value === 'string' && /^[0-9a-v]{19}[0g]$/.test(value)
export const isNoticeSequence = (value: unknown): value is string => {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(value)) return false
  return BigInt(value) <= 18446744073709551615n
}

/** Reject arbitrary navigation, command names and content from a corrupted response. */
export function isNoticeTarget(value: unknown): value is NoticeTarget {
  if (!value || typeof value !== 'object') return false
  const { kind, id } = value as NoticeTarget
  switch (kind) {
    case 'channel': case 'approval': case 'recovery': case 'plan': return isNoticeHash(id)
    case 'inbox': return id === null || isNoticeHash(id)
    case 'agent_mail': return typeof id === 'string' && /^did:agent:[A-Za-z0-9:_-]{1,200}$/.test(id)
    case 'task': return typeof id === 'string' && /^(conflicts|sync_failed):[1-9][0-9]*$/.test(id)
    case 'service': return typeof id === 'string' && /^(network|registration):[1-9][0-9]*$/.test(id)
    case 'wallet': return !!id && typeof id === 'object' && validPrincipal(id.wallet) && isNoticeSequence(id.proposal)
    case 'summary': return noticeCategories.includes(id)
    default: return false
  }
}

export function isNoticePlan(value: unknown): value is NoticePlan {
  if (!value || typeof value !== 'object') return false
  const plan = value as NoticePlan
  return isNoticeHash(plan.system_id) && isNoticeHash(plan.delivery_id) && isNoticeAccount(plan.account)
    && noticeCategories.includes(plan.category) && isNoticeTarget(plan.target)
    && Array.isArray(plan.covered) && plan.covered.length > 0 && plan.covered.length <= 1000
    && plan.covered.every((item) => isNoticeHash(item.id) && isNoticeSequence(item.version) && Number.isInteger(item.stage) && item.stage >= 0 && item.stage <= 3)
    && typeof plan.sound === 'boolean' && typeof plan.in_app === 'boolean' && typeof plan.update === 'boolean'
}

/** This only chooses a view. The native owner checks the original account first. */
export function noticeRoute(target: NoticeTarget): string {
  switch (target.kind) {
    case 'channel': return `/channels?open=${target.id}`
    case 'inbox': return target.id ? `/inbox?open=${target.id}` : '/inbox'
    case 'approval': return `/approvals?open=${target.id}`
    case 'recovery': return '/settings/security'
    case 'agent_mail': return `/agents/mail/${encodeURIComponent(target.id)}`
    case 'wallet': return `/wallet/${target.id.wallet}/${target.id.proposal}`
    case 'plan': return '/plan'
    case 'task': case 'service': return '/settings/session'
    case 'summary': return `/reminders?scope=${target.id === 'message' ? 'recent' : 'open'}`
  }
}
