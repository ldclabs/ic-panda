import assert from 'node:assert/strict'
import { test } from 'node:test'
import { boundsOnScreen, isViewRoute, readDesktopState } from '../src/main/desktop-state.ts'
import { linkParts } from '../src/renderer/components/links.ts'
import { best, GROUP_LIMIT, matchScore, tokensOf } from '../src/renderer/shell/palette.ts'
import { arrange, firstUnread, listTime } from '../src/renderer/views/channels/channels.ts'
import { forgetPlain, lastRoute, noteRoute, remember, remembered, rememberedPlain, rememberPlain, settleMemory } from '../src/renderer/view-memory.ts'

test('the palette ranks a name that starts with the search first and stops at its limit', () => {
  const tokens = tokensOf('  Git  Hub ')
  assert.deepEqual(tokens, ['git', 'hub'])
  assert.equal(matchScore('GitHub token', [], tokens), 3)
  assert.equal(matchScore('My github key', [], tokens), 2)
  assert.equal(matchScore('Work key', ['github'], tokens), 1)
  assert.equal(matchScore('Work key', ['gitlab'], tokens), 0)
  const entries = [{ label: 'Work key', keywords: ['github'] }, { label: 'My github key' }, { label: 'GitHub token' }, { label: 'Unrelated' }]
  assert.deepEqual(best(entries, tokens).map((e) => e.label), ['GitHub token', 'My github key', 'Work key'])
  const many = Array.from({ length: GROUP_LIMIT * 3 }, (_, i) => ({ label: `Item ${i}` }))
  const found = best(many, ['item'])
  assert.equal(found.length, GROUP_LIMIT)
  assert.equal(found[0].label, 'Item 0', 'equal scores keep their order')
})

test('addresses become links without the punctuation of their sentence', () => {
  assert.deepEqual(linkParts('see https://dmsg.net/a, then'), [
    { text: 'see ' },
    { text: 'https://dmsg.net/a', url: 'https://dmsg.net/a', host: 'dmsg.net' },
    { text: ', then' }
  ])
  assert.deepEqual(linkParts('（https://dmsg.net）。'), [{ text: '（' }, { text: 'https://dmsg.net', url: 'https://dmsg.net', host: 'dmsg.net' }, { text: '）。' }])
  const parts = linkParts('(https://en.wikipedia.org/wiki/A_(b))')
  assert.equal(parts[1].text, 'https://en.wikipedia.org/wiki/A_(b)')
  assert.equal(linkParts('dmsg://channels/1')[0].url, 'dmsg://channels/1')
  for (const plain of ['no address here', 'javascript:alert(1)', 'file:///etc/passwd', 'ftp://host/x']) assert.deepEqual(linkParts(plain), [{ text: plain }])
})

test('places last until the account changes, and plaintext only until it locks', () => {
  settleMemory({ account: 'a', state: 'unlocked' })
  remember('vault.selected', 'id')
  noteRoute('/vault', '/vault/ssh')
  rememberPlain('draft', 'hello')
  settleMemory({ account: 'a', state: 'locked' })
  assert.equal(remembered('vault.selected'), 'id')
  assert.equal(lastRoute('/vault'), '/vault/ssh')
  assert.equal(rememberedPlain('draft'), undefined)
  rememberPlain('draft', 'again')
  forgetPlain()
  assert.equal(rememberedPlain('draft'), undefined)
  settleMemory({ account: 'b', state: 'unlocked' })
  assert.equal(remembered('vault.selected'), undefined)
  assert.equal(lastRoute('/vault'), '/vault')
  remember('vault.selected', 'id')
  remember('vault.selected', null)
  assert.equal(remembered('vault.selected'), undefined)
})

test('a route that asks for something once is not gone back to', () => {
  settleMemory({ account: 'c', state: 'unlocked' })
  noteRoute('/channels', '/channels?open=1')
  assert.equal(lastRoute('/channels'), '/channels')
  noteRoute('/wallet', '/wallet/w?tab=team')
  assert.equal(lastRoute('/wallet'), '/wallet/w?tab=team')
  noteRoute('/wallet', '/wallet/w?move=3')
  assert.equal(lastRoute('/wallet'), '/wallet/w?tab=team')
})

test('a departing view cannot put plaintext back after the session was invalidated', () => {
  settleMemory({ account: 'a', state: 'unlocked' })
  rememberPlain('draft', 'before lock')
  forgetPlain()
  // Svelte destroys the old view after App handles session/changed.
  rememberPlain('draft', 'saved by the old view on destroy')
  assert.equal(rememberedPlain('draft'), undefined)
  settleMemory({ account: 'a', state: 'locked' })
  rememberPlain('draft', 'locked view teardown')
  assert.equal(rememberedPlain('draft'), undefined)
  settleMemory({ account: 'a', state: 'unlocked' })
  rememberPlain('draft', 'new draft')
  assert.equal(rememberedPlain('draft'), 'new draft')
})

test('the desktop state keeps only what it knows and only views', () => {
  assert.equal(isViewRoute('/vault/ssh'), true)
  assert.equal(isViewRoute('/approval/abc'), false)
  assert.equal(isViewRoute('/channels?open=secret'), false)
  const state = readDesktopState(JSON.stringify({ bounds: { x: 1, y: 2, width: 0, height: 5 }, route: '/approval/1', maximized: 'yes', other: 1 }))
  assert.deepEqual(state, { bounds: undefined, maximized: false, route: undefined, lockOnScreenLock: true, closeHintShown: false })
  assert.equal(readDesktopState('{"lockOnScreenLock":false}').lockOnScreenLock, false)
})

test('a window out of reach opens where the system puts it', () => {
  const display = { x: 0, y: 0, width: 1440, height: 900 }
  const kept = { x: 100, y: 100, width: 900, height: 600 }
  assert.deepEqual(boundsOnScreen(kept, [display]), kept)
  assert.equal(boundsOnScreen({ ...kept, x: 3000 }, [display]), undefined)
  assert.equal(boundsOnScreen({ ...kept, y: 880 }, [display]), undefined, 'its title bar would be off the screen')
  assert.equal(boundsOnScreen(undefined, [display]), undefined)
})

test('pinned channels lead the list and the rest follow by activity', () => {
  const member = (id: string, at: number) => ({ id, name: id, active_at: at, ledger: { status: 'active', members: { me: {} } } })
  const list = [member('old', 1), member('new', 3), member('pinned', 2), { id: 'left', name: 'left', ledger: { status: 'active', members: {} } }]
  const { live, other } = arrange(list, 'me', ['pinned'])
  assert.deepEqual(live.map((c) => c.id), ['pinned', 'new', 'old'])
  assert.deepEqual(other.map((c) => c.id), ['left'])
})

test('a channel opens at the first message of someone else that is new', () => {
  const messages = [{ seq: 1, account: 'them' }, { seq: 2, account: 'me' }, { seq: 3, account: 'them' }]
  assert.equal(firstUnread(messages, 1, 'me'), 3)
  assert.equal(firstUnread(messages, 3, 'me'), null)
})

test('a list row says when in as few words as it can', () => {
  const now = new Date(2026, 10, 6, 15, 0).getTime()
  assert.match(listTime(new Date(2026, 10, 6, 9, 5).getTime(), now, 'en'), /9:05/)
  assert.match(listTime(new Date(2026, 10, 5, 9, 5).getTime(), now, 'en'), /yesterday/i)
  assert.match(listTime(new Date(2026, 10, 3, 9, 5).getTime(), now, 'en'), /Tue/)
  assert.match(listTime(new Date(2025, 10, 3, 9, 5).getTime(), now, 'en'), /25/)
})
