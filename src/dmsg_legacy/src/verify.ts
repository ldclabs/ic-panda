import { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'
import { binary, digest, LegacyError, pack, readObservation, type Gap } from './base'
import { type LegacyArchive, validateArchive } from './archive'
import { decodeLegacyMessage, legacyDecrypt } from './crypto'
import { errorCode } from './reader'
import { Principal } from '@icp-sdk/core/principal'

/** Recompute content evidence offline. Exporter-supplied check labels are not
 * trusted as successful decryption, historical authorship, or source finality. */
export async function verifyArchiveContent(
  value: LegacyArchive,
  onObject?: (index: number, checked: boolean, error: Gap | null) => Promise<void>
) {
  const archive = validateArchive(value),
    gaps: Gap[] = [...archive.inventory.gaps]
  const checks: LegacyArchive['checks'] = []
  const messages: {
    source: string
    text: string
    deleted: boolean
    system: boolean
    author: string
    time: string
    file: { source: string; name: string; size: number; mime: string } | null
  }[] = []
  for (const [index, object] of archive.inventory.objects.entries()) {
    if (!['message', 'file', 'avatar'].includes(object.kind)) {
      await onObject?.(index, true, null)
      continue
    }
    let failed: Gap | null = null
    try {
      const record = readObservation(object.bytes)
      if (object.kind === 'avatar') {
        const bytes = binary(record.bytes)
        if (
          bytes.length > 2 * 1024 * 1024 ||
          digest(bytes) !== record.digest ||
          Number(record.info.size) !== bytes.length
        )
          throw new LegacyError('corrupt', 'Avatar length or digest mismatch')
        checks.push({
          source: object.key,
          digest: object.digest,
          result: 'source_plaintext',
          plaintextDigest: digest(bytes)
        })
        await onObject?.(index, true, null)
        continue
      }
      const channel =
        object.kind === 'file' ? record.channel : object.key.split('/message/')[0]
      const key = archive.keys.find((k) => k.source === channel && k.purpose === 'channel_dek')
      const dek = key ? AesGcmKey.fromBytes(key.coseKey) : null
      if (object.kind === 'message') {
        const decoded = await decodeLegacyMessage(dek, {
          kind: record.kind,
          payload: binary(record.payload)
        })
        checks.push({
          source: object.key,
          digest: object.digest,
          result: decoded.deleted
            ? 'deleted'
            : record.kind === 1
              ? 'source_plaintext'
              : 'aead_verified',
          plaintextDigest: decoded.deleted ? null : digest(pack(decoded))
        })
        messages.push({
          source: object.key,
          text: decoded.text,
          deleted: decoded.deleted,
          system: record.kind === 1,
          author: record.created_by?.toText() ?? 'unknown',
          time: String(record.created_at ?? ''),
          file: decoded.file
            ? {
                source: `${Principal.fromUint8Array(decoded.file.canister).toText()}/file/${decoded.file.id}`,
                name: decoded.file.name,
                size: decoded.file.size,
                mime: decoded.file.type
              }
            : null
        })
      } else {
        if (record.complete === false)
          throw new LegacyError(
            'missing',
            'Incomplete upload: ciphertext fragments preserved without a complete COSE object'
          )
        if (!dek) throw new LegacyError('missing_key', 'Historical channel DEK missing')
        const plain = await legacyDecrypt(dek, binary(record.ciphertext))
        checks.push({
          source: object.key,
          digest: object.digest,
          result: 'aead_verified',
          plaintextDigest: digest(plain)
        })
        plain.fill(0)
      }
    } catch (cause) {
      failed = {
        source: object.key,
        code: errorCode(cause),
        detail:
          cause instanceof LegacyError
            ? cause.message
            : 'Historical object could not be verified'
      }
      gaps.push(failed)
    }
    await onObject?.(index, failed === null, failed)
  }
  for (const claimed of archive.checks) {
    const actual = checks.find((c) => c.source === claimed.source)
    if (
      actual &&
      (actual.result !== claimed.result || actual.plaintextDigest !== claimed.plaintextDigest)
    )
      gaps.push({
        source: claimed.source,
        code: 'corrupt',
        detail: 'Exporter verification disagrees with offline verification'
      })
  }
  return {
    checks,
    gaps,
    messages,
    partial: gaps.length > 0,
    snapshot: archive.inventory.snapshot,
    principal: archive.inventory.principal
  }
}

export async function openArchiveFile(value: LegacyArchive, source: string) {
  const archive = validateArchive(value),
    object = archive.inventory.objects.find((o) => o.key === source && o.kind === 'file')
  if (!object) throw new LegacyError('missing', 'Historical file is missing')
  const record = readObservation(object.bytes)
  if (record.complete === false)
    throw new LegacyError(
      'missing',
      'Incomplete upload cannot be decrypted as a complete file'
    )
  const material = archive.keys.find(
    (k) => k.source === record.channel && k.purpose === 'channel_dek'
  )
  if (!material) throw new LegacyError('missing_key', 'Historical channel DEK missing')
  const key = AesGcmKey.fromBytes(material.coseKey)
  let name = String(record.info?.name ?? 'legacy-file')
  for (const message of archive.inventory.objects.filter(
    (o) => o.kind === 'message' && o.key.startsWith(`${record.channel}/message/`)
  )) {
    const data = readObservation(message.bytes)
    try {
      const file = (
        await decodeLegacyMessage(key, { kind: data.kind, payload: binary(data.payload) })
      ).file
      if (
        file &&
        `${Principal.fromUint8Array(file.canister).toText()}/file/${file.id}` === source
      ) {
        name = file.name
        break
      }
    } catch {
      /* Other damaged messages do not replace this file's verified bytes. */
    }
  }
  return { name, bytes: await legacyDecrypt(key, binary(record.ciphertext)) }
}
