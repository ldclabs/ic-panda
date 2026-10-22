import { expect, it } from 'vitest'
import { digest, observation, pack, readObservation } from '../src'

it('encodes IndexedDB ArrayBuffer keys as the same bytes as Uint8Array keys', () => {
  const backing = new Uint8Array([99, 1, 2, 3, 88])
  const bytes = backing.subarray(1, 4)
  const buffer = backing.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength
  )
  const expected = pack(observation([bytes, 7, 11]))
  const actual = pack(observation([buffer, 7, 11]))

  expect(actual).toEqual(expected)
  expect(digest(actual)).toBe(digest(expected))
  expect(readObservation(actual)).toEqual([new Uint8Array([1, 2, 3]), 7, 11])
  expect(readObservation(pack(observation(new ArrayBuffer(0))))).toEqual(new Uint8Array())
})

it('still rejects unsupported observation types', () => {
  for (const value of [undefined, new Date(), new Set([1]), () => 1]) {
    expect(() => observation(value)).toThrow('Unsupported observation type')
  }
})
