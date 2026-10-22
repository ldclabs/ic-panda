// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { ProfilePage, ProfileWorkerResponse } from './public-profile-types'

class BackgroundWorker {
  static instances: BackgroundWorker[] = []
  onmessage: ((event: MessageEvent<ProfileWorkerResponse>) => void) | null =
    null
  onerror: (() => void) | null = null
  onmessageerror: (() => void) | null = null
  postMessage = vi.fn()
  terminate = vi.fn()
  constructor(
    readonly url: URL,
    readonly options: WorkerOptions
  ) {
    BackgroundWorker.instances.push(this)
  }
  reply(data: ProfileWorkerResponse) {
    this.onmessage?.({ data } as MessageEvent<ProfileWorkerResponse>)
  }
  get lastId(): number {
    return this.postMessage.mock.calls.at(-1)![0].id
  }
}
let readProfile: (typeof import('./public-profile'))['readProfile']
let ProfileError: (typeof import('./public-profile'))['ProfileError']
const page = (account = 'account-a', updated_at = 1): ProfilePage => ({
  account,
  document: {
    id: `https://id.dmsg.test/${account}`,
    protocol: 'agent-delegation/1.0',
    controllers: [],
    updated_at,
    name: 'Alice'
  }
})
const signal = () => new AbortController().signal
beforeEach(async () => {
  vi.resetModules()
  BackgroundWorker.instances = []
  vi.stubGlobal('Worker', BackgroundWorker)
  ;({ readProfile, ProfileError } = await import('./public-profile'))
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})
it('starts one lazy module worker and matches concurrent replies to their requests', async () => {
  expect(BackgroundWorker.instances).toHaveLength(0)
  const first = readProfile('alice', signal())
  const worker = BackgroundWorker.instances[0]!
  const firstId = worker.lastId
  const second = readProfile('bob', signal(), { refresh: true })
  expect(BackgroundWorker.instances).toHaveLength(1)
  expect(worker.url.pathname).toMatch(/public-profile\.worker\.ts$/)
  expect(worker.options.type).toBe('module')
  expect(worker.postMessage).toHaveBeenLastCalledWith({
    type: 'read',
    id: worker.lastId,
    input: 'bob',
    options: { refresh: true }
  })
  worker.reply({ id: worker.lastId, result: page('b') })
  worker.reply({ id: firstId, result: page('a') })
  expect(await first).toEqual(page('a'))
  expect(await second).toEqual(page('b'))
})
it('cancels navigation immediately and ignores a late reply without cancelling the next read', async () => {
  const controller = new AbortController()
  const first = readProfile('alice', controller.signal)
  const rejected = expect(first).rejects.toBeInstanceOf(ProfileError)
  const worker = BackgroundWorker.instances[0]!
  const oldId = worker.lastId
  controller.abort()
  await rejected
  expect(worker.postMessage).toHaveBeenLastCalledWith({
    type: 'cancel',
    id: oldId
  })
  const next = readProfile('bob', signal())
  worker.reply({ id: oldId, result: page('b', 100) })
  worker.reply({ id: worker.lastId, result: page('b', 1) })
  expect(await next).toEqual(page('b', 1))
})
it('does not start a worker for an already cancelled request', async () => {
  const controller = new AbortController()
  controller.abort()
  await expect(readProfile('alice', controller.signal)).rejects.toMatchObject({
    reason: 'unavailable'
  })
  expect(BackgroundWorker.instances).toHaveLength(0)
})
it('propagates verification failures without returning any profile', async () => {
  const result = readProfile('alice', signal())
  const rejected = expect(result).rejects.toMatchObject({
    reason: 'verification_failed'
  })
  const worker = BackgroundWorker.instances[0]!
  worker.reply({ id: worker.lastId, error: 'verification_failed' })
  await rejected
})
it('fails outstanding requests if a worker crashes, retries in a new worker, and retains rollback protection', async () => {
  const verified = readProfile('alice', signal())
  const worker = BackgroundWorker.instances[0]!
  worker.reply({ id: worker.lastId, result: page('a', 2) })
  await verified
  const interrupted = readProfile('alice', signal())
  const rejected = expect(interrupted).rejects.toMatchObject({
    reason: 'unavailable'
  })
  worker.onerror!()
  await rejected
  expect(worker.terminate).toHaveBeenCalledTimes(1)
  const retry = readProfile('alice', signal(), { refresh: true })
  const failedRollback = expect(retry).rejects.toMatchObject({
    reason: 'verification_failed'
  })
  const replacement = BackgroundWorker.instances[1]!
  replacement.reply({ id: replacement.lastId, result: page('a', 1) })
  await failedRollback
})
it('bounds worker startup time and cancels a stalled request', async () => {
  vi.useFakeTimers()
  const result = readProfile('alice', signal())
  const rejected = expect(result).rejects.toMatchObject({
    reason: 'unavailable'
  })
  const worker = BackgroundWorker.instances[0]!
  const id = worker.lastId
  await vi.advanceTimersByTimeAsync(25_000)
  await rejected
  expect(worker.postMessage).toHaveBeenLastCalledWith({ type: 'cancel', id })
})
