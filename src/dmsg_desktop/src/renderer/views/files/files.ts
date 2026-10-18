/**
 * The file library's rules apart from its window: a version's status, where
 * a version came from, an icon for a type, and how a list is ordered and
 * searched. Pure, so they are tested under node.
 */
import type { Glyph } from '../../components/glyphs'
import type { Label } from '../../i18n/messages/en'

/** The root folder's ID, as the daemon names it. */
export const ROOT = 'root'

/** What the library knows of a version's copies, as the daemon reports it. */
export type Status = 'local' | 'backing_up' | 'backed_up' | 'cloud_only' | 'too_large' | 'failed'

export const statuses: Status[] = ['local', 'backing_up', 'backed_up', 'cloud_only', 'too_large', 'failed']

export const isStatus = (value: unknown): value is Status => statuses.includes(value as Status)

/** A status the window does not know reads as local only. */
export const statusOf = (value: unknown): Status => (isStatus(value) ? value : 'local')

export const statusLabel: Record<Status, Label> = {
  local: 'statusLocal',
  backing_up: 'statusBackingUp',
  backed_up: 'statusBackedUp',
  cloud_only: 'statusCloudOnly',
  too_large: 'statusTooLarge',
  failed: 'statusFailed'
}

export const statusIcon: Record<Status, Glyph> = {
  local: 'drive',
  backing_up: 'cloud-up',
  backed_up: 'sync',
  cloud_only: 'cloud-down',
  too_large: 'cloud-off',
  failed: 'warning'
}

export const statusTone: Record<Status, 'neutral' | 'forest' | 'gem' | 'warn' | 'danger' | 'ok'> = {
  local: 'neutral',
  backing_up: 'gem',
  backed_up: 'ok',
  cloud_only: 'forest',
  too_large: 'warn',
  failed: 'danger'
}

/** Where a version came from, as the daemon's `origin.type` names it. */
export const originLabel: Record<string, Label> = {
  imported: 'originImported',
  edited: 'originEdited',
  fetched: 'originFetched',
  mounted: 'originMounted',
  shared_in: 'originShared'
}

/** An icon for a media type: pictures, text, anything else. */
export function fileIcon(mime: string | undefined): Glyph {
  if (!mime) return 'file'
  if (mime.startsWith('image/')) return 'image'
  if (mime.startsWith('text/') || ['application/json', 'application/xml', 'application/toml', 'application/yaml'].includes(mime)) return 'file-text'
  return 'file'
}

/** Whether a preview is a picture or text the window can show. */
export function previewable(mime: string | undefined): boolean {
  return fileIcon(mime) !== 'file'
}

/** What a folder's list is ordered by, and which way. */
export type SortKey = 'name' | 'modified' | 'size' | 'type'
export interface Sort {
  key: SortKey
  descending: boolean
}

export const sortKeys: SortKey[] = ['name', 'modified', 'size', 'type']

export const isSort = (value: any): value is Sort => sortKeys.includes(value?.key) && typeof value?.descending === 'boolean'

/**
 * Folders first, then files, each by name in the interface language, or by
 * `sort`. Folders have no size or type: by those they stay by name, A to Z.
 * Ties go by name.
 */
export function ordered<T extends { name: string; updated_at?: number; size?: number; mime?: string }>(
  folders: T[],
  files: T[],
  locale: string,
  sort: Sort = { key: 'name', descending: false }
): T[] {
  const byName = (a: T, b: T) => a.name.localeCompare(b.name, locale, { numeric: true, sensitivity: 'base' })
  const by = (key: SortKey, descending: boolean) => (a: T, b: T) => {
    const order =
      key === 'modified'
        ? (a.updated_at ?? 0) - (b.updated_at ?? 0)
        : key === 'size'
          ? (a.size ?? 0) - (b.size ?? 0)
          : key === 'type'
            ? (a.mime ?? '').localeCompare(b.mime ?? '')
            : byName(a, b)
    return (descending ? -order : order) || byName(a, b)
  }
  const named = sort.key === 'size' || sort.key === 'type'
  return [...[...folders].sort(by(named ? 'name' : sort.key, named ? false : sort.descending)), ...[...files].sort(by(sort.key, sort.descending))]
}

/** The text a search looks in, as the daemon's own search: name, tags and note. */
export function matchesFile(file: { name?: string; tags?: string[]; note?: string }, query: string): boolean {
  const q = query.trim().toLowerCase()
  if (!q) return true
  return [file.name, file.note, ...(file.tags ?? [])].some((field) => String(field ?? '').toLowerCase().includes(q))
}

/** What a dragged row carries: the IDs of the rows dragged, as JSON. */
export const DRAG_TYPE = 'application/x-dmsg-files'

/** Whether a drag carries library rows or files from the system. */
export const carries = (event: DragEvent) => !!event.dataTransfer?.types.some((type) => type === DRAG_TYPE || type === 'Files')

/** The library rows a drag carries; none for anything else. */
export function draggedIds(event: DragEvent): string[] {
  try {
    const ids = JSON.parse(event.dataTransfer?.getData(DRAG_TYPE) || '[]')
    return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : []
  } catch {
    return []
  }
}

/** The folders from the root down to `id`, itself included, by walking up the parents. */
export function ancestorsOf(folders: { id: string; parent: string; name: string }[], id: string): { id: string; name: string }[] {
  const chain: { id: string; name: string }[] = []
  let cursor = folders.find((f) => f.id === id)
  while (cursor && chain.length <= folders.length) {
    chain.unshift({ id: cursor.id, name: cursor.name })
    const parent: string = cursor.parent
    cursor = folders.find((f) => f.id === parent)
  }
  return chain
}

/** A folder tree from the flat list: children by parent, each level by name. */
export function childrenOf(folders: { id: string; parent: string; name: string }[], parent: string, locale: string) {
  return folders.filter((f) => f.parent === parent).sort((a, b) => a.name.localeCompare(b.name, locale, { numeric: true, sensitivity: 'base' }))
}

/** Whether `folder` is `ancestor` or lies under it. */
export function within(folders: { id: string; parent: string }[], folder: string, ancestor: string): boolean {
  let cursor: string | undefined = folder
  for (let steps = 0; cursor && steps <= folders.length; steps++) {
    if (cursor === ancestor) return true
    cursor = folders.find((f) => f.id === cursor)?.parent
  }
  return cursor === ancestor
}

/** The MIME type a dropped-in text preview was decoded from, kept short for a badge. */
export function shortMime(mime: string | undefined): string {
  if (!mime) return ''
  const [, subtype = mime] = mime.split('/')
  return subtype.replace(/^x-/, '').replace(/^vnd\./, '').toUpperCase().slice(0, 12)
}
