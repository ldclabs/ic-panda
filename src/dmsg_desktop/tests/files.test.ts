import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ancestorsOf, carries, DRAG_TYPE, draggedIds, isSort, matchesFile, ordered, within } from '../src/renderer/views/files/files.ts'

const file = (name: string, updated_at: number, size: number, mime = 'application/octet-stream') => ({ name, updated_at, size, mime })

test('folders come first; files order by name, time, size or type, ties by name', () => {
  const folders = [file('b', 1, 0), file('A', 9, 0)]
  const files = [file('report 10.pdf', 3, 50, 'application/pdf'), file('report 2.pdf', 1, 50, 'application/pdf'), file('notes.txt', 2, 9, 'text/plain')]
  const names = (sorted: { name: string }[]) => sorted.map((f) => f.name)
  assert.deepEqual(names(ordered(folders, files, 'en')), ['A', 'b', 'notes.txt', 'report 2.pdf', 'report 10.pdf'])
  assert.deepEqual(names(ordered(folders, files, 'en', { key: 'modified', descending: true })), ['A', 'b', 'report 10.pdf', 'notes.txt', 'report 2.pdf'])
  assert.deepEqual(names(ordered(folders, files, 'en', { key: 'size', descending: false })), ['A', 'b', 'notes.txt', 'report 2.pdf', 'report 10.pdf'])
  assert.deepEqual(names(ordered(folders, files, 'en', { key: 'type', descending: true })), ['A', 'b', 'notes.txt', 'report 2.pdf', 'report 10.pdf'])
  assert.equal(isSort({ key: 'size', descending: true }), true)
  assert.equal(isSort({ key: 'color', descending: true }), false)
  assert.equal(isSort(null), false)
})

test('a folder knows its ancestors and what lies under it', () => {
  const folders = [
    { id: 'a', parent: 'root', name: 'docs' },
    { id: 'b', parent: 'a', name: '2026' },
    { id: 'c', parent: 'b', name: 'q4' }
  ]
  assert.deepEqual(ancestorsOf(folders, 'c'), [
    { id: 'a', name: 'docs' },
    { id: 'b', name: '2026' },
    { id: 'c', name: 'q4' }
  ])
  assert.deepEqual(ancestorsOf(folders, 'root'), [])
  // A cycle the daemon would never send still ends.
  assert.equal(ancestorsOf([{ id: 'x', parent: 'y', name: 'x' }, { id: 'y', parent: 'x', name: 'y' }], 'x').length <= 3, true)
  assert.equal(within(folders, 'c', 'a'), true)
  assert.equal(within(folders, 'a', 'c'), false)
})

test('a drag carries library rows by their IDs, or files from the system', () => {
  const drag = (types: string[], data = '') => ({ dataTransfer: { types, getData: (type: string) => (type === DRAG_TYPE ? data : '') } }) as unknown as DragEvent
  assert.deepEqual(draggedIds(drag([DRAG_TYPE], '["a","b"]')), ['a', 'b'])
  assert.deepEqual(draggedIds(drag([DRAG_TYPE], '{"a":1}')), [])
  assert.deepEqual(draggedIds(drag([DRAG_TYPE], 'not json')), [])
  assert.deepEqual(draggedIds(drag(['Files'])), [])
  assert.equal(carries(drag(['Files'])), true)
  assert.equal(carries(drag([DRAG_TYPE])), true)
  assert.equal(carries(drag(['text/plain'])), false)
})

test('a search looks in the name, tags and note', () => {
  const report = { name: 'Report.pdf', tags: ['work'], note: 'final draft' }
  for (const query of ['report', 'WORK', 'draft', ' ']) assert.equal(matchesFile(report, query), true, query)
  assert.equal(matchesFile(report, 'invoice'), false)
})
