import type { Label } from '../i18n/messages/en'

export const deviceCapabilities = new Map<string, Label>([
  ['ContentSign', 'deviceContentSigning'], ['VaultUnlock', 'deviceUnlockWorkspace'],
  ['RootManage', 'deviceManageSecurity'], ['FormalApprove', 'formal'],
  ['PaymentOffer', 'deviceAuthorizeLetterOffers'], ['WalletApprove', 'deviceWalletProposals']
])

export function deviceCapabilityName(value: string, t: (key: Label) => string): string {
  const label = deviceCapabilities.get(value)
  return label ? t(label) : value
}

export function deviceRoleName(value: string, t: (key: Label) => string): string {
  return value === 'Administrator' ? t('roleAdmin') : value === 'Member' ? t('roleMember') : value
}

export interface SigningPolicy {
  frozen: boolean
  daily_executions: number
  allowed_purposes: string[]
}

/** The user home accepts at most 64 formal signatures a day; zero stops execution. */
export function dailyLimit(value: string): number | null {
  const number = Number(value)
  return /^\d+$/.test(value) && Number.isInteger(number) && number >= 0 && number <= 64 ? number : null
}

/** A selection is a set, regardless of the order in which boxes were checked. */
export function sameSelection(left: string[], right: string[]): boolean {
  const values = new Set(left)
  return values.size === new Set(right).size && right.every((value) => values.has(value))
}

/** Send only edited fields, together in one approval; never overwrite the freeze state. */
export function policyChanges(policy: SigningPolicy, daily: string, purposes: string[]) {
  const value = dailyLimit(daily)
  if (value === null) return null
  const changes: { daily_executions?: number; allowed_purposes?: string[] } = {}
  if (value !== policy.daily_executions) changes.daily_executions = value
  if (!sameSelection(purposes, policy.allowed_purposes)) changes.allowed_purposes = [...purposes]
  return Object.keys(changes).length ? changes : null
}
