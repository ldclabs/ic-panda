import { expect, it } from 'vitest'
import { IDL } from '@icp-sdk/core/candid'
import { Tagged } from 'cborg'
import { candidValue } from '../src/lib/protocol/account'
import { canonical, decodeCanonical, hex } from '../src/lib/protocol/codec'
import { paymentAmount } from '../src/lib/protocol/payment'

it('preserves certified u128 amounts above the CBOR uint64 range without rounding', () => {
  const maximum = (1n << 128n) - 1n
  const bytes = canonical(candidValue(IDL.Nat, maximum))
  expect(hex(bytes)).toBe('c250' + 'ff'.repeat(16))
  expect(paymentAmount(decodeCanonical(bytes))).toBe(maximum)
  expect(paymentAmount(decodeCanonical(canonical(0xffffffffffffffffn)))).toBe(
    0xffffffffffffffffn
  )
  expect(() => paymentAmount(Number.MAX_SAFE_INTEGER + 1)).toThrow()
  expect(() => paymentAmount(new Tagged(2, new Uint8Array(17).fill(1)))).toThrow()
  expect(() => paymentAmount(-1)).toThrow()
})
