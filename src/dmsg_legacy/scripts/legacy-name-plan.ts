import { IDL } from '@icp-sdk/core/candid'
import { hex } from 'dmsg-sdk/codec'
import { LegacyReservationType, LegacySnapshotType, prepareLegacyNames } from './legacy-names'

// The dmsg_handle methods called, with their arguments as in src/dmsg_handle/dmsg_handle.did.
const methods: Record<string, IDL.Type[]> = {
  begin_legacy_snapshot: [LegacySnapshotType],
  import_legacy_handles: [IDL.Vec(IDL.Nat8), IDL.Nat64, IDL.Vec(LegacyReservationType)],
  seal_legacy_snapshot: []
}

/** Offline, unsigned controller call material. Does not connect to any service. */
export async function nameImportPlan(input: Parameters<typeof prepareLegacyNames>[0]) {
  const prepared = await prepareLegacyNames(input)
  const call = (method: string, args: unknown[]) => ({
    method,
    candid_hex: hex(new Uint8Array(IDL.encode(methods[method], args)))
  })
  const calls = [call('begin_legacy_snapshot', [prepared.snapshot])]
  // Each batch carries its snapshot position, so one executed out of order is rejected.
  for (let i = 0; i < prepared.entries.length; i += 256)
    calls.push(
      call('import_legacy_handles', [
        prepared.snapshot.snapshot_id,
        BigInt(i),
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
