import { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'
import { validateArchive, type LegacyArchive } from './archive'
import {
  binary,
  digest,
  observation,
  pack,
  readObservation,
  requireLegacy,
  type SourceObject
} from './base'
import { legacyDecrypt } from './crypto'
import type { SharedSource } from './shared'

/** Preserve only one old channel's content and its two historical channel keys.
 * No master key, static ECDH private key, profile or other channel is included. */
export async function sharedHistoryArchive(
  value: LegacyArchive,
  source: SharedSource
): Promise<LegacyArchive> {
  const archive = validateArchive(value),
    key = `${source.source}/channel/${source.channel}`
  const original = archive.inventory.objects.find((o) => o.kind === 'channel' && o.key === key)
  requireLegacy(original, 'missing', 'Original channel record is required')
  const channel = readObservation(original.bytes)
  requireLegacy(
    digest(binary(channel.dek)) === source.dekDigest,
    'version',
    'Historical channel key differs from the frozen source'
  )
  const kek = archive.keys.find((k) => k.source === key && k.purpose === 'channel_kek'),
    dek = archive.keys.find((k) => k.source === key && k.purpose === 'channel_dek')
  requireLegacy(
    kek && dek,
    'missing_key',
    'Both historical channel keys are required to verify the frozen wrapper'
  )
  const decoded = await legacyDecrypt(AesGcmKey.fromBytes(kek.coseKey), binary(channel.dek))
  try {
    requireLegacy(
      digest(AesGcmKey.fromBytes(decoded).toBytes()) ===
        digest(AesGcmKey.fromBytes(dek.coseKey).toBytes()),
      'corrupt',
      'Historical DEK does not match the frozen wrapper'
    )
  } finally {
    decoded.fill(0)
  }
  const metadata = pack(
    observation({
      id: source.channel,
      canister: channel.canister,
      dek: binary(channel.dek),
      message_start: source.messageStart,
      latest_message_id: source.messageEnd
    })
  )
  const objects: SourceObject[] = [
    {
      key,
      kind: 'channel',
      bytes: metadata,
      digest: digest(metadata),
      observedAt: original.observedAt,
      trust: 'derived_archive'
    }
  ]
  const observed = new Set<number>()
  for (const object of archive.inventory.objects) {
    if (object.kind === 'message' && object.key.startsWith(`${key}/message/`)) {
      const message = readObservation(object.bytes)
      if (message.id >= source.messageStart && message.id <= source.messageEnd) {
        objects.push(object)
        observed.add(message.id)
      }
    } else if (object.kind === 'file' && readObservation(object.bytes).channel === key)
      objects.push(object)
  }
  const included = new Set(objects.map((o) => o.key)),
    gaps = archive.inventory.gaps.filter(
      (g) => g.source.startsWith(key) || included.has(g.source)
    )
  for (let id = source.messageStart; id <= source.messageEnd; id++)
    if (!observed.has(id))
      gaps.push({
        source: `${key}/message/${id}`,
        code: 'missing',
        detail: 'Frozen message slot is not present in the supplied archive'
      })
  const result: LegacyArchive = {
    format: 'dmsg-legacy-archive/1',
    inventory: { ...archive.inventory, objects, gaps, calls: [] },
    keys: [kek, dek].map((k) => ({
      ...k,
      coseKey: new Uint8Array(k.coseKey),
      parameters: pack({ source_key: key, frozen_source_digest: source.digest })
    })),
    checks: archive.checks.filter((c) => included.has(c.source)),
    createdAt: Date.now()
  }
  validateArchive(result)
  return result
}
