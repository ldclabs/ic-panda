import { expect, it } from 'vitest'
import {
  reconcileOutbox,
  type OutboxStore,
  type OutboxTransport
} from '../src/lib/services/outbox'
import type { OutboxJob } from '../src/lib/models'
const initial = (): OutboxJob => ({
  id: 'operation',
  objectKey: 'object:version',
  frame: 'immutable-ciphertext',
  digest: 'digest',
  state: 'queued',
  attempt: 0,
  nextAttempt: 0
})
it('reconciles lost ACK after restart without republishing or changing encrypted bytes', async () => {
  let job = initial(),
    sends = 0
  const store: OutboxStore = {
    list: async () => [job],
    put: async (value) => {
      job = { ...value }
    }
  }
  const transport: OutboxTransport = {
    status: async () => ({ sequence: 1, digest: 'digest' }),
    publish: async (sending) => {
      expect(job.state).toBe('sending')
      expect(sending.frame).toBe('immutable-ciphertext')
      sends++
      throw new Error('ACK lost after durable commit')
    }
  }
  await reconcileOutbox(store, transport, 1000)
  expect(job.state).toBe('unknown')
  await reconcileOutbox(store, transport, 100000)
  expect(job.state).toBe('stored')
  expect(sends).toBe(1)
})
it('does not convert unknown status, damaged ACKs or local-only objects into successful delivery', async () => {
  let job = { ...initial(), state: 'unknown' as const } as OutboxJob,
    sends = 0
  const store: OutboxStore = {
    list: async () => [job],
    put: async (value) => {
      job = { ...value }
    }
  }
  const transport: OutboxTransport = {
    status: async () => 'unknown',
    publish: async () => {
      sends++
      return { sequence: 1, digest: 'wrong' }
    }
  }
  await reconcileOutbox(store, transport, 1)
  expect(job.state).toBe('unknown')
  expect(sends).toBe(0)
  job = initial()
  await reconcileOutbox(store, transport, 1)
  expect(job.state).toBe('blocked')
  job = { ...initial(), state: 'local' }
  await reconcileOutbox(store, transport, 1)
  expect(job.state).toBe('local')
  expect(sends).toBe(1)
})
