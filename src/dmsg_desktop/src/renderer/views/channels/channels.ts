/**
 * The channel views' rules apart from the window: who may do what in a
 * ledger, members in order, a timeline split into days, and the tone of an
 * avatar. Pure, so they are tested under node.
 */
import type { Label } from '../../i18n/messages/en'

export const roles = ['admin', 'publisher', 'member'] as const

export const roleLabel: Record<string, Label> = { owner: 'roleOwner', admin: 'roleAdmin', publisher: 'rolePublisher', member: 'roleMember' }
export const kindLabel: Record<string, Label> = { collaboration: 'collaboration', distribution: 'distribution', direct: 'direct' }
export const statusLabel: Record<string, Label> = { active: 'channelActive', rotation_required: 'channelRotation', archived: 'channelArchived' }

/** What this account may do in a channel, from its ledger. */
export function powers(ledger: any, me: string | undefined) {
  const role: string | undefined = me ? ledger?.members?.[me]?.role : undefined
  const owner = !!me && ledger?.owner === me
  return {
    member: !!role,
    owner,
    manages: owner || role === 'admin',
    writes: !!role && ledger?.status === 'active'
  }
}

const rank: Record<string, number> = { owner: 0, admin: 1, publisher: 2, member: 3 }

/** Members with the owner first, then by role, then by account. */
export function ordered(ledger: any): any[] {
  return Object.values(ledger?.members ?? {}).sort(
    (a: any, b: any) => (rank[a.role] ?? 9) - (rank[b.role] ?? 9) || String(a.account).localeCompare(String(b.account))
  )
}

/** Messages in days: each day's key (local date) and its messages, oldest first. */
export function days<T extends { stored_at: number }>(messages: T[]): { day: string; at: number; messages: T[] }[] {
  const groups: { day: string; at: number; messages: T[] }[] = []
  for (const message of [...messages].sort((a, b) => a.stored_at - b.stored_at)) {
    const date = new Date(message.stored_at)
    const day = `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`
    const last = groups.at(-1)
    if (last?.day === day) last.messages.push(message)
    else groups.push({ day, at: message.stored_at, messages: [message] })
  }
  return groups
}

/** A steady tone for an account or channel's avatar, from its ID. */
export function tone(id: string, tones = 4): number {
  let hash = 0
  for (const char of id) hash = (hash * 31 + char.charCodeAt(0)) >>> 0
  return hash % tones
}

/** The letter an avatar shows: a name's first, or an ID's first two characters. */
export function initials(name: string): string {
  const text = name.trim()
  if (!text) return '?'
  return /^[0-9a-z-]{10,}$/.test(text) ? text.slice(0, 2) : Array.from(text)[0].toUpperCase()
}
