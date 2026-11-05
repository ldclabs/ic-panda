import type { Label } from '../../i18n/messages/en'

/** One link each, at most this many at a time. */
export const MAX_REQUEST_LABELS = 50

export const requestStates: Record<string, Label> = {
  open: 'requestStateOpen',
  full: 'requestStateFull',
  paused: 'requestStatePaused',
  expired: 'requestStateExpired',
  revoked: 'requestStateClosed'
}

/** A link still counts against the plan until it expires or is closed. */
export const isActiveRequest = (link: { status?: string }) =>
  link.status !== 'expired' && link.status !== 'revoked'

/** The names of a list, one per line. */
export function requestLabels(names: string): string[] {
  return names
    .split('\n')
    .map((name) => name.trim())
    .filter(Boolean)
}

const MIB = 1024 * 1024
const TOTALS = [100, 500, 1024, 2048, 5120, 10240]

/** What a link may take in all, in MiB: never more than the plan stores; Max starts higher. */
export function requestTotals(usage: {
  storage_limit?: number | null
  backup_max_file_bytes?: number | null
} | null): { options: number[]; preferred: number } {
  const room = usage?.storage_limit ?? Infinity
  const fits = TOTALS.filter((mib) => mib * MIB <= room)
  const options = fits.length ? fits : [1]
  const wanted = (usage?.backup_max_file_bytes ?? 0) > 100 * MIB ? 2048 : 500
  return { options, preferred: options.includes(wanted) ? wanted : options[options.length - 1]! }
}
