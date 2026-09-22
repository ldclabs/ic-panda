import { z } from 'zod'
import { digest, MAX_ARCHIVE, pack, principal, requireLegacy, unpack } from './base'

const bytes = z.custom<Uint8Array>((v) => v instanceof Uint8Array)
const text = z.string().max(2048)
const hash = z.string().regex(/^[0-9a-f]{64}$/)
const integer = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER)
const identity = z
  .string()
  .max(64)
  .refine((v) => {
    try {
      return principal(v) === v
    } catch {
      return false
    }
  })
const failure = z.enum([
  'network',
  'permission',
  'missing',
  'missing_key',
  'version',
  'corrupt',
  'limit',
  'cancelled'
])
export const archiveSchema = z
  .object({
    format: z.literal('dmsg-legacy-archive/1'),
    inventory: z
      .object({
        format: z.literal('dmsg-legacy-inventory/1'),
        principal: identity,
        messageCanister: identity,
        mode: z.enum(['Local', 'ECDH', 'VetKey']),
        snapshot: z.literal('pre_migration'),
        objects: z
          .array(
            z
              .object({
                key: text,
                kind: z.enum([
                  'identity',
                  'profile',
                  'channel',
                  'message',
                  'file',
                  'local',
                  'setting',
                  'entitlement',
                  'avatar'
                ]),
                bytes: bytes,
                digest: hash,
                trust: z.enum(['query_observation', 'local_cache', 'derived_archive']),
                observedAt: integer
              })
              .strict()
          )
          .max(100000),
        gaps: z
          .array(z.object({ source: text, code: failure, detail: text }).strict())
          .max(100000),
        calls: z
          .array(
            z
              .object({
                canister: identity,
                method: z.string().max(80),
                at: integer,
                outcome: z.union([failure, z.literal('ok')])
              })
              .strict()
          )
          .max(100000)
      })
      .strict(),
    // These secrets only exist inside the HPKE transfer and encrypted vault file.
    keys: z
      .array(
        z
          .object({
            source: text,
            principal: identity,
            purpose: z.enum(['master', 'channel_dek', 'channel_kek', 'static_ecdh']),
            coseKey: bytes,
            parameters: bytes
          })
          .strict()
      )
      .max(10000),
    checks: z
      .array(
        z
          .object({
            source: text,
            digest: hash,
            result: z.enum([
              'aead_verified',
              'source_plaintext',
              'deleted',
              'ciphertext_only'
            ]),
            plaintextDigest: hash.nullable()
          })
          .strict()
      )
      .max(100000),
    createdAt: integer
  })
  .strict()
export type LegacyArchive = z.infer<typeof archiveSchema>
export function validateArchive(value: unknown): LegacyArchive {
  const archive = archiveSchema.parse(value)
  const objects = new Map<string, string>()
  for (const object of archive.inventory.objects) {
    requireLegacy(
      !objects.has(object.key) &&
        object.bytes.length <= MAX_ARCHIVE &&
        digest(object.bytes) === object.digest,
      'corrupt',
      'Duplicate or substituted legacy source object'
    )
    objects.set(object.key, object.digest)
  }
  const keys = new Set<string>()
  for (const key of archive.keys) {
    const id = `${key.source}/${key.purpose}`
    requireLegacy(
      key.principal === archive.inventory.principal &&
        key.coseKey.length > 0 &&
        key.coseKey.length <= 2048 &&
        key.parameters.length <= 8192 &&
        !keys.has(id),
      'permission',
      'Historical key identity, bounds or duplication mismatch'
    )
    keys.add(id)
  }
  const checks = new Set<string>()
  for (const check of archive.checks) {
    requireLegacy(
      objects.get(check.source) === check.digest && !checks.has(check.source),
      'corrupt',
      'Verification points to a different source object'
    )
    checks.add(check.source)
  }
  return archive
}
export function encodeArchive(archive: LegacyArchive) {
  const bytes = pack(validateArchive(archive))
  requireLegacy(
    bytes.length <= MAX_ARCHIVE,
    'limit',
    'Legacy archive exceeds 256 MiB including metadata'
  )
  return bytes
}
export function decodeArchive(bytes: Uint8Array) {
  return validateArchive(unpack(bytes))
}

/** Stable job identity excludes collection timestamps and transport nonces. */
export function archiveContentId(value: LegacyArchive) {
  const archive = validateArchive(value)
  const order = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0)
  return digest(
    pack({
      format: 'dmsg-legacy-content/1',
      principal: archive.inventory.principal,
      source: archive.inventory.messageCanister,
      mode: archive.inventory.mode,
      snapshot: archive.inventory.snapshot,
      objects: archive.inventory.objects
        .map((o) => [o.key, o.kind, o.digest, o.trust])
        .sort((a, b) => order(String(a[0]), String(b[0]))),
      keys: archive.keys
        .map((k) => [k.source, k.purpose, digest(k.coseKey), digest(k.parameters)])
        .sort((a, b) => order(a.join(':'), b.join(':'))),
      checks: archive.checks
        .map((c) => [c.source, c.digest, c.result, c.plaintextDigest])
        .sort((a, b) => order(String(a[0]), String(b[0]))),
      gaps: archive.inventory.gaps
        .map((g) => [g.source, g.code, g.detail])
        .sort((a, b) => order(a.join(':'), b.join(':')))
    })
  )
}
