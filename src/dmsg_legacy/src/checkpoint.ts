import { openDB, type IDBPDatabase } from 'idb'
import { archiveSchema } from './archive'
import { pack, unpack, requireLegacy, digest, MAX_ARCHIVE, type Inventory } from './base'
import type { ReaderSource } from './reader'

export interface ReaderCheckpoint {
  inventory: Inventory
  completed: string[]
}
/** Dedicated migration cache; never opens or changes ICPanda_* stores. Only
 * ciphertext observations are checkpointed, never passwords or unlocked keys. */
export class LegacyCheckpointStore {
  private constructor(
    private db: IDBPDatabase,
    private key: CryptoKey,
    readonly id: string
  ) {}
  static async open(source: ReaderSource) {
    const db = await openDB('dmsg-legacy-export/1', 1, {
      upgrade(db) {
        db.createObjectStore('state')
      }
    })
    let key = (await db.get('state', 'key')) as CryptoKey | undefined
    if (!key) {
      const candidate = await crypto.subtle.generateKey(
        { name: 'AES-GCM', length: 256 },
        false,
        ['encrypt', 'decrypt']
      )
      const tx = db.transaction('state', 'readwrite')
      key = (await tx.store.get('key')) as CryptoKey | undefined
      if (!key) {
        key = candidate
        await tx.store.put(key, 'key')
      }
      await tx.done
    }
    return new LegacyCheckpointStore(
      db,
      key,
      digest(pack(['dmsg-legacy-checkpoint/1', source]))
    )
  }
  async load(): Promise<ReaderCheckpoint | null> {
    const stored = await this.db.get('state', this.id)
    if (!stored) return null
    requireLegacy(
      stored.iv instanceof Uint8Array &&
        stored.iv.length === 12 &&
        stored.bytes instanceof Uint8Array &&
        stored.bytes.length <= MAX_ARCHIVE + 16,
      'corrupt',
      'Invalid migration checkpoint'
    )
    const plain = new Uint8Array(
      await crypto.subtle.decrypt(
        { name: 'AES-GCM', iv: stored.iv, additionalData: new TextEncoder().encode(this.id) },
        this.key,
        stored.bytes
      )
    )
    try {
      const value = unpack(plain) as ReaderCheckpoint
      const inventory = archiveSchema.shape.inventory.parse(value.inventory)
      requireLegacy(
        Array.isArray(value.completed) &&
          value.completed.length <= 100000 &&
          value.completed.every((v) => typeof v === 'string' && v.length < 256),
        'corrupt',
        'Invalid migration cursors'
      )
      requireLegacy(
        inventory.objects.every((o) => digest(o.bytes) === o.digest),
        'corrupt',
        'Checkpoint digest mismatch'
      )
      return { inventory, completed: value.completed }
    } finally {
      plain.fill(0)
    }
  }
  async save(value: ReaderCheckpoint) {
    const plain = pack(value),
      iv = crypto.getRandomValues(new Uint8Array(12))
    requireLegacy(
      plain.length <= MAX_ARCHIVE,
      'limit',
      'Migration checkpoint capacity exceeded'
    )
    try {
      const bytes = new Uint8Array(
        await crypto.subtle.encrypt(
          { name: 'AES-GCM', iv, additionalData: new TextEncoder().encode(this.id) },
          this.key,
          new Uint8Array(plain)
        )
      )
      await this.db.put('state', { iv, bytes }, this.id)
    } finally {
      plain.fill(0)
    }
  }
  clear() {
    return this.db.delete('state', this.id)
  }
  close() {
    this.db.close()
  }
}
