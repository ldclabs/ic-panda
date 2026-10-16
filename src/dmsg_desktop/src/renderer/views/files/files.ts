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

/** Folders first, then files, each by name in the interface language. */
export function ordered<T extends { name: string }>(folders: T[], files: T[], locale: string): T[] {
  const byName = (a: T, b: T) => a.name.localeCompare(b.name, locale, { numeric: true, sensitivity: 'base' })
  return [...[...folders].sort(byName), ...[...files].sort(byName)]
}

/** The text a search looks in, as the daemon's own search: name, tags and note. */
export function matchesFile(file: { name?: string; tags?: string[]; note?: string }, query: string): boolean {
  const q = query.trim().toLowerCase()
  if (!q) return true
  return [file.name, file.note, ...(file.tags ?? [])].some((field) => String(field ?? '').toLowerCase().includes(q))
}

/** The names of the folders down to `id`, from the folders the daemon lists with their paths. */
export function pathOf(folders: { id: string; name: string; path: string[] }[], id: string): string[] {
  if (id === ROOT) return []
  const folder = folders.find((f) => f.id === id)
  return folder ? [...folder.path, folder.name] : []
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
