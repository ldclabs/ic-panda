import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  DraftVersion,
  rotationAllowed,
  sourceItems,
  quoteShell,
  validPath
} from '../src/renderer/views/secrets/model.ts'
import { rows } from '../src/renderer/views/secrets/experience-catalog.ts'

test('an async preview cannot be reused after editing away and back to the same choices', () => {
  const draft = new DraftVersion(),
    pending = draft.capture()
  assert.equal(draft.current(pending), true)
  draft.change()
  draft.change()
  assert.equal(draft.current(pending), false)
  assert.equal(draft.current(draft.capture()), true)
})
test('rotation steps require the checked current source and exact dependency inventory', () => {
  const r = {
    previous_revision: 'old',
    new_revision: null,
    can_report_created: false
  }
  assert.equal(rotationAllowed(r, 'created', 'renamed', null), false)
  assert.equal(
    rotationAllowed({ ...r, can_report_created: true }, 'created', 'new', null),
    true
  )
  assert.equal(rotationAllowed(r, 'tested', 'new', null), false)
  const created = {
    ...r,
    new_revision: 'new',
    created: true,
    tested: true,
    references_checked: true,
    references_digest: 'before'
  }
  assert.equal(
    rotationAllowed(created, 'upstream_revoked', 'new', {
      revision: 'new',
      digest: 'after'
    }),
    false
  )
  assert.equal(
    rotationAllowed(created, 'references_checked', 'new', {
      revision: 'new',
      digest: 'after'
    }),
    true
  )
  assert.equal(
    rotationAllowed(created, 'upstream_revoked', 'new', {
      revision: 'new',
      digest: 'before'
    }),
    true
  )
  assert.equal(
    rotationAllowed(created, 'upstream_revoked', 'changed', {
      revision: 'new',
      digest: 'before'
    }),
    false
  )
})
test('source choices contain only metadata and flag conflicts and managed keys', () => {
  const input = {
    id: 'id',
    revision: 'revision',
    item: {
      title: 'Name',
      type: 'api',
      tags: [],
      fields: [
        {
          id: 'credential',
          label: 'Token',
          type: 'concealed',
          value: 'never retain'
        }
      ],
      notes: 'also secret'
    }
  }
  const [item] = sourceItems([input])
  assert.equal(JSON.stringify(item).includes('secret'), false)
  assert.equal(JSON.stringify(item).includes('never retain'), false)
  assert.equal(item.blocked, false)
  assert.equal(sourceItems([{ ...input, conflict: true }])[0].blocked, true)
  assert.equal(
    sourceItems([
      { ...input, item: { ...input.item, tags: ['dmsg-agent'] } }
    ])[0].blocked,
    true
  )
})
test('generated shell configuration quotes paths and mappings reject traversal', () => {
  assert.equal(quoteShell("a'$(touch nope)"), "'a'\\''$(touch nope)'")
  for (const p of ['.env', 'config/service.json'])
    assert.equal(validPath(p), true)
  for (const p of [
    '',
    '/tmp/key',
    '../key',
    'x/../key',
    'x//key',
    'x\nkey',
    'x\\key'
  ])
    assert.equal(validPath(p), false, p)
})
test('credential workflow copy is complete in all six interface languages', () => {
  const keys = new Set<string>()
  for (const [key, ...values] of rows) {
    assert.equal(keys.has(key), false, key)
    keys.add(key)
    assert.equal(values.length, 6, key)
    for (const value of values) assert.ok(value.trim(), key)
  }
})
