/**
 * The channel views' rules apart from the window: who may do what in a
 * ledger, members in order, the channel list in order, a timeline split
 * into days, receipts, invitation links, transfer packets in a timeline,
 * and the tone of an avatar. Pure, so they are tested under node.
 */
import type { Label } from '../../i18n/messages/en'

export const roles = ['admin', 'publisher', 'member'] as const

export const roleLabel: Record<string, Label> = { owner: 'roleOwner', admin: 'roleAdmin', publisher: 'rolePublisher', member: 'roleMember' }
export const kindLabel: Record<string, Label> = { collaboration: 'collaboration', distribution: 'distribution', direct: 'direct' }
export const statusLabel: Record<string, Label> = { active: 'channelActive', rotation_required: 'channelRotation', archived: 'channelArchived' }

/** Whether an account coordinates rotations: the owner, an admin or an allowed member holding the current epoch. */
export function mayRotate(ledger: any, account: string | undefined): boolean {
  const member = account ? ledger?.members?.[account] : undefined
  return !!member && (member.role === 'owner' || member.role === 'admin' || member.can_rotate) && (ledger.epoch === 0 || member.joined_epoch <= ledger.epoch)
}

/** What this account may do in a channel, from its ledger. */
export function powers(ledger: any, me: string | undefined) {
  const role: string | undefined = me ? ledger?.members?.[me]?.role : undefined
  const owner = !!me && ledger?.owner === me
  return {
    member: !!role,
    owner,
    manages: owner || role === 'admin',
    rotates: mayRotate(ledger, me),
    // A distribution channel's members read; its owner, admins and publishers write.
    writes: !!role && ledger?.status === 'active' && (ledger?.type !== 'distribution' || role !== 'member')
  }
}

/** Largest message text in bytes, as the daemon counts them. */
export const MAX_TEXT = 16_000

/** A text's length in UTF-8 bytes. */
export const bytes = (text: string): number => new TextEncoder().encode(text).length

/**
 * The channel list: the channels this account takes part in, latest
 * activity first, then the others (archived, left or hidden) by name.
 */
export function arrange(channels: any[], me: string | undefined): { live: any[]; other: any[] } {
  const live: any[] = []
  const other: any[] = []
  for (const channel of channels) {
    const member = !!me && !!channel.ledger?.members?.[me]
    if (member && channel.ledger?.status !== 'archived' && !channel.hidden) live.push(channel)
    else other.push(channel)
  }
  const at = (c: any) => c.last?.stored_at ?? 0
  live.sort((a, b) => at(b) - at(a) || String(a.name).localeCompare(String(b.name)))
  other.sort((a, b) => String(a.name).localeCompare(String(b.name)))
  return { live, other }
}

/** How far the other members took and read a message of this account (C04). */
export function receiptOf(seq: number, receipts: any, ledger: any, me: string | undefined): { delivered: number; read: number; others: number } {
  const others = Object.keys(ledger?.members ?? {}).filter((a) => a !== me)
  const cursors = receipts?.cursors ?? {}
  return {
    delivered: others.filter((a) => (cursors[a]?.delivered ?? 0) >= seq).length,
    read: others.filter((a) => (cursors[a]?.read ?? 0) >= seq).length,
    others: others.length
  }
}

const LINK = 'dmsg://channel/join#'

/** The link that opens an invitation in the invited account's desktop. */
export function invitationLink(invitation: unknown): string {
  const text = JSON.stringify(invitation)
  const base = btoa(String.fromCharCode(...new TextEncoder().encode(text)))
  return LINK + base.replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '')
}

/** An invitation pasted as its JSON or its link; null when it is neither. */
export function readInvitation(text: string): any | null {
  const value = text.trim()
  try {
    if (value.startsWith('{')) return JSON.parse(value)
    const encoded = value.startsWith(LINK) ? value.slice(LINK.length) : value
    const binary = atob(encoded.replaceAll('-', '+').replaceAll('_', '/'))
    const parsed = JSON.parse(new TextDecoder().decode(Uint8Array.from(binary, (c) => c.charCodeAt(0))))
    return parsed?.format === 'dmsg-channel-invitation/1' ? parsed : null
  } catch {
    return null
  }
}

/**
 * What this account does with a transfer packet handed over in a channel:
 * accept a proposal to it, commit an acceptance of its own proposal, or
 * nothing; done once the channel shows the transfer.
 */
export function packetStep(packet: any, ledger: any, billing: any, me: string | undefined): { kind: 'owner' | 'sponsor'; target: string; step: 'accept' | 'commit' | 'wait' | 'done' } | null {
  const owner = packet?.format === 'dmsg-channel-owner-transfer/1'
  if (!owner && packet?.format !== 'dmsg-channel-sponsor/1') return null
  const target: string = owner ? packet.payload?.action?.account : packet.payload?.to_payer
  const done = owner ? ledger?.owner === target : billing?.billing_account === target
  const step = done ? 'done' : !packet.acceptance ? (target === me ? 'accept' : 'wait') : packet.context?.account_id === me ? 'commit' : 'wait'
  return { kind: owner ? 'owner' : 'sponsor', target, step }
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
