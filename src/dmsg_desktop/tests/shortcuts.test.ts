import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Shortcuts } from '../src/main/shortcuts.ts'
import { defaultShortcuts, recordedShortcut, shortcutLabel, validBindings, validShortcut, type ShortcutBindings } from '../src/shared/shortcuts.ts'

function fixture(save: (value: ShortcutBindings) => Promise<void> = async () => {}) {
  const callbacks = new Map<string, () => void>(), blocked = new Set<string>(), actions: string[] = []
  const bindings = defaultShortcuts('darwin')
  const native = { register(key: string, callback: () => void) {
    if (blocked.has(key)) return false
    assert.equal(callbacks.has(key), false)
    callbacks.set(key, callback)
    return true
  }, unregister(key: string) { callbacks.delete(key) } }
  const manager = new Shortcuts(native, 'darwin', bindings, {
    open: () => actions.push('open'), lock: () => actions.push('lock')
  }, save)
  return { manager, callbacks, blocked, actions, bindings, native }
}

test('global shortcuts are bounded chords, with platform-specific defaults and no duplicate modifiers', () => {
  assert.equal(defaultShortcuts('darwin').open, 'Command+Shift+Space')
  assert.equal(defaultShortcuts('win32').lock, 'Control+Shift+L')
  for (const key of ['A', 'Shift+A', 'Command+Command+A', 'Shift+Command+A', 'Meta+L', 'Command+F25', 'Command+MediaPlayPause']) assert.equal(validShortcut(key, 'darwin'), false, key)
  assert.equal(validShortcut('Command+Shift+Space', 'linux'), false)
  assert.equal(validShortcut('Control+Alt+F12', 'linux'), true)
  assert.equal(validBindings({ open: null, lock: 'Command+Shift+L', run: 'command' }, 'darwin'), false)
  assert.equal(validBindings({ open: null, lock: null }, 'darwin'), true)
  assert.equal(shortcutLabel('Command+Shift+L', 'darwin'), '⌘ + ⇧ + L')
  const event = { key: 'l', code: 'KeyL', metaKey: true, ctrlKey: false, altKey: false, shiftKey: true, isComposing: false }
  assert.equal(recordedShortcut(event, 'darwin'), 'Command+Shift+L')
  assert.equal(recordedShortcut({ ...event, isComposing: true }, 'darwin'), null)
  assert.equal(recordedShortcut({ ...event, key: 'λ', altKey: true }, 'darwin'), 'Command+Alt+Shift+L')
  assert.equal(recordedShortcut({ ...event, metaKey: false }, 'darwin'), null)
})

test('startup reports occupied shortcuts and retries without discarding the other working action', async () => {
  const f = fixture()
  f.blocked.add(f.bindings.open!)
  f.manager.start()
  assert.deepEqual(f.manager.status().registered, { open: false, lock: true })
  f.callbacks.get(f.bindings.lock!)!()
  assert.deepEqual(f.actions, ['lock'])
  f.blocked.clear()
  await f.manager.configure(f.bindings)
  assert.deepEqual(f.manager.status().registered, { open: true, lock: true })
})

test('a failed registration or disk write rolls back new keys and retains the previous bindings', async () => {
  let writes = 0
  const f = fixture(async () => { writes++; throw new Error('disk unavailable') })
  f.manager.start()
  const next = { open: 'Alt+Shift+F11', lock: 'Alt+Shift+F12' }
  f.blocked.add(next.lock)
  await assert.rejects(f.manager.configure(next), /could not be registered/)
  assert.equal(writes, 0)
  assert.deepEqual([...f.callbacks.keys()], Object.values(f.bindings))
  f.blocked.clear()
  await assert.rejects(f.manager.configure(next), /disk unavailable/)
  assert.equal(writes, 1)
  assert.deepEqual(f.manager.status().bindings, f.bindings)
  assert.deepEqual([...f.callbacks.keys()], Object.values(f.bindings))
  f.callbacks.get(f.bindings.open!)!()
  assert.deepEqual(f.actions, ['open'])
})

test('a save keeps the previous actions until persisted, rejects concurrent writes and can swap or disable keys', async () => {
  let finish!: () => void
  let saved: ShortcutBindings | undefined
  const f = fixture(async value => { await new Promise<void>(resolve => { finish = resolve }); saved = value })
  f.manager.start()
  const swapping = { open: f.bindings.lock, lock: f.bindings.open }
  const pending = f.manager.configure(swapping)
  f.callbacks.get(f.bindings.open!)!()
  assert.deepEqual(f.actions, ['open'])
  await assert.rejects(f.manager.configure({ open: null, lock: null }), /saving/)
  finish(); await pending
  assert.deepEqual(saved, swapping)
  f.callbacks.get(f.bindings.open!)!()
  assert.deepEqual(f.actions, ['open', 'lock'])
  const disable = f.manager.configure({ open: null, lock: null })
  finish(); await disable
  assert.equal(f.callbacks.size, 0)
  assert.deepEqual(f.manager.status().registered, { open: false, lock: false })
})

test('saved bindings restart with the same actions, duplicate drafts do nothing and quitting releases only owned keys', async () => {
  let saved: ShortcutBindings | undefined
  const f = fixture(async value => { saved = JSON.parse(JSON.stringify(value)) })
  f.manager.start()
  await assert.rejects(f.manager.configure({ open: 'Alt+L', lock: 'Alt+L' }), /different shortcuts/)
  assert.equal(saved, undefined)
  await f.manager.configure({ open: 'Alt+Shift+F11', lock: null })
  f.manager.dispose()
  assert.equal(f.callbacks.size, 0)
  const restarted = new Shortcuts(f.native, 'darwin', saved!, { open: () => f.actions.push('reopened'), lock: () => assert.fail('disabled') }, async () => {})
  restarted.start()
  f.callbacks.get('Alt+Shift+F11')!()
  assert.deepEqual(f.actions, ['reopened'])
  f.callbacks.set('Control+F10', () => {})
  restarted.dispose()
  assert.deepEqual([...f.callbacks.keys()], ['Control+F10'])
})
