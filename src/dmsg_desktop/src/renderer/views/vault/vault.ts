/**
 * The vault's rules apart from its window: item types and their fields,
 * which items a filter and a search show, the filter counts, and how a
 * conflict copy differs from the current version. Pure, so they are tested
 * under node.
 */
import type { Glyph } from '../../components/glyphs'
import type { Label } from '../../i18n/messages/en'

export type ItemType = 'note' | 'login' | 'api' | 'key' | 'file'
export type Field = 'username' | 'secret' | 'url'

/** The types in the order the filters and the new-item dialog offer them. */
export const types: ItemType[] = ['login', 'api', 'key', 'note', 'file']

export const typeLabel: Record<ItemType, Label> = { note: 'typeNote', login: 'typeLogin', api: 'typeApi', key: 'typeKey', file: 'typeFile' }
export const typeIcon: Record<ItemType, Glyph> = { note: 'note', login: 'key', api: 'api', key: 'keypair', file: 'file' }

/** Which fields each type has besides the title, tags and body. */
export const fields: Record<ItemType, Field[]> = {
  note: [],
  login: ['username', 'secret', 'url'],
  api: ['username', 'secret', 'url'],
  key: ['secret'],
  file: []
}

export const isType = (value: unknown): value is ItemType => types.includes(value as ItemType)

/** An item's type; one this version does not know reads as a note. */
export const typeOf = (item: { type?: unknown }): ItemType => (isType(item.type) ? item.type : 'note')

/** A filter of the list: everything, starred items, one type, one tag, or the trash. */
export type Scope = 'all' | 'starred' | 'trash' | `type:${ItemType}` | `tag:${string}`

/** The text a search looks in, as the daemon's own search: title, tags, body, user name and URL. */
export function matches(item: any, query: string): boolean {
  const q = query.trim().toLowerCase()
  if (!q) return true
  return [item.title, item.body, item.username, item.url, ...(item.tags ?? [])].some((field) => String(field ?? '').toLowerCase().includes(q))
}

/** Whether an entry belongs to a filter; the trash is its own list. */
export function inScope(entry: any, scope: Scope): boolean {
  if (scope === 'all' || scope === 'trash') return true
  if (scope === 'starred') return !!entry.item.favorite
  if (scope.startsWith('type:')) return entry.item.type === scope.slice(5)
  return (entry.item.tags ?? []).includes(scope.slice(4))
}

export interface Facets {
  all: number
  starred: number
  types: Partial<Record<ItemType, number>>
  /** Tags by how many items carry them, then by name. */
  tags: [string, number][]
}

/** How many items each filter holds. */
export function facets(entries: any[]): Facets {
  const result: Facets = { all: entries.length, starred: 0, types: {}, tags: [] }
  const tags = new Map<string, number>()
  for (const { item } of entries) {
    if (item.favorite) result.starred++
    const type = item.type
    if (isType(type)) result.types[type] = (result.types[type] ?? 0) + 1
    for (const tag of item.tags ?? []) tags.set(tag, (tags.get(tag) ?? 0) + 1)
  }
  result.tags = [...tags].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
  return result
}

/** The fields a conflict copy can differ in. The secret is masked on both sides, so it is never compared. */
export const compared = ['title', 'username', 'url', 'tags', 'favorite', 'body'] as const
export type Compared = (typeof compared)[number] | 'deleted'

/** The fields in which a conflict copy differs from the current version; `deleted` when only one is in the trash. */
export function differences(head: any, copy: any): Compared[] {
  const names: Compared[] = compared.filter((name) => JSON.stringify(head.item[name] ?? null) !== JSON.stringify(copy.item[name] ?? null))
  if (!!head.deleted !== !!copy.deleted) names.push('deleted')
  return names
}

export interface DiffLine {
  text: string
  /** Only on this side: removed on the current version's side, added on the copy's. */
  changed: boolean
}

/**
 * A line diff of two bodies for side-by-side reading: each side keeps its
 * own lines, and the lines the other side lacks are marked (longest common
 * subsequence).
 */
export function lineDiff(current: string, copy: string): { current: DiffLine[]; copy: DiffLine[] } {
  const a = current.split('\n')
  const b = copy.split('\n')
  const lcs: number[][] = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0))
  for (let i = a.length - 1; i >= 0; i--)
    for (let j = b.length - 1; j >= 0; j--) lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1])
  const keptA = new Set<number>()
  const keptB = new Set<number>()
  for (let i = 0, j = 0; i < a.length && j < b.length; ) {
    if (a[i] === b[j]) {
      keptA.add(i++)
      keptB.add(j++)
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) i++
    else j++
  }
  return {
    current: a.map((text, i) => ({ text, changed: !keptA.has(i) })),
    copy: b.map((text, j) => ({ text, changed: !keptB.has(j) }))
  }
}
