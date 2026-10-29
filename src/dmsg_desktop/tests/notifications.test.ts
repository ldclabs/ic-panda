import assert from 'node:assert/strict'
import { EventEmitter } from 'node:events'
import { test } from 'node:test'
import { Notifications, nativeNoticeId } from '../src/main/notifications.ts'
import { isNoticePlan, isNoticeSequence, isNoticeTarget, noticeRoute, type GlobalNoticeSettings, type NoticePlan } from '../src/shared/notifications.ts'
import { quietUntil, tomorrowMorning, validTimeZone } from '../src/shared/notification-time.ts'
import { notificationCatalogs, notificationText } from '../src/shared/notification-text.ts'
import { locales } from '../src/shared/locales.ts'
import { isRead } from '../src/shared/requests.ts'

const makePlan = (changes: Partial<NoticePlan> = {}): NoticePlan => ({
  system_id: 'a'.repeat(64), delivery_id: 'b'.repeat(64), account: '040g2081040g2081040g',
  category: 'message', target: { kind: 'channel', id: 'c'.repeat(64) },
  covered: [{ id: 'd'.repeat(64), version: '9007199254740993', stage: 1 }], count: 2,
  sound: false, in_app: false, update: false, attempted_at: 1000, ...changes
})
class Note extends EventEmitter {
  id: string
  shows = 0
  closes = 0
  constructor(id: string) { super(); this.id = id }
  show() { this.shows++ }
  close() { this.closes++ }
}

function fixture() {
  const options: any[] = [], notes: Note[] = [], opened: NoticePlan[] = [], hints: NoticePlan[] = [], results: string[] = []
  let foreground = false
  const presenter = new Notifications({
    create: (value) => { options.push(value); const note = new Note(value.id); notes.push(note); return note },
    body: () => 'Fixed generic text', foreground: () => foreground,
    inApp: (plan) => hints.push(plan), open: async (plan) => { opened.push(plan) },
    result: async (_plan, result) => { results.push(result) }
  })
  return { presenter, options, notes, opened, hints, results, focus: (value: boolean) => { foreground = value } }
}

test('notification text is fixed, repeated consumption never shows a second banner', () => {
  const f = fixture(), plan = makePlan()
  f.presenter.present({ ...plan, title: 'private account name', body: 'private correspondence' })
  f.presenter.present(plan)
  assert.equal(f.notes.length, 1)
  assert.equal(f.notes[0].shows, 1)
  assert.deepEqual(f.options[0], { id: nativeNoticeId(plan.system_id), groupId: nativeNoticeId(`${plan.account}:message`), title: 'dMsg', body: 'Fixed generic text', silent: true })
  f.notes[0].emit('click')
  assert.equal(f.opened[0].account, plan.account)
  assert.equal(f.opened[0].target, plan.target)
})

test('restore reattaches the original account without showing history again', async () => {
  const f = fixture(), plan = makePlan(), restored = new Note(nativeNoticeId(plan.system_id))
  f.presenter.restore([restored], [plan])
  f.presenter.present(plan)
  assert.equal(restored.shows, 0)
  assert.equal(f.notes.length, 0)
  assert.equal(await f.presenter.activate(restored.id), true)
  assert.equal(f.opened[0].account, plan.account)
  assert.equal(await f.presenter.activate('unknown'), false)
  f.presenter.remove([plan.system_id])
  assert.equal(restored.closes, 1)
  assert.equal(await f.presenter.activate(restored.id), false)
})

test('new focus converts a pending OS banner to one quiet application hint', () => {
  const f = fixture()
  f.focus(true)
  f.presenter.present(makePlan({ sound: true }))
  assert.equal(f.options.length, 0)
  assert.equal(f.hints.length, 1)
  assert.deepEqual(f.results, ['suppressed'])
})

test('an explicit native failure permits the core to retry the original ID quietly', () => {
  const f = fixture(), plan = makePlan({ sound: true })
  f.presenter.present(plan)
  f.notes[0].emit('failed')
  f.presenter.present({ ...plan, sound: false })
  assert.equal(f.notes.length, 2)
  assert.equal(f.options[0].id, f.options[1].id)
  assert.equal(f.options[1].silent, true)
  assert.deepEqual(f.results, ['submitted', 'failed', 'submitted'])
  // Late callbacks on the replaced native object cannot affect the new attempt.
  f.notes[0].emit('failed')
  assert.equal(f.results.length, 3)
})

test('only closed targets and lossless sequence strings reach the presenter', () => {
  assert.equal(isNoticeSequence('9007199254740993'), true)
  for (const value of [9007199254740993, '01', '18446744073709551616', '-1']) assert.equal(isNoticeSequence(value), false)
  assert.equal(isNoticeTarget({ kind: 'channel', id: 'https://example.com' }), false)
  assert.equal(isNoticeTarget({ kind: 'rpc', id: 'approval/respond' }), false)
  assert.equal(isNoticePlan(makePlan()), true)
  const f = fixture()
  f.presenter.present(makePlan({ target: { kind: 'channel', id: '../another-account' } }))
  assert.equal(f.options.length, 0)
  assert.equal(noticeRoute({ kind: 'approval', id: 'c'.repeat(64) }), `/approvals?open=${'c'.repeat(64)}`)
  for (const method of ['notifications/next', 'notifications/result', 'notifications/context', 'notifications/observed']) assert.equal(isRead(method), false)
  for (const method of ['notifications/list', 'notifications/status', 'notifications/settings']) assert.equal(isRead(method), true)
})

const quiet = (timezone: string, start = 1320, end = 480): GlobalNoticeSettings => ({ revision: 1, enabled: true, paused_until: null, quiet_hours: { start, end, timezone }, security_exception: false, badge: 'all' })
test('quiet hours cross midnight and follow the configured IANA clock through DST', () => {
  assert.equal(quietUntil(quiet('Asia/Shanghai'), Date.parse('2026-10-28T15:00:00Z')), Date.parse('2026-10-29T00:00:00Z'))
  assert.equal(quietUntil(quiet('Asia/Shanghai'), Date.parse('2026-10-29T02:00:00Z')), null)
  assert.equal(quietUntil(quiet('America/New_York'), Date.parse('2026-03-08T06:30:00Z')), Date.parse('2026-03-08T12:00:00Z'))
  assert.equal(quietUntil(quiet('America/New_York'), Date.parse('2026-11-01T05:30:00Z')), Date.parse('2026-11-01T13:00:00Z'))
  assert.equal(quietUntil(quiet('America/New_York', 1320, 90), Date.parse('2026-11-01T05:45:00Z')), Date.parse('2026-11-01T06:30:00Z'))
  assert.equal(tomorrowMorning(Date.parse('2026-10-28T15:30:00Z'), 'Asia/Shanghai'), Date.parse('2026-10-29T00:00:00Z'))
  assert.equal(validTimeZone('not/a-zone'), false)
})

test('notification settings and native text cover every supported interface locale', () => {
  assert.deepEqual(Object.keys(notificationCatalogs).sort(), [...locales].sort())
  const keys = Object.keys(notificationText('en')).sort()
  for (const locale of locales) {
    assert.deepEqual(Object.keys(notificationText(locale)).sort(), keys)
    assert.ok(Object.values(notificationText(locale)).every((value) => typeof value === 'string' && value.length > 0))
  }
})
