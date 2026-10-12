import { describe, expect, it } from 'vitest'
import { Principal } from '@icp-sdk/core/principal'
import { hex } from 'dmsg-sdk/codec'
import { legacyEntryDigest, type LegacyReservation } from '../scripts/legacy-names'

describe('name import plan', () => {
  it('folds entries into the digest dmsg_handle commits', () => {
    // dmsg_protocol's handle tests pin the same value.
    const entries: LegacyReservation[] = [
      {
        handle: 'alice',
        legacy_owner: Principal.fromUint8Array(Uint8Array.of(1)),
        legacy_name_principal: [Principal.fromUint8Array(Uint8Array.of(2))],
        frozen_admins: [Principal.fromUint8Array(Uint8Array.of(3))],
        quarantined: false
      },
      {
        handle: 'bob',
        legacy_owner: Principal.fromUint8Array(Uint8Array.of(4)),
        legacy_name_principal: [],
        frozen_admins: [],
        quarantined: true
      }
    ]
    const rolling = entries.reduce(
      (rolling, entry) => Uint8Array.from(legacyEntryDigest(rolling, entry)),
      new Uint8Array(32)
    )
    expect(hex(rolling)).toBe('3ace7fa09259fd6011ec83d07aa489eba29d086861057865f57d0a1673da486a')
  })
})
