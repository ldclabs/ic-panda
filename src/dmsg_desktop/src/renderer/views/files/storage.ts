export function phaseOf(entry: any): string { return entry.phase ?? 'stored' }
export function candidates(entries: any[], selected: string[]) {
  return entries.filter((entry) => selected.includes(entry.id) && (entry.can_delete || entry.can_cancel || entry.can_revoke))
}
export function releaseBytes(entries: any[]): number {
  return entries.reduce((sum, entry) => sum + (entry.charged_bytes == null || entry.reserved_bytes == null ? entry.bytes ?? 0 : entry.charged_bytes + entry.reserved_bytes), 0)
}
export function filtered(entries: any[], filter: { text: string; kind: string; phase: string; resource: string; before: string; sort: string }) {
  const needle = filter.text.trim().toLocaleLowerCase()
  const before = filter.before ? new Date(filter.before + 'T23:59:59').getTime() : Infinity
  return entries.filter((entry) => (!needle || `${entry.name ?? ''} ${entry.upload_id ?? ''} ${entry.source_file ?? ''}`.toLocaleLowerCase().includes(needle)) &&
    (!filter.kind || entry.kind === filter.kind) && (!filter.phase || phaseOf(entry) === filter.phase) &&
    (!filter.resource || entry.resource === filter.resource) && (entry.created_at ?? 0) <= before)
    .sort((a, b) => filter.sort === 'size' ? (b.bytes ?? 0) - (a.bytes ?? 0) : (b.created_at ?? 0) - (a.created_at ?? 0))
}
export function targetOf(entry: any, action: string) {
  return { owner_kind: entry.owner_kind, resource: entry.resource, upload_id: entry.upload_id, generation: entry.generation ?? 0, action,
    share_id: action === 'delete' ? entry.share?.share_id ?? null : null }
}
