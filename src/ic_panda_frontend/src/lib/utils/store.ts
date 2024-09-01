import { openDB, type IDBPDatabase } from 'idb'

export type IDBValidKey = string | number | Date | BufferSource | IDBValidKey[]

export class KVStore {
  private static storeName = 'KV'
  private db: Promise<IDBPDatabase>

  constructor(dbName: string) {
    if (!dbName.trim()) {
      throw new Error('dbName is required')
    }
    this.db = openDB(dbName, 1, {
      upgrade(db) {
        db.createObjectStore(KVStore.storeName)
      }
    })
  }

  async getItem<T>(key: IDBValidKey): Promise<T | null> {
    const db = await this.db
    return (await db.get(KVStore.storeName, key)) || null
  }

  async setItem<T>(key: IDBValidKey, value: T): Promise<void> {
    const db = await this.db
    const tx = db.transaction(KVStore.storeName, 'readwrite')
    await Promise.all([tx.store.put(value, key), tx.done])
  }

  async removeItem(key: IDBValidKey): Promise<void> {
    const db = await this.db
    const tx = db.transaction(KVStore.storeName, 'readwrite')
    await Promise.all([tx.store.delete(key), tx.done])
  }
}