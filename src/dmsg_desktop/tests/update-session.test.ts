import assert from 'node:assert/strict'
import { test } from 'node:test'
import { UpdateSession } from '../src/main/update-session.ts'
import type { DesktopRelease } from '../src/main/release-feed.ts'
import type { UpdateChannel } from '../src/shared/contract.ts'

const candidate = (channel: UpdateChannel): DesktopRelease => ({ version: channel === 'beta' ? '1.3.0-beta.2' : '1.2.4', channel, installer: 'fixture.zip', assets: [] })
function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

test('switching off beta during discovery ignores its late reply and serializes the new check', async () => {
  const found = deferred<DesktopRelease>()
  const started = deferred<void>()
  const channels: UpdateChannel[] = []
  const downloaded: string[] = []
  const session = new UpdateSession({
    enabled: true, channel: 'beta', changed() {},
    find: async channel => {
      channels.push(channel)
      if (channel === 'beta') { started.resolve(); return found.promise }
      return candidate(channel)
    },
    download: async release => { downloaded.push(release.version) }
  })
  const first = session.check()
  assert.equal(session.check(), first)
  await started.promise
  session.changeChannel('stable')
  assert.equal(session.beginApply(), null)
  found.resolve(candidate('beta'))
  await session.check()
  assert.deepEqual(channels, ['beta', 'stable'])
  assert.deepEqual(downloaded, ['1.2.4'])
  assert.deepEqual(session.state, { kind: 'ready', version: '1.2.4' })
})

test('a cancelled download cannot become ready, even if the downloader resolves after cancellation', async () => {
  const downloading = deferred<AbortSignal>()
  const finish = deferred<void>()
  const ready: string[] = []
  const session = new UpdateSession({
    enabled: true, channel: 'beta',
    changed() { if (session.state.kind === 'ready') ready.push(session.state.version) },
    find: async channel => channel === 'beta' ? candidate(channel) : null,
    download: async (_release, signal) => { downloading.resolve(signal); await finish.promise }
  })
  void session.check()
  const signal = await downloading.promise
  session.changeChannel('stable')
  assert.equal(signal.aborted, true)
  finish.resolve()
  await session.check()
  assert.deepEqual(session.state, { kind: 'latest' })
  assert.deepEqual(ready, [])
  assert.equal(session.beginApply(), null)
})

test('switching off beta invalidates a downloaded package; installation locks the channel until released', async () => {
  const session = new UpdateSession({ enabled: true, channel: 'beta', changed() {}, find: async channel => channel === 'beta' ? candidate(channel) : null, download: async () => {} })
  await session.check()
  assert.equal(session.beginApply()?.version, '1.3.0-beta.2')
  assert.equal(session.beginApply(), null)
  assert.throws(() => session.changeChannel('stable'), /being installed/)
  session.endApply()
  session.beginApply()
  session.installFailed(new Error('Signature verification failed'))
  assert.equal(session.applying, false)
  assert.deepEqual(session.state, { kind: 'error', message: 'Signature verification failed' })
  assert.equal(session.beginApply(), null)
  await session.check()
  session.changeChannel('stable')
  assert.equal(session.beginApply(), null)
  await session.check()
  assert.deepEqual(session.state, { kind: 'latest' })
})

test('errors are retryable, stale errors do not overwrite a new channel, and development never fetches', async () => {
  let attempts = 0
  const session = new UpdateSession({ enabled: true, channel: 'stable', changed() {}, find: async () => {
    if (++attempts === 1) throw new Error('Network unavailable\nprivate HTTP details')
    return candidate('stable')
  } })
  await session.check()
  assert.deepEqual(session.state, { kind: 'error', message: 'Network unavailable' })
  await session.check()
  assert.deepEqual(session.state, { kind: 'available', version: '1.2.4' })
  const late = deferred<DesktopRelease | null>()
  const stale = new UpdateSession({ enabled: true, channel: 'beta', changed() {}, find: async channel => channel === 'beta' ? late.promise : null })
  void stale.check()
  await Promise.resolve()
  stale.changeChannel('stable')
  late.reject(new Error('Late beta error'))
  await stale.check()
  assert.deepEqual(stale.state, { kind: 'latest' })
  const dev = new UpdateSession({ enabled: false, channel: 'stable', changed() {}, find: async () => { throw new Error('Must not fetch') } })
  dev.changeChannel('beta')
  assert.deepEqual(await dev.check(), { kind: 'off' })
})
