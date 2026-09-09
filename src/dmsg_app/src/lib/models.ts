export type ItemKind = 'note' | 'login' | 'api' | 'key' | 'file'
export type ObjectKind =
  'vault' | 'channel' | 'message' | 'profile' | 'draft' | 'request' | 'migration'
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
export interface Channel {
  name: string
  type: 'direct' | 'collaboration' | 'distribution'
  members: {
    subject: string
    role: 'owner' | 'admin' | 'publisher' | 'member'
    accepted: boolean
  }[]
  epoch: number
  state: 'draft' | 'active' | 'rotation_required' | 'archived'
  controlHead: string | null
  key: string
  createdAt: number
}
export interface Message {
  channelId: string
  text: string
  createdAt: number
  replyTo?: string
  epoch: number
  state: 'draft' | 'queued' | 'stored' | 'received' | 'read'
  signature?: string
}
export interface Profile {
  name: string
  bio: string
  link: string
  contact: 'closed' | 'invitation'
  publicFields: string[]
}
export interface WorkspaceMeta {
  // Local vault identity used in encryption AAD; never an on-chain AccountId.
  subjectId: string
  account?: { id: string; issuer: string; homeUser: string }
  deviceId: string
  environment: string
  createdAt: number
  signingPublic: string
  hpkePublic: string
  transportPublic: string
  recoveryPublic: string
  recoverySigningPublic: string
  recoveryGeneration: number
  rootGeneration: number
  recoveryChecked: boolean
  lastBackupAt: number | null
  lastBackupCount: number
  registered: boolean
  recoveryEnvelope: { enc: string; ciphertext: string }
}
export interface KdfParams {
  algorithm: 'argon2id'
  version: 1
  memory: number
  iterations: number
  parallelism: number
  salt: string
}
export interface LocalEnvelope {
  id: string
  kdf: KdfParams
  wrappedKey: string
  privateBundle: string
}
export interface OutboxJob {
  id: string
  objectKey: string
  frame: string
  digest: string
  state: 'local' | 'queued' | 'sending' | 'stored' | 'blocked' | 'unknown'
  attempt: number
  nextAttempt: number
  error?: string
  receipt?: { sequence: number; digest: string }
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
export interface RecoveryArchive {
  format: 'dmsg-backup/1'
  meta: WorkspaceMeta
  createdAt: number
  scope: 'local-inclusive' | 'partial'
  objects: EncryptedObject[]
  chunks: Chunk[]
  missing: string[]
  manifestDigest: string
  authentication: string
}
export interface ViewData {
  meta: WorkspaceMeta | null
  entries: VaultEntry[]
  channels: { record: EncryptedObject; channel: Channel }[]
  messages: { record: EncryptedObject; message: Message }[]
  profile: Profile | null
  outbox: OutboxJob[]
  conflicts: VaultEntry[]
  imports: { id: string; name: string; size: number; completed: number; total: number }[]
}
