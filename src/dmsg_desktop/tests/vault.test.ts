import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  below,
  charsets,
  defaultOptions,
  differences,
  drafts,
  entropy,
  facets,
  fieldChanges,
  fieldName,
  findingsByItem,
  generate,
  healthKey,
  inScope,
  isManaged,
  line,
  lineDiff,
  matches,
  sections,
  sorted,
  spaced,
  type Field,
  type Random
} from '../src/renderer/views/vault/vault.ts'

const field = (id: string, type: Field['type'], value: string, label = '', section = ''): Field => ({ id, type, label, section, value })

const entry = (item: Record<string, unknown>, deleted = false, id = 'a') => ({
  id,
  revision: 'r1',
  deleted,
  at: 0,
  item: { type: 'note', title: '', tags: [], notes: '', fields: [], files: [], favorite: false, createdAt: 0, updatedAt: 0, ...item }
})

const t = (label: string) => `<${label}>`

test('a search looks in the title, tags, notes, labels and shown values, never hidden ones', () => {
  const login = entry({
    type: 'login',
    title: 'GitHub',
    tags: ['work'],
    notes: 'org admin',
    fields: [field('username', 'text', 'mira'), field('password', 'concealed', '********'), field('url', 'url', 'https://github.com'), field('x1', 'concealed', '********', 'Recovery codes')]
  })
  for (const query of ['github', 'WORK', 'Mira', 'github.com', 'admin', 'recovery', '  ']) assert.equal(matches(login.item, query), true, query)
  assert.equal(matches(login.item, '****'), false)
  assert.equal(matches(login.item, 'gitlab'), false)
})

test('filters show starred items, flagged items, one type or one tag, and count them', () => {
  const items = [
    entry({ type: 'login', tags: ['work', 'infra'], favorite: true }, false, 'a'),
    entry({ type: 'note', tags: ['work'] }, false, 'b'),
    entry({ type: 'wallet', tags: ['family'] }, false, 'c')
  ]
  assert.equal(items.filter((e) => inScope(e, 'starred')).length, 1)
  assert.equal(items.filter((e) => inScope(e, 'type:note')).length, 1)
  assert.equal(items.filter((e) => inScope(e, 'tag:work')).length, 2)
  assert.equal(items.filter((e) => inScope(e, 'health', new Set(['c']))).length, 1)
  assert.equal(items.filter((e) => inScope(e, 'all')).length, 3)
  assert.deepEqual(facets(items), {
    all: 3,
    starred: 1,
    types: { login: 1, note: 1, wallet: 1 },
    tags: [
      ['work', 2],
      ['family', 1],
      ['infra', 1]
    ]
  })
})

test('fields are named by their label, else their template name, and grouped by section', () => {
  assert.equal(fieldName(field('password', 'concealed', ''), t), '<fieldPassword>')
  assert.equal(fieldName(field('password', 'concealed', '', 'Wi-Fi'), t), 'Wi-Fi')
  assert.equal(fieldName(field('ab12cd34', 'text', ''), t), 'ab12cd34')
  const grouped = sections([field('a', 'text', '1', '', 'Server'), field('b', 'text', '2'), field('c', 'text', '3', '', 'Server')])
  assert.deepEqual(
    grouped.map((s) => [s.title, s.fields.map((f) => f.id)]),
    [
      ['', ['b']],
      ['Server', ['a', 'c']]
    ]
  )
  const item = { type: 'login', notes: 'n', fields: [field('password', 'concealed', '********'), field('url', 'url', 'https://a.example')] }
  assert.equal(line(item, t), 'a.example')
  assert.equal(line({ type: 'card', notes: '', fields: [] }, t), '<typeCard>')
  assert.equal(isManaged({ tags: ['dmsg-agent'] }), true)
})

test('an edit keeps hidden values it never loaded, and changes or clears them on request', () => {
  const saved = [field('username', 'text', 'mira'), field('password', 'concealed', '********'), field('pin', 'concealed', '')]
  const form = drafts(saved)
  assert.deepEqual(
    form.map((d) => [d.value, d.hiddenState]),
    [
      ['mira', undefined],
      ['', 'keep'],
      ['', undefined]
    ]
  )
  assert.equal('value' in fieldChanges(form)[1], false)
  // A change left empty keeps the saved value; only Clear empties it.
  form[1] = { ...form[1], hiddenState: 'change' }
  assert.equal('value' in fieldChanges(form)[1], false)
  form[1] = { ...form[1], value: 'new' }
  assert.equal(fieldChanges(form)[1].value, 'new')
  form[1] = { ...form[1], value: '', hiddenState: 'clear' }
  form[2] = { ...form[2], value: '1234' }
  const changes = fieldChanges(form)
  assert.equal(changes[1].value, '')
  assert.equal(changes[2].value, '1234')
  assert.equal(fieldChanges([{ ...field('', 'text', 'x'), label: 'New' }])[0].id, undefined)
})

test('a conflict copy names the parts it changed; hidden values only as the daemon reports them', () => {
  const head = entry({ title: 'Plan', notes: 'a\nb', tags: ['x'], fields: [field('password', 'concealed', '********'), field('url', 'url', 'https://a')] })
  const copy = entry(
    { title: 'Plan', notes: 'a\nc', tags: ['x', 'y'], fields: [field('password', 'concealed', '********'), field('pin', 'concealed', '********')] },
    true
  )
  assert.deepEqual(differences(head, copy), ['tags', 'notes', 'field:url', 'field:pin', 'deleted'])
  assert.deepEqual(differences(head, copy, ['password', 'pin']), ['tags', 'notes', 'field:password', 'field:url', 'field:pin', 'deleted'])
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

test('the list sorts by edit, creation or title; the trash by deletion', () => {
  const list = [
    { ...entry({ title: 'b', createdAt: 1, updatedAt: 9 }), at: 5 },
    { ...entry({ title: 'a', createdAt: 3, updatedAt: 2 }), at: 7 }
  ]
  assert.deepEqual(
    sorted(list, 'updated').map((e) => e.item.title),
    ['b', 'a']
  )
  assert.deepEqual(
    sorted(list, 'created').map((e) => e.item.title),
    ['a', 'b']
  )
  assert.deepEqual(
    sorted(list, 'title').map((e) => e.item.title),
    ['a', 'b']
  )
  assert.deepEqual(
    sorted(list, 'updated', true).map((e) => e.item.title),
    ['a', 'b']
  )
})

/** A counter instead of randomness, so every draw is known. */
function counter(): Random {
  let n = 0
  return (values) => {
    for (let i = 0; i < values.length; i++) values[i] = n++
    return values
  }
}

test('the generator draws uniformly and keeps every kind of character asked for', () => {
  const random = counter()
  assert.deepEqual(
    Array.from({ length: 5 }, () => below(3, random)),
    [0, 1, 2, 0, 1]
  )
  // A value past the last whole multiple is drawn again.
  let drawn = 0
  const high: Random = (values) => ((values[0] = drawn++ === 0 ? 0xffff_ffff : 7), values)
  assert.equal(below(10, high), 7)
  assert.equal(drawn, 2)

  const secure: Random = (values) => crypto.getRandomValues(values)
  for (let i = 0; i < 50; i++) {
    const password = generate({ ...defaultOptions, length: 12 }, secure)
    assert.equal(password.length, 12)
    for (const set of charsets(defaultOptions)) assert.ok([...password].some((c) => set.includes(c)), password)
    assert.ok(!/[0Oo1lI|]/.test(password), password)
  }
  assert.match(generate({ ...defaultOptions, mode: 'pin', length: 6 }, secure), /^\d{6}$/)
  const words = generate({ ...defaultOptions, mode: 'words', words: 4, separator: '.' }, secure, ['alpha', 'beta', 'gamma'])
  assert.match(words, /^(alpha|beta|gamma)(\.(alpha|beta|gamma)){3}$/)
  assert.equal(Math.round(entropy({ ...defaultOptions, mode: 'words', words: 5 })), 65)
  assert.ok(entropy({ ...defaultOptions, length: 20 }) > 100)
})

test('codes read in halves and findings group by item and field', () => {
  assert.equal(spaced('123456'), '123 456')
  assert.equal(spaced('12345678'), '1234 5678')
  const byItem = findingsByItem([
    { id: 'a', field: 'password', issue: 'weak' },
    { id: 'a', field: 'password', issue: 'reused' },
    { id: 'b', field: 'url', issue: 'insecure' }
  ])
  assert.deepEqual(byItem.get('a')?.get('password'), ['weak', 'reused'])
  assert.deepEqual([...byItem.keys()], ['a', 'b'])
})


test('security findings are reused across sync ACKs, but not edits, removals or days', () => {
  const first = { id: 'a', revision: '1', synced: false }
  const second = { id: 'b', revision: '1', synced: false }
  const now = 86_400_000
  const key = healthKey([first, second], now)
  assert.equal(healthKey([{ ...second }, { ...first }].map((entry) => ({ ...entry, synced: true })), now + 1), key)
  assert.notEqual(healthKey([{ ...first, revision: '2' }, second], now), key)
  assert.notEqual(healthKey([first], now), key)
  assert.notEqual(healthKey([first, second], now + 86_400_000), key)
})
