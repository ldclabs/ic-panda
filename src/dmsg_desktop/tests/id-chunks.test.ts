import assert from 'node:assert/strict'
import { test } from 'node:test'
import { idChunks } from '../src/renderer/components/id-chunks.ts'

test('a long ID reads in groups of four, and joins back to itself', () => {
  const id = 'ab'.repeat(32)
  const parts = idChunks(id)
  assert.equal(parts.length, 16)
  assert.ok(parts.every((part) => part.length === 4))
  assert.equal(parts.join(''), id)
  assert.deepEqual(idChunks('0123456789abcdefgh'), ['0123', '4567', '89ab', 'cdef', 'gh'])
})

test('short IDs and principals stay whole', () => {
  assert.deepEqual(idChunks('ab12cd34'), ['ab12cd34'])
  assert.deepEqual(idChunks('rdmx6-jaaaa-aaaaa-aaadq-cai'), ['rdmx6-jaaaa-aaaaa-aaadq-cai'])
})
