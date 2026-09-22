import { openDB, type IDBPDatabase } from 'idb'
import { LegacyError, requireLegacy, principal } from './base'
import { Principal } from '@icp-sdk/core/principal'

function cached(value: any, depth = 0): any {
  requireLegacy(depth < 32, 'limit', 'Legacy cache nesting limit')
  if (!value || value instanceof Uint8Array || value instanceof Principal) return value
  if (Array.isArray(value)) return value.map((v) => cached(v, depth + 1))
  if (value instanceof Map)
    return new Map([...value].map(([k, v]) => [cached(k, depth + 1), cached(v, depth + 1)]))
  if (typeof value === 'object' && Object.getPrototypeOf(value) === Object.prototype) {
    if (value._isPrincipal === true) return Principal.from(value)
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, cached(v, depth + 1)]))
  }
  return value
}

/** Never creates/upgrades a legacy database or writes even a cache entry. */
export class LegacyLocalReader {
  private constructor(private readonly db: IDBPDatabase) {}
  static async open(identity: string): Promise<LegacyLocalReader | null> {
    const name = `ICPanda_${principal(identity)}`
    const databases = await indexedDB.databases()
    if (!databases.some((db) => db.name === name)) return null
    const db = await openDB(name, undefined, {
      upgrade(_db, _old, _next, transaction) {
        transaction.abort()
      },
      blocking() {
        db.close()
      }
    })
    requireLegacy(
      db.version === 1 &&
        ['My', 'Keys', 'Channels', 'Messages'].every((s) => db.objectStoreNames.contains(s)),
      'version',
      'Unsupported legacy IndexedDB schema'
    )
    return new LegacyLocalReader(db)
  }
  async get(store: 'My' | 'Keys', key: string): Promise<unknown> {
    return cached(await this.db.get(store, key))
  }
  async *entries(
    store: 'My' | 'Keys' | 'Channels' | 'Messages',
    limit = 100000
  ): AsyncGenerator<{ key: IDBValidKey; value: unknown }> {
    // A separate read transaction per page avoids holding a transaction across
    // consumer awaits. The caller labels this active-browser read a snapshot.
    let last: IDBValidKey | undefined
    let count = 0
    for (;;) {
      const tx = this.db.transaction(store, 'readonly')
      let cursor = await tx.store.openCursor(
        last === undefined ? undefined : IDBKeyRange.lowerBound(last, true)
      )
      const page: { key: IDBValidKey; value: unknown }[] = []
      while (cursor && page.length < 64) {
        page.push({ key: cursor.key, value: cached(cursor.value) })
        cursor = await cursor.continue()
      }
      await tx.done
      if (!page.length) return
      for (const entry of page) {
        if (++count > limit) throw new LegacyError('limit', 'Legacy database entry limit')
        yield entry
      }
      last = page.at(-1)!.key
    }
  }
  close() {
    this.db.close()
  }
}
