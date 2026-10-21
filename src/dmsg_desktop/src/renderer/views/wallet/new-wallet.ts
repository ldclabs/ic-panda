/**
 * The new-wallet form's rules apart from its window: what a member draft
 * means, which IDs the hub accepts, how many approvals the policy may take,
 * and what the form asks the hub to create. Pure, so they are tested under
 * node.
 */
import { validPrincipal } from './model.ts'
import type { Label } from '../../i18n/messages/en'

/** One member line being edited. `key` keeps its input with its row. */
export type MemberDraft = {
  key: number
  id: string
  role: string
  label: string
  /** Whether the line left the keyboard, so a format complaint may show. */
  touched: boolean
}

/** The creator plus these make the hub's limit of 64 members. */
export const MAX_DRAFT_MEMBERS = 63

/** Roles a wallet member may take, as the daemon reads them. */
export const MEMBER_ROLES = ['admin', 'approver', 'proposer', 'viewer', 'recovery'] as const

const ACCOUNT = /^[0-9a-v]{19}[0g]$/

/** Whether text is a dMsg account ID (a 20-character Xid) or a Principal’s text form. */
export const isMemberId = (id: string) => ACCOUNT.test(id) || (validPrincipal(id) && !['2vxsx-fae', 'aaaaa-aa'].includes(id))

/** The rows that name someone; a blank row is an invitation to type, not a member. */
export const namedMembers = (rows: MemberDraft[]) => rows.filter((row) => row.id.trim())

/** What is wrong with a row, for people, or null: the creator, a repeat, or not an ID. */
export function memberProblem(row: MemberDraft, rows: MemberDraft[], creator: string): Label | null {
  const id = row.id.trim()
  if (!id) return null
  if (creator && id === creator) return 'twMemberSelf'
  if (rows.some((other) => other.key !== row.key && other.id.trim() === id)) return 'twMemberDuplicate'
  if (!isMemberId(id)) return 'twMemberInvalid'
  return null
}

/**
 * Admins and approvers a policy may count, the creator included: only their
 * roles approve; a Proposer, Viewer or Recovery never does.
 */
export function thresholdLimits(rows: MemberDraft[]): { admins: number; approvers: number } {
  let admins = 1
  let approvers = 1
  for (const row of namedMembers(rows)) {
    if (row.role === 'admin') {
      admins += 1
      approvers += 1
    } else if (row.role === 'approver') {
      approvers += 1
    }
  }
  return { admins, approvers }
}

/** The members the hub is asked to add; the creator joins as an Admin by itself. */
export function memberInputs(rows: MemberDraft[]) {
  return namedMembers(rows).map((row) => ({ id: row.id.trim(), role: row.role, label: row.label.trim() }))
}

/** The activation price and its ledger fee, added in e8s. */
export const activationTotal = (price: string, fee: string) => (BigInt(price) + BigInt(fee)).toString()
