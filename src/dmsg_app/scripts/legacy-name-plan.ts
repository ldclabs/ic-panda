import { IDL } from '@icp-sdk/core/candid'
import { idlFactory } from '../src/lib/canisters/generated/handle/index.js'
import { prepareLegacyNames } from '../src/lib/services/legacy-names'
import { hex } from '../src/lib/protocol/codec'

/** Offline, unsigned controller call material. Does not connect to any service. */
export async function nameImportPlan(input: Parameters<typeof prepareLegacyNames>[0]) {
  const prepared = await prepareLegacyNames(input)
  const service = idlFactory({ IDL }) as IDL.ServiceClass
  const call = (method: string, args: unknown[]) => ({
    method,
    candid_hex: hex(
      new Uint8Array(
        IDL.encode(service._fields.find(([name]) => name === method)![1].argTypes, args)
      )
    )
  })
  const calls = [call('begin_legacy_snapshot', [prepared.snapshot])]
  for (let i = 0; i < prepared.entries.length; i += 256)
    calls.push(
      call('import_legacy_handles', [
        prepared.snapshot.snapshot_id,
        prepared.entries.slice(i, i + 256)
      ])
    )
  calls.push(call('seal_legacy_snapshot', []))
  return {
    format: 'dmsg-legacy-name-plan/1',
    snapshot: hex(Uint8Array.from(prepared.snapshot.snapshot_id)),
    source: input.source,
    identity: input.identity,
    cutover: hex(input.cutover),
    count: prepared.entries.length,
    quarantined: prepared.quarantined,
    calls
  }
}
