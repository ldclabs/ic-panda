import { z } from 'zod'
const hash = z.string().regex(/^[0-9a-f]{64}$/),
  uint = z.number().int().nonnegative().safe()
export const legacyHistoryScopeSchema = z.strictObject({
  format: z.literal('dmsg-legacy-history-grant/1'),
  directory_key: hash,
  proposal_digest: hash,
  source_digest: hash,
  grant_id: hash,
  channel_id: hash,
  epoch: uint.min(1),
  control_head: hash,
  member: z.string().max(64),
  account: z.string().regex(/^[0-9a-v]{19}[0g]$/),
  device: hash,
  security_epoch: uint,
  recovery_generation: uint.min(1),
  archive_digest: hash,
  principal: z.string().max(64)
})
export type LegacyHistoryScope = z.infer<typeof legacyHistoryScopeSchema>
export interface LegacyHistoryGrant {
  scope: LegacyHistoryScope
  manifest: string
  recipients: { recipient: string; envelope: { enc: string; ciphertext: string } }[]
  parts: { upload_id: string; manifest_digest: string }[]
}
