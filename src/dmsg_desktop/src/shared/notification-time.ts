import { CalendarDateTime, fromDate, toCalendarDate, toZoned, type CalendarDate } from '@internationalized/date'
import type { GlobalNoticeSettings } from './notifications.ts'

function boundary(date: CalendarDate, minute: number, zone: string, end = false): number {
  const wall = new CalendarDateTime(date.year, date.month, date.day, Math.floor(minute / 60), minute % 60)
  return toZoned(wall, zone, end ? 'later' : 'compatible').toDate().getTime()
}

export function validTimeZone(zone: string): boolean {
  try { new Intl.DateTimeFormat('en', { timeZone: zone }).format(); return true } catch { return false }
}

/** Convert one configured local interval to UTC. No persistent timer uses wall-clock minutes. */
export function quietUntil(settings: GlobalNoticeSettings, now: number): number | null {
  const hours = settings.quiet_hours
  if (!hours) return null
  if (!validTimeZone(hours.timezone) || hours.start < 0 || hours.start >= 1440 || hours.end < 0 || hours.end >= 1440 || hours.start === hours.end) {
    throw new Error('INVALID_INPUT: quiet hours')
  }
  const today = toCalendarDate(fromDate(new Date(now), hours.timezone))
  for (const date of [today.subtract({ days: 1 }), today]) {
    const start = boundary(date, hours.start, hours.timezone)
    const end = boundary(hours.end <= hours.start ? date.add({ days: 1 }) : date, hours.end, hours.timezone, true)
    if (now >= start && now < end) return end
  }
  return null
}

export function tomorrowMorning(now: number, zone: string): number {
  const tomorrow = toCalendarDate(fromDate(new Date(now), zone)).add({ days: 1 })
  return boundary(tomorrow, 8 * 60, zone, true)
}
