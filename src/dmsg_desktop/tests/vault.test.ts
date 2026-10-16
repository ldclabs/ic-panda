import assert from 'node:assert/strict'
import { test } from 'node:test'
import { differences, facets, inScope, lineDiff, matches } from '../src/renderer/views/vault/vault.ts'

const entry = (item: Record<string, unknown>, deleted = false) => ({
  id: 'a',
  revision: 'r1',
  deleted,
  item: { type: 'note', title: '', tags: [], body: '', username: '', secret: '', url: '', favorite: false, ...item }
})

test('a search looks in the title, tags, body, user name and URL, ignoring case', () => {
  const login = entry({ type: 'login', title: 'GitHub', tags: ['work'], username: 'mira', url: 'https://github.com', body: 'org admin', secret: '********' })
  for (const query of ['github', 'WORK', 'Mira', 'github.com', 'admin', '  ']) assert.equal(matches(login.item, query), true, query)
  assert.equal(matches(login.item, '****'), false)
  assert.equal(matches(login.item, 'gitlab'), false)
})

test('filters show starred items, one type or one tag, and count them', () => {
  const items = [
    entry({ type: 'login', tags: ['work', 'infra'], favorite: true }),
    entry({ type: 'note', tags: ['work'] }),
    entry({ type: 'file', tags: ['family'] })
  ]
  assert.equal(items.filter((e) => inScope(e, 'starred')).length, 1)
  assert.equal(items.filter((e) => inScope(e, 'type:note')).length, 1)
  assert.equal(items.filter((e) => inScope(e, 'tag:work')).length, 2)
  assert.equal(items.filter((e) => inScope(e, 'all')).length, 3)
  assert.deepEqual(facets(items), {
    all: 3,
    starred: 1,
    types: { login: 1, note: 1, file: 1 },
    tags: [
      ['work', 2],
      ['family', 1],
      ['infra', 1]
    ]
  })
})

test('a conflict copy names the fields it changed, never the masked secret', () => {
  const head = entry({ title: 'Plan', body: 'a\nb', secret: '********', tags: ['x'] })
  const copy = entry({ title: 'Plan', body: 'a\nc', secret: '********', tags: ['x', 'y'] }, true)
  assert.deepEqual(differences(head, copy), ['tags', 'body', 'deleted'])
  assert.deepEqual(differences(head, head), [])
})

test('a line diff marks the lines each side lacks', () => {
  const diff = lineDiff('1. freeze\n2. sign\n3. upload', '1. freeze\n2. sign in dMsg\n3. upload\n4. announce')
  assert.deepEqual(
    diff.current.map((l) => l.changed),
    [false, true, false]
  )
  assert.deepEqual(
    diff.copy.map((l) => l.changed),
    [false, true, false, true]
  )
  assert.deepEqual(lineDiff('', 'x').copy, [{ text: 'x', changed: true }])
})
