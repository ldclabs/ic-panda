import { afterEach, expect, it, vi } from 'vitest'
import {
  mergeCloudEvidence,
  type CloudSecurityEvidence
} from '../src/lib/services/cloud-security'
import { CloudClient } from '../src/lib/services/relay'
import { b64, canonical, decodeCanonical } from '../src/lib/protocol/codec'
import { xidText } from '../src/lib/protocol/identity'

afterEach(() => vi.unstubAllGlobals())
function evidence(count: number, deviceBytes = 3200): CloudSecurityEvidence {
  return {
    schema: 1,
    canister: 'aaaaa-aa',
    certificate: b64(new Uint8Array(500)),
    entries: Array.from({ length: count }, (_, i) => ({
      account_id: xidText(new Uint8Array(12).fill(i + 1)),
      value: b64(new Uint8Array(523)),
      witness: b64(new Uint8Array(771)),
      devices: b64(new Uint8Array(deviceBytes))
    }))
  }
}
it('splits large device evidence into uploads accepted by the relay byte limit', async () => {
  const input = evidence(64),
    batches = mergeCloudEvidence(
      input.entries.map((entry) => ({ ...input, entries: [entry] }))
    )
  expect(canonical(input).length).toBeGreaterThan(262144)
  expect(batches.length).toBeGreaterThan(1)
  expect(batches.flatMap((batch) => batch.entries)).toEqual(input.entries)
  const uploaded: CloudSecurityEvidence[] = []
  vi.stubGlobal(
    'fetch',
    vi.fn(async (_url: URL, init: RequestInit) => {
      const body = init.body as Uint8Array
      expect(body.length).toBeLessThanOrEqual(262144)
      uploaded.push(decodeCanonical<CloudSecurityEvidence>(body))
      return Response.json({ ok: true, data: {} })
    })
  )
  const cloud = new CloudClient({ origin: 'https://dmsg.test', environment: 'local' })
  for (const batch of batches) await cloud.publishSecurity(batch)
  expect(uploaded.flatMap((batch) => batch.entries)).toEqual(input.entries)
})
it('keeps the leaf count and certificate boundaries when byte sizes are small', () => {
  const input = evidence(65, 0),
    different = { ...evidence(1, 0), certificate: 'different-certificate' }
  const batches = mergeCloudEvidence([input, different])
  expect(batches.map((batch) => batch.entries.length)).toEqual([64, 1, 1])
  expect(batches[2]).toEqual(different)
  expect(input.entries).toHaveLength(65)
})
it('rejects an individual proof that cannot fit in a relay request', () => {
  expect(() => mergeCloudEvidence([evidence(1, 262144)])).toThrow('QUOTA_EXCEEDED')
})
