import { bytesToBase64Url, encodeCBOR } from '@ldclabs/cose-ts/utils'
import { describe, expect, it } from 'vitest'
import { decodePrize, type Prize } from './prize'

describe('prize code protocol', () => {
  it('accepts the canonical copied prefix and raw code regardless of UI language', () => {
    const prize: Prize = [123, 1700000000, 60, 100, 10]
    const code = bytesToBase64Url(
      encodeCBOR([encodeCBOR(prize), new Uint8Array(32)])
    )
    expect(decodePrize(code)).toEqual(prize)
    expect(decodePrize('PRIZE:' + code)).toEqual(prize)
    // UI labels must never be used as protocol prefixes.
    expect(decodePrize('奖励：' + code)).toBeNull()
    expect(decodePrize('PRIX : ' + code)).toBeNull()
  })
})
