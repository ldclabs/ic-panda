export type ItemKind = 'note' | 'login' | 'api' | 'key' | 'file'
export type ObjectKind =
  | 'vault'
  | 'profile'
  | 'draft'
  | 'request'
  | 'migration'
  | 'migration_part'
  | 'formal_channel'
  | 'formal_control'
  | 'formal_message'
  | 'formal_file'
  | 'commerce'
  | 'inbox'
export interface Item {
  type: ItemKind
  title: string
  tags: string[]
  body: string
  username: string
  secret: string
  url: string
  favorite: boolean
  createdAt: number
  updatedAt: number
  file?: FileManifest
  resolvedConflicts?: string[]
}
export interface FileManifest {
  id: string
  version: string
  name: string
  mime: string
  size: number
  sha256: string
  chunks: { id: string; digest: string; size: number }[]
  key: string // Inside encrypted item payload only; never runtime metadata.
}
export interface EncryptedObject {
  key: string
  id: string
  revision: string
  parent: string | null
  subjectId: string
  deviceId: string
  generation: number
  kind: ObjectKind
  ciphertext: string
  keyEnvelope: string
  digest: string
  tombstone: boolean
  conflict: boolean
  createdAt: number
}
export interface VaultEntry {
  record: EncryptedObject
  item: Item
}
export interface Profile {
  avatarFile?: string
  name: string
  bio: string
  link: string
  contact: 'closed' | 'invitation'
  publicFields: string[]
}
export interface WorkspaceMeta {
  cloudSnapshot?: {
    through: number
    evidence: string
    uploads: { plan: Record<string, unknown>; manifest: string; chunkIds: string[] }[]
    heads: [string, string][]
  }
  // Random before the workspace is bound; the canister-allocated Xid afterwards.
  subjectId: string
  account?: {
    id: string
    issuer: string
    homeUser: string
    rootDigest?: string
    rootBytesDigest?: string
    rootUploadId?: string
  }
  rootHistory?: string
  deviceId: string
  environment: string
  createdAt: number
  signingPublic: string
  hpkePublic: string
  rootGeneration: number
  registered: boolean
  /** How the local data key is unlocked: a provisional key until the user
   * home's login-gated secret wraps it. */
  unlock: 'provisional' | 'login'
  /** Platform-authenticator fast unlock, when enabled on this device. */
  prf?: { credentialId: string; enabledAt: number }
  /** Last unlock through the login path; PRF unlocks are refused a week later. */
  loginUnlockedAt?: number
}
export interface LocalEnvelope {
  id: string
  /** Plaintext provisional unlock key, present only before the workspace is bound. */
  provisional?: string
  wrappedKey: string
  /** The local data key wrapped under the PRF-derived key. */
  prfWrappedKey?: string
  privateBundle: string
}
export interface OutboxJob {
  id: string
  objectKey: string
  frame: string
  digest: string
  state: 'local' | 'queued' | 'sending' | 'stored' | 'blocked' | 'unknown'
  error?: string
  cloudReceipt?: { revision_id: string; head: string; conflict: boolean; tombstone: boolean }
}
export interface Chunk {
  id: string
  ciphertext: string
  digest: string
}
export interface Lease {
  id: 'crypto-owner'
  owner: string
  fence: number
  expiresAt: number
  documentId?: string
}
export interface ViewData {
  meta: WorkspaceMeta | null
  entries: VaultEntry[]
  profile: Profile | null
  outbox: Pick<OutboxJob, 'id' | 'state' | 'error'>[]
  conflicts: VaultEntry[]
  imports: { id: string; name: string; size: number; completed: number; total: number }[]
}
