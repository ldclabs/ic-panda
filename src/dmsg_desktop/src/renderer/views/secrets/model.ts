export interface SourceItem {
  id: string
  revision: string
  title: string
  type: string
  blocked: boolean
  fields: { id: string; label: string; type: string }[]
  files: string[]
}
/** Keep only metadata in the UI; hidden values never enter search or drafts. */
export function sourceItems(entries: any[]): SourceItem[] {
  return entries.map((e) => ({
    id: e.id,
    revision: e.revision,
    title: e.item.title,
    type: e.item.type,
    blocked:
      !!e.deleted ||
      !!e.conflict ||
      (e.item.tags ?? []).some((tag: string) => tag.startsWith('dmsg-')),
    fields: e.item.fields.map((f: any) => ({
      id: f.id,
      label: f.label,
      type: f.type
    })),
    files: e.item.files ?? []
  }))
}
export const toggle = <T>(values: T[], value: T, checked: boolean) =>
  checked ? [...new Set([...values, value])] : values.filter((v) => v !== value)
export const quoteShell = (value: string) =>
  `'${value.replaceAll("'", "'\\''")}'`
export const validPath = (path: string) =>
  !!path &&
  !path.startsWith('/') &&
  path.split('/').every((p) => p && p !== '.' && p !== '..') &&
  !/[\0\r\n\\]/.test(path)
export function rotationAllowed(
  r: any,
  step: string,
  revision: string | undefined,
  index: any
): boolean {
  if (r.completed_at || r.source_error || !revision) return false
  const current = r.new_revision === revision
  if (step === 'created')
    return (
      r.can_report_created === true &&
      revision !== r.previous_revision &&
      (!r.created || !current)
    )
  if (step === 'tested') return !!r.created && current && !r.tested
  if (step === 'references_checked')
    return (
      !!r.tested &&
      current &&
      index?.revision === revision &&
      !(r.references_checked && r.references_digest === index.digest)
    )
  return (
    !!r.references_checked &&
    current &&
    index?.revision === revision &&
    r.references_digest === index.digest &&
    !r.upstream_revoked
  )
}
/** Snapshot equality is checked again after async preparation, even across A→B→A edits. */
export class DraftVersion {
  private version = 0
  change() {
    this.version++
  }
  capture() {
    return this.version
  }
  current(version: number) {
    return version === this.version
  }
}
export const card =
  'min-w-0 space-y-3 rounded-xl border border-line bg-surface p-4'
