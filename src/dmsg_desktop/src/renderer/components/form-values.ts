export const textInputTypes = ['text', 'search', 'url', 'email', 'tel'] as const
export type TextInputType = (typeof textInputTypes)[number]
export type DateMode = 'date' | 'month' | 'datetime'

/** Reject rollover and nonexistent local times, without converting calendar-only values to UTC. */
export function validDateText(value: string, mode: DateMode): boolean {
  const pattern =
    mode === 'month'
      ? /^(\d{4})-(\d{2})$/
      : mode === 'date'
        ? /^(\d{4})-(\d{2})-(\d{2})$/
        : /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/
  const match = pattern.exec(value)
  if (!match) return false
  const [, y, m, d = '01', h = '00', min = '00'] = match
  const date = new Date(0)
  date.setFullYear(Number(y), Number(m) - 1, Number(d))
  date.setHours(Number(h), Number(min), 0, 0)
  return (
    Number(y) > 0 &&
    date.getFullYear() === Number(y) &&
    date.getMonth() === Number(m) - 1 &&
    date.getDate() === Number(d) &&
    date.getHours() === Number(h) &&
    date.getMinutes() === Number(min)
  )
}

export function localMinute(at: number): string {
  const date = new Date(at)
  return new Date(at - date.getTimezoneOffset() * 60_000)
    .toISOString()
    .slice(0, 16)
}

export function progressValue(value: number | null, max: number) {
  if (
    value === null ||
    !Number.isFinite(value) ||
    !Number.isFinite(max) ||
    max <= 0
  )
    return { value: null, max: 100, percent: 0 }
  const current = Math.max(0, Math.min(value, max))
  return { value: current, max, percent: (current / max) * 100 }
}
