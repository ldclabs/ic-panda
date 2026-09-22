import {
  Certificate,
  Cbor,
  lookup_path,
  lookupResultToBuffer,
  reconstruct,
  type HashTree,
  type HttpAgent
} from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import type { CertifiedBatch } from '../canisters/generated/user'
import { bytes, equal } from '../protocol/codec'
import { ensure } from '../errors'

export async function certifiedLeaf(
  batch: CertifiedBatch,
  agent: HttpAgent,
  source: string,
  key: Uint8Array,
  now = Date.now()
) {
  const canister = Principal.fromText(source)
  ensure(
    batch.schema === 1 &&
      batch.canister.toText() === source &&
      batch.entries.length <= 64 &&
      agent.rootKey &&
      batch.certificate.length <= 65536,
    'INTEGRITY_FAILED'
  )
  const certificate = await Certificate.create({
    certificate: bytes(Uint8Array.from(batch.certificate)),
    rootKey: agent.rootKey,
    principal: { canisterId: canister },
    disableTimeVerification: true
  })
  const time = lookupResultToBuffer(certificate.lookup_path(['time']))
  ensure(time && time.length > 0 && time.length <= 10, 'INTEGRITY_FAILED')
  let nanos = 0n
  for (let i = 0; i < time.length; i++) {
    ensure(
      i === time.length - 1 ? !(time[i] & 128) : Boolean(time[i] & 128),
      'INTEGRITY_FAILED'
    )
    nanos |= BigInt(time[i] & 127) << BigInt(i * 7)
  }
  const at = Number(nanos / 1000000n)
  ensure(Number.isSafeInteger(at) && at <= now && now < at + 60000, 'POLICY_STALE')
  const entries = batch.entries.filter((e) => equal(Uint8Array.from(e.key), key))
  ensure(
    entries.length === 1 &&
      entries[0].value.length <= 1 &&
      entries[0].witness.length <= 262144,
    'INTEGRITY_FAILED'
  )
  const root = lookupResultToBuffer(
    certificate.lookup_path(['canister', canister.toUint8Array(), 'certified_data'])
  )
  const tree = Cbor.decode<HashTree>(Uint8Array.from(entries[0].witness)),
    lookup = lookup_path([key], tree),
    value = lookupResultToBuffer(lookup) ?? null
  ensure(root && equal(root, await reconstruct(tree)), 'INTEGRITY_FAILED')
  if (entries[0].value.length)
    ensure(value && equal(value, Uint8Array.from(entries[0].value[0]!)), 'INTEGRITY_FAILED')
  else ensure(lookup.status === 'Absent', 'INTEGRITY_FAILED')
  return { value, certifiedAt: at }
}

export async function certifiedValue(...args: Parameters<typeof certifiedLeaf>) {
  const result = await certifiedLeaf(...args)
  ensure(result.value, 'INTEGRITY_FAILED')
  return { ...result, value: result.value }
}
