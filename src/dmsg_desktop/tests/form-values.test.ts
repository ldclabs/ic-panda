import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  validDateText,
  localMinute,
  progressValue,
  textInputTypes
} from '../src/renderer/components/form-values.ts'

test('calendar values reject rollover, malformed input and invalid month ranges', () => {
  for (const value of ['2024-02-29', '2026-10-26', '1900-01-01'])
    assert.equal(validDateText(value, 'date'), true)
  for (const value of [
    '',
    '2026-02-29',
    '2026-04-31',
    '2026-13-01',
    '2026-00-01',
    '0000-01-01',
    '26-01-01',
    '2026-1-1'
  ])
    assert.equal(validDateText(value, 'date'), false)
  assert.equal(validDateText('2026-10', 'month'), true)
  assert.equal(validDateText('2026-13', 'month'), false)
  assert.equal(validDateText('2026-10-01', 'month'), false)
})

test('local date-time round trips preserve minutes and reject impossible hours', () => {
  const date = new Date(2026, 9, 26, 15, 32)
  assert.equal(localMinute(date.getTime()), '2026-10-26T15:32')
  assert.equal(new Date(localMinute(date.getTime())).getTime(), date.getTime())
  assert.equal(validDateText('2026-10-26T15:32', 'datetime'), true)
  assert.equal(validDateText('2026-10-26T24:00', 'datetime'), false)
  assert.equal(validDateText('2026-10-26T15:60', 'datetime'), false)
})

test('progress bounds overshoot and does not claim zero completion for an unknown total', () => {
  assert.deepEqual(progressValue(2, 4), { value: 2, max: 4, percent: 50 })
  assert.deepEqual(progressValue(5, 4), { value: 4, max: 4, percent: 100 })
  assert.equal(progressValue(null, 4).value, null)
  assert.equal(progressValue(2, 0).value, null)
  assert.equal(progressValue(NaN, 4).value, null)
  assert.equal(progressValue(-1, 4).value, 0)
  assert.deepEqual(textInputTypes, ['text', 'search', 'url', 'email', 'tel'])
})
