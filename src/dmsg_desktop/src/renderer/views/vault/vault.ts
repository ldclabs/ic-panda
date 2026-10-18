/**
 * The vault's rules apart from its window: item types and their fields,
 * how a field is named and shown, which items a filter and a search show,
 * the filter counts, how a conflict copy differs from the current version
 * and how a merge names its parts, sorting, and the password generator.
 * Pure, so they are tested under node. The fields a new item starts with
 * come from the daemon (`vault/templates`).
 */
import type { Glyph } from '../../components/glyphs'
import type { Label } from '../../i18n/messages/en'

export type ItemType = 'login' | 'api' | 'key' | 'ssh' | 'wallet' | 'card' | 'identity' | 'server' | 'database' | 'license' | 'note'
export type FieldType = 'text' | 'concealed' | 'url' | 'email' | 'phone' | 'date' | 'month' | 'otp'

export interface Field {
  id: string
  type: FieldType
  label: string
  section: string
  value: string
}

/** What `vault/templates` answers: each type's fields, and the field holding its main secret. */
export type Templates = Record<string, { fields: { id: string; type: FieldType }[]; secret: string | null }>

/** The types in the order the filters and the new-item dialog offer them. */
export const types: ItemType[] = ['login', 'api', 'key', 'ssh', 'wallet', 'card', 'identity', 'server', 'database', 'license', 'note']

export const typeLabel: Record<ItemType, Label> = {
  login: 'typeLogin',
  api: 'typeApi',
  key: 'typeKey',
  ssh: 'typeSsh',
  wallet: 'typeWallet',
  card: 'typeCard',
  identity: 'typeIdentity',
  server: 'typeServer',
  database: 'typeDatabase',
  license: 'typeLicense',
  note: 'typeNote'
}

export const typeIcon: Record<ItemType, Glyph> = {
  login: 'key',
  api: 'api',
  key: 'keypair',
  ssh: 'ssh',
  wallet: 'wallet',
  card: 'card',
  identity: 'identity',
  server: 'server',
  database: 'database',
  license: 'license',
  note: 'note'
}

/** The field types a person adds, in the order the menu offers them. */
export const fieldTypes: FieldType[] = ['text', 'concealed', 'url', 'email', 'phone', 'date', 'month', 'otp']

export const fieldTypeLabel: Record<FieldType, Label> = {
  text: 'fieldTypeText',
  concealed: 'fieldTypeConcealed',
  url: 'fieldTypeUrl',
  email: 'fieldTypeEmail',
  phone: 'fieldTypePhone',
  date: 'fieldTypeDate',
  month: 'fieldTypeMonth',
  otp: 'fieldTypeOtp'
}

/** The names of the templates' own fields, by ID; a field with a label of its own shows that. */
export const fieldLabel: Record<string, Label> = {
  username: 'username',
  password: 'fieldPassword',
  url: 'fieldWebsite',
  credential: 'fieldCredential',
  expires: 'fieldExpires',
  key: 'fieldKey',
  private_key: 'fieldPrivateKey',
  public_key: 'fieldPublicKey',
  fingerprint: 'fingerprint',
  phrase: 'fieldPhrase',
  address: 'fieldAddress',
  network: 'fieldNetwork',
  cardholder: 'fieldCardholder',
  number: 'fieldCardNumber',
  expiry: 'fieldExpiry',
  cvv: 'fieldCvv',
  pin: 'fieldPin',
  name: 'fieldName',
  email: 'fieldEmail',
  phone: 'fieldPhone',
  birthday: 'fieldBirthday',
  id_number: 'fieldIdNumber',
  engine: 'fieldEngine',
  host: 'fieldHost',
  port: 'fieldPort',
  database: 'fieldDatabase',
  license_key: 'fieldLicenseKey',
  licensed_to: 'fieldLicensedTo',
  purchased: 'fieldPurchased',
  otp: 'fieldTypeOtp',
  secret: 'secret'
}

/** Fields whose value usually spans lines. */
const MULTILINE = new Set(['key', 'private_key', 'public_key', 'phrase', 'address'])

/** Hidden fields whose generator makes passwords rather than nothing. */
const GENERATED = new Set(['password', 'credential', 'key', 'pin', 'license_key'])

/** What a hidden value reads as before it is revealed. */
export const MASK = '********'

export const isType = (value: unknown): value is ItemType => types.includes(value as ItemType)

/** An item's type; one this version does not know reads as a note. */
export const typeOf = (item: { type?: unknown }): ItemType => (isType(item.type) ? item.type : 'note')

/** Concealed and one-time-password fields: shown only on request. */
export const isHidden = (field: { type: string }) => field.type === 'concealed' || field.type === 'otp'

/** Whether a field's value spans lines: by its role, or by what it holds. */
export const isMultiline = (field: Field) => MULTILINE.has(field.id) || (!isHidden(field) && field.value.includes('\n'))

/** Whether a hidden field's generator fills it with a password or a PIN. */
export const generates = (field: Field) => field.type === 'concealed' && (GENERATED.has(field.id) || !fieldLabel[field.id])

/** A field's own name, or its template's. */
export function fieldName(field: Field, t: (label: Label) => string): string {
  if (field.label.trim()) return field.label
  const label = fieldLabel[field.id]
  return label ? t(label) : field.id
}

/** The fields under each section heading, in order; the unnamed section first. */
export function sections(fields: Field[]): { title: string; fields: Field[] }[] {
  const out: { title: string; fields: Field[] }[] = []
  for (const field of fields) {
    const title = field.section.trim()
    let section = out.find((s) => s.title === title)
    if (!section) {
      section = { title, fields: [] }
      if (title) out.push(section)
      else out.unshift(section)
    }
    section.fields.push(field)
  }
  return out
}

/** A field's value: empty for none. */
export const valueOf = (item: any, id: string): string => (item.fields ?? []).find((f: Field) => f.id === id)?.value ?? ''

/** dMsg's own keys: shown read-only, never edited, copied or trashed. */
export const isManaged = (item: any) => (item.tags ?? []).some((tag: string) => tag.startsWith('dmsg-'))

/** The one-time-password field an item shows a code for. */
export const otpField = (item: any): Field | undefined => (item.fields ?? []).find((f: Field) => f.type === 'otp' && f.value)

/** A line on what the item is: the user name, the address, another shown value, the notes' first line, or its type. */
export function line(item: any, t: (label: Label) => string): string {
  const shown = (item.fields ?? []).filter((f: Field) => !isHidden(f) && f.value.trim())
  const pick = valueOf(item, 'username') || valueOf(item, 'url').replace(/^https?:\/\//, '') || shown[0]?.value.split('\n')[0]
  return pick || (item.notes ?? '').split('\n')[0] || t(typeLabel[typeOf(item)])
}

/** The text a search looks in, as the daemon's own search: the title, tags, notes, field labels and shown values. */
export function matches(item: any, query: string): boolean {
  const q = query.trim().toLowerCase()
  if (!q) return true
  const fields: Field[] = item.fields ?? []
  return [item.title, item.notes, ...(item.tags ?? []), ...fields.map((f) => f.label), ...fields.filter((f) => !isHidden(f)).map((f) => f.value)].some((text) =>
    String(text ?? '')
      .toLowerCase()
      .includes(q)
  )
}

/** A filter of the list: everything, starred items, one type, one tag, items the security check flags, or the trash. */
export type Scope = 'all' | 'starred' | 'health' | 'trash' | `type:${ItemType}` | `tag:${string}`

/** Whether an entry belongs to a filter; the trash is its own list. `flagged` holds the IDs the security check names. */
export function inScope(entry: any, scope: Scope, flagged: Set<string> = new Set()): boolean {
  if (scope === 'all' || scope === 'trash') return true
  if (scope === 'starred') return !!entry.item.favorite
  if (scope === 'health') return flagged.has(entry.id)
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

export type Sort = 'updated' | 'created' | 'title'
export const sorts: { value: Sort; label: Label }[] = [
  { value: 'updated', label: 'sortUpdated' },
  { value: 'created', label: 'sortCreated' },
  { value: 'title', label: 'sortTitle' }
]

/** The list in an order: newest edit or creation first, or by title. The trash goes by when items were deleted. */
export function sorted(entries: any[], sort: Sort, trash = false): any[] {
  const list = [...entries]
  if (trash) return list.sort((a, b) => (b.at ?? 0) - (a.at ?? 0))
  if (sort === 'title') return list.sort((a, b) => String(a.item.title).localeCompare(String(b.item.title)))
  const key = sort === 'created' ? 'createdAt' : 'updatedAt'
  return list.sort((a, b) => (b.item[key] ?? 0) - (a.item[key] ?? 0))
}

/** The parts of an item a conflict copy can differ in, and a merge picks: whole parts, or one field by ID. */
export type Part = 'title' | 'tags' | 'notes' | 'favorite' | 'files' | `field:${string}` | 'deleted'

/**
 * The parts in which a conflict copy differs from the current version.
 * Hidden values are masked on both sides, so `hidden` (from the daemon)
 * names the hidden fields whose values differ; `deleted` when only one side
 * is in the trash.
 */
export function differences(head: any, copy: any, hidden: string[] = []): Part[] {
  const parts: Part[] = (['title', 'tags', 'notes', 'favorite', 'files'] as const).filter(
    (name) => JSON.stringify(head.item[name] ?? null) !== JSON.stringify(copy.item[name] ?? null)
  )
  const ids: string[] = []
  for (const field of [...(head.item.fields ?? []), ...(copy.item.fields ?? [])]) if (!ids.includes(field.id)) ids.push(field.id)
  for (const id of ids) {
    const a = (head.item.fields ?? []).find((f: Field) => f.id === id)
    const b = (copy.item.fields ?? []).find((f: Field) => f.id === id)
    const shape = (f?: Field) => (f ? JSON.stringify([f.type, f.label, f.section, isHidden(f) ? '' : f.value]) : null)
    if (shape(a) !== shape(b) || hidden.includes(id)) parts.push(`field:${id}`)
  }
  if (!!head.deleted !== !!copy.deleted) parts.push('deleted')
  return parts
}

export interface DiffLine {
  text: string
  /** Only on this side: removed on the current version's side, added on the copy's. */
  changed: boolean
}

/**
 * A line diff of two texts for side-by-side reading: each side keeps its
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

/** The fields of an edit form: each field with its value, or for a hidden one already saved, whether it is kept, changed or cleared. */
export interface Draft extends Field {
  /** A hidden value already saved: `keep` sends no value, `clear` sends an empty one. */
  hiddenState?: 'keep' | 'change' | 'clear'
}

/**
 * The form's fields as `vault/edit` or `vault/create` field changes; a kept
 * hidden value, or one opened for a change and left empty, is left out, so
 * the daemon keeps it. Only Clear empties it.
 */
export function fieldChanges(drafts: Draft[]): Record<string, unknown>[] {
  return drafts.map((d) => {
    const change: Record<string, unknown> = { id: d.id || undefined, type: d.type, label: d.label, section: d.section }
    if (d.hiddenState === 'keep' || (d.hiddenState === 'change' && !d.value)) return change
    change.value = d.hiddenState === 'clear' ? '' : d.value
    return change
  })
}

/** An item's fields as a form edits them: hidden values already saved are kept unless changed. */
export function drafts(fields: Field[]): Draft[] {
  return fields.map((f) => (isHidden(f) && f.value ? { ...f, value: '', hiddenState: 'keep' } : { ...f }))
}

export type GeneratorMode = 'random' | 'words' | 'pin'

export interface GeneratorOptions {
  mode: GeneratorMode
  /** Characters of a random password or digits of a PIN. */
  length: number
  upper: boolean
  lower: boolean
  digits: boolean
  symbols: boolean
  /** Leave out characters that look alike: 0 O o 1 l I |. */
  unambiguous: boolean
  /** Words of a passphrase. */
  words: number
  separator: string
}

export const defaultOptions: GeneratorOptions = {
  mode: 'random',
  length: 20,
  upper: true,
  lower: true,
  digits: true,
  symbols: true,
  unambiguous: true,
  words: 5,
  separator: '-'
}

const SETS = {
  upper: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  lower: 'abcdefghijklmnopqrstuvwxyz',
  digits: '0123456789',
  symbols: '!@#$%^&*()-_=+[]{};:,.?/~'
}
const AMBIGUOUS = /[0Oo1lI|]/g

/** Fills an array with random values, as `crypto.getRandomValues` does. */
export type Random = (values: Uint32Array<ArrayBuffer>) => Uint32Array<ArrayBuffer>

/** A uniform random integer below `n`, by rejection sampling over 32-bit values. */
export function below(n: number, random: Random): number {
  const limit = Math.floor(0x1_0000_0000 / n) * n
  const one = new Uint32Array(1)
  for (;;) {
    const value = random(one)[0]
    if (value < limit) return value % n
  }
}

/** The character sets a random password draws from, with the options applied. */
export function charsets(options: GeneratorOptions): string[] {
  const picked = (['upper', 'lower', 'digits', 'symbols'] as const).filter((name) => options[name]).map((name) => SETS[name])
  const sets = (picked.length ? picked : [SETS.lower]).map((set) => (options.unambiguous ? set.replace(AMBIGUOUS, '') : set))
  return sets.filter((set) => set.length)
}

/**
 * A new secret: a random password with a character of every chosen kind, a
 * passphrase of `words` from the list, or a PIN. `random` fills an array
 * with random values (`crypto.getRandomValues`).
 */
export function generate(options: GeneratorOptions, random: Random, list: readonly string[] = []): string {
  if (options.mode === 'pin') {
    const n = Math.max(4, Math.min(12, options.length))
    return Array.from({ length: n }, () => String(below(10, random))).join('')
  }
  if (options.mode === 'words') {
    const n = Math.max(3, Math.min(12, options.words))
    return Array.from({ length: n }, () => list[below(list.length, random)]).join(options.separator)
  }
  const sets = charsets(options)
  const all = sets.join('')
  const n = Math.max(sets.length, Math.max(8, Math.min(64, options.length)))
  for (;;) {
    const chars = Array.from({ length: n }, () => all[below(all.length, random)])
    if (sets.every((set) => chars.some((c) => set.includes(c)))) return chars.join('')
  }
}

/** About how many bits a generated secret holds with these options. */
export function entropy(options: GeneratorOptions, listSize = 7776): number {
  if (options.mode === 'pin') return Math.max(4, Math.min(12, options.length)) * Math.log2(10)
  if (options.mode === 'words') return Math.max(3, Math.min(12, options.words)) * Math.log2(listSize)
  const pool = charsets(options).join('').length
  return Math.max(8, Math.min(64, options.length)) * Math.log2(pool)
}

/** A one-time code in two halves for reading: `123 456`. */
export const spaced = (code: string) => (code.length >= 6 ? `${code.slice(0, Math.ceil(code.length / 2))} ${code.slice(Math.ceil(code.length / 2))}` : code)

/** Findings of `vault/health`, by item, then by field. */
export function findingsByItem(findings: { id: string; field: string; issue: string }[]): Map<string, Map<string, string[]>> {
  const out = new Map<string, Map<string, string[]>>()
  for (const f of findings) {
    const fields = out.get(f.id) ?? new Map<string, string[]>()
    fields.set(f.field, [...(fields.get(f.field) ?? []), f.issue])
    out.set(f.id, fields)
  }
  return out
}

export const issueLabel: Record<string, Label> = {
  weak: 'issueWeak',
  reused: 'issueReused',
  insecure: 'issueInsecure',
  expired: 'issueExpired',
  expiring: 'issueExpiring'
}
