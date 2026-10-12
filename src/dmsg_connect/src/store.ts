/**
 * Where the bridge keeps its pairings and operations. The page uses
 * IndexedDB `dmsg-connect:<environment>`: keys stay CryptoKey objects
 * (structured clones of non-extractable keys), never bytes. The headless
 * bridge keeps them in memory.
 */
import type { Environment } from './protocol.ts'

/** A pairing as stored, keyed by its account (hex). */
export interface PairingRecord {
  account: Uint8Array
  id: Uint8Array
  key: CryptoKeyPair
  pub: Uint8Array
  label: string
  state: 'candidate' | 'active'
  home?: string
  grant?: string
  expiresAt?: number
}

/** An operation as stored, keyed by `origin \n appId \n operationId`; its pairing by account (hex). */
export interface OperationRecord {
  key: string
  origin: string
  command: Record<string, string | null>
  digest: Uint8Array
  kind: string
  pairing: string
  requestId: Uint8Array
  resultKey: CryptoKeyPair
  seen: number
  device: Uint8Array | null
  expiresAt: number
  delivered: string | null
  last: Record<string, unknown> | null
  updatedAt: number
}

export interface BridgeStore {
  load(): Promise<{ pairings: PairingRecord[]; operations: OperationRecord[] }>
  putPairing(record: PairingRecord): Promise<void>
  putOperation(record: OperationRecord): Promise<void>
  deleteOperation(key: string): Promise<void>
  /** Small values: the last click in the popup (Safari, connect-v1 §4.1). */
  meta(name: string): Promise<unknown>
  setMeta(name: string, value: unknown): Promise<void>
}

export class MemoryStore implements BridgeStore {
  private pairings = new Map<string, PairingRecord>()
  private operations = new Map<string, OperationRecord>()
  private values = new Map<string, unknown>()

  async load() {
    return { pairings: [...this.pairings.values()], operations: [...this.operations.values()] }
  }

  async putPairing(record: PairingRecord) {
    this.pairings.set(hexKey(record.account), record)
  }

  async putOperation(record: OperationRecord) {
    this.operations.set(record.key, record)
  }

  async deleteOperation(key: string) {
    this.operations.delete(key)
  }

  async meta(name: string) {
    return this.values.get(name)
  }

  async setMeta(name: string, value: unknown) {
    this.values.set(name, value)
  }
}

const hexKey = (bytes: Uint8Array) => Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')

/** IndexedDB with three stores: `pairings` (by account hex), `operations` (by key) and `meta`. */
export class IdbStore implements BridgeStore {
  private db: Promise<IDBDatabase>

  constructor(environment: Environment) {
    this.db = new Promise((resolve, reject) => {
      const request = indexedDB.open(`dmsg-connect:${environment}`, 1)
      request.onupgradeneeded = () => {
        request.result.createObjectStore('pairings')
        request.result.createObjectStore('operations')
        request.result.createObjectStore('meta')
      }
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
  }

  private async run<T>(store: string, mode: IDBTransactionMode, fn: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
    const db = await this.db
    return new Promise((resolve, reject) => {
      const tx = db.transaction(store, mode)
      const request = fn(tx.objectStore(store))
      tx.oncomplete = () => resolve(request.result)
      tx.onerror = tx.onabort = () => reject(tx.error)
    })
  }

  async load() {
    const [pairings, operations] = await Promise.all([
      this.run<PairingRecord[]>('pairings', 'readonly', (s) => s.getAll()),
      this.run<OperationRecord[]>('operations', 'readonly', (s) => s.getAll())
    ])
    return { pairings, operations }
  }

  async putPairing(record: PairingRecord) {
    await this.run('pairings', 'readwrite', (s) => s.put(record, hexKey(record.account)))
  }

  async putOperation(record: OperationRecord) {
    await this.run('operations', 'readwrite', (s) => s.put(record, record.key))
  }

  async deleteOperation(key: string) {
    await this.run('operations', 'readwrite', (s) => s.delete(key))
  }

  async meta(name: string) {
    return this.run('meta', 'readonly', (s) => s.get(name))
  }

  async setMeta(name: string, value: unknown) {
    await this.run('meta', 'readwrite', (s) => s.put(value, name))
  }
}
