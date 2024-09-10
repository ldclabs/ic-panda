import { Principal } from '@dfinity/principal'
import { openDB, type IDBPDatabase } from 'idb'

export type IDBValidKey = string | number | Date | BufferSource | IDBValidKey[]

export class KVStore {
  private db: Promise<IDBPDatabase>

  constructor(
    dbName: string,
    version: number = 1,
    stores: Array<[string] | [string, IDBObjectStoreParameters]> = []
  ) {
    if (!dbName.trim()) {
      throw new Error('dbName is required')
    }
    if (stores.length == 0) {
      throw new Error('stores is empty')
    }

    stores.push()
    this.db = openDB(dbName, version, {
      upgrade(db) {
        for (const store of stores) {
          if (!db.objectStoreNames.contains(store[0])) {
            db.createObjectStore(store[0], store[1])
          }
        }
      }
    })
  }

  async get<T>(storeName: string, key: IDBValidKey): Promise<T | null> {
    const db = await this.db
    const tx = db.transaction(storeName, 'readonly')
    const value = await tx.store.get(key)
    await tx.done
    return decodeObjectPrincipal(value)
  }

  async *iterate(
    storeName: string,
    query: IDBValidKey | IDBKeyRange,
    direction?: IDBCursorDirection
  ) {
    const db = await this.db
    const tx = db.transaction(storeName, 'readonly')
    let cursor = await tx.store.openCursor(query, direction)

    while (cursor) {
      yield {
        key: cursor.key,
        value: decodeObjectPrincipal(cursor.value)
      }

      cursor = await cursor.continue()
    }

    return await tx.done
  }

  async set<T>(storeName: string, value: T, key?: IDBValidKey): Promise<void> {
    const db = await this.db
    const tx = db.transaction(storeName, 'readwrite')
    await tx.store.put(encodeObjectPrincipal(value, tx.store.keyPath), key)
    await tx.done
  }

  async setMany<T>(storeName: string, values: T[]): Promise<void> {
    const db = await this.db
    const tx = db.transaction(storeName, 'readwrite')
    const keyPath = tx.store.keyPath
    await Promise.all(
      values.map((value) => tx.store.put(encodeObjectPrincipal(value, keyPath)))
    )
    await tx.done
  }

  async add<T>(storeName: string, value: T): Promise<void> {
    const db = await this.db
    const tx = db.transaction(storeName, 'readwrite')
    const keyPath = tx.store.keyPath
    await tx.store.add(encodeObjectPrincipal(value, keyPath))
    await tx.done
  }

  async delete(
    storeName: string,
    key: IDBValidKey | IDBKeyRange
  ): Promise<void> {
    const db = await this.db
    const tx = db.transaction(storeName, 'readwrite')
    await tx.store.delete(key)
    await tx.done
  }

  async clear(storeName: string): Promise<void> {
    const db = await this.db
    const tx = db.transaction(storeName, 'readwrite')
    await tx.store.clear()
    await tx.done
  }
}

export function encodeObjectPrincipal(
  obj: any,
  keyPath: string | string[]
): any {
  if (!obj || !keyPath) return obj
  if (!Array.isArray(keyPath)) {
    keyPath = [keyPath]
  }

  if (Object.getPrototypeOf(obj) == Object.prototype) {
    for (const key of keyPath) {
      if (key.startsWith('_')) {
        obj[key] = obj[key.slice(1)]
        if (obj[key] instanceof Principal) {
          obj[key] = obj[key].toUint8Array()
        } else if (obj[key] && obj[key]._isPrincipal === true) {
          obj[key] = obj[key]._arr
        }
      }
    }
  }

  return obj
}

export function decodeObjectPrincipal(obj: any): any {
  if (!obj || obj instanceof Principal) return obj

  if (Object.getPrototypeOf(obj) == Object.prototype) {
    if (obj._isPrincipal === true) {
      return Principal.from(obj)
    }

    for (const key of Object.keys(obj)) {
      obj[key] = decodeObjectPrincipal(obj[key])
    }
  }

  if (Array.isArray(obj)) {
    for (let i = 0; i < obj.length; i++) {
      obj[i] = decodeObjectPrincipal(obj[i])
    }
  }

  return obj
}
