import type { ReaderCheckpoint } from './checkpoint'
import { Principal } from '@icp-sdk/core/principal'
import {
  binary,
  digest,
  LegacyError,
  MAX_ARCHIVE,
  observation,
  OSS_CHUNK,
  pack,
  principal,
  requireLegacy,
  type Failure,
  type Inventory,
  type SourceObject
} from './base'

type CoreService = 'message' | 'channel' | 'profile' | 'cose' | 'bucket'
export type Service = CoreService | 'ledger' | 'minter'
const methods: Record<Service, ReadonlySet<string>> = {
  ledger: new Set(['icrc1_balance_of', 'icrc1_decimals', 'icrc1_symbol']),
  minter: new Set(['get_state', 'get_block']),
  message: new Set([
    'get_state',
    'get_user',
    'my_iv',
    'icrc3_get_blocks',
    'icrc3_get_tip_certificate'
  ]),
  channel: new Set([
    'get_state',
    'my_channel_ids',
    'get_channel_if_update',
    'list_messages',
    'get_message',
    'download_files_token'
  ]),
  profile: new Set(['get_profile']),
  cose: new Set([
    'setting_get',
    'vetkd_public_key',
    'vetkd_encrypted_key',
    'ecdh_cose_encrypted_key'
  ]),
  bucket: new Set(['get_file_info', 'get_file_chunks', 'list_files', 'get_folder_info'])
}
export interface Transport {
  identity(): string
  call(service: Service, canister: string, method: string, args: unknown[]): Promise<unknown>
}
export interface ReaderSource {
  principal: string
  message: string
  mode: Inventory['mode']
  // Explicit I0 inventory: never accept new destinations from an untrusted reply.
  allowed: Record<CoreService, string[]> & Partial<Record<'ledger' | 'minter', string[]>>
  derivation?: { contextVersion: 1 | 2; keyNames: Record<Inventory['mode'], string> }
}
type RecordValue = Record<string, any>
export function errorCode(cause: unknown): Failure {
  return cause instanceof LegacyError ? cause.code : 'network'
}
function unwrap(value: unknown): any {
  requireLegacy(value && typeof value === 'object', 'corrupt', 'Malformed canister response')
  const result = value as RecordValue
  if ('Ok' in result) return result['Ok']
  if ('Err' in result) {
    const error = String(result['Err'])
    // Keep raw server text out of diagnostics: it may contain a SettingPath.
    const code = /not.?found|not exist/i.test(error)
      ? 'missing'
      : /unauthor|permission|not (a |the )?(member|manager)|access|forbidden|authentication/i.test(
            error
          )
        ? 'permission'
        : 'version'
    throw new LegacyError(code, `Legacy service returned ${code}`)
  }
  throw new LegacyError('corrupt', 'Missing result variant')
}
export class LegacyReader {
  readonly inventory: Inventory
  private completed = new Set<string>()
  private total = 0
  private seen = new Map<string, string>()
  constructor(
    readonly source: ReaderSource,
    private readonly transport: Transport,
    private readonly checkpoint?: {
      restored: ReaderCheckpoint | null
      save(value: ReaderCheckpoint): Promise<void>
    }
  ) {
    principal(source.principal)
    requireLegacy(
      source.allowed.message.includes(source.message),
      'permission',
      'Source not in deployment inventory'
    )
    this.inventory = {
      format: 'dmsg-legacy-inventory/1',
      principal: source.principal,
      messageCanister: source.message,
      mode: source.mode,
      snapshot: 'pre_migration',
      objects: [],
      gaps: [],
      calls: []
    }
    const restored = checkpoint?.restored
    if (restored) {
      requireLegacy(
        restored.inventory.principal === source.principal &&
          restored.inventory.messageCanister === source.message &&
          restored.inventory.mode === source.mode,
        'permission',
        'Checkpoint identity mismatch'
      )
      this.inventory = restored.inventory
      for (const object of this.inventory.objects) {
        requireLegacy(
          digest(object.bytes) === object.digest,
          'corrupt',
          'Checkpoint object mismatch'
        )
        this.seen.set(object.key, object.digest)
        this.total += object.bytes.length
      }
      this.completed = new Set(restored.completed)
    }
  }
  private persist() {
    return this.checkpoint?.save({ inventory: this.inventory, completed: [...this.completed] })
  }
  async call(
    service: Service,
    canister: string,
    method: string,
    args: unknown[] = []
  ): Promise<any> {
    requireLegacy(
      methods[service].has(method) &&
        this.source.allowed[service]?.includes(principal(canister)),
      'permission',
      'Legacy method or destination denied'
    )
    requireLegacy(
      this.transport.identity() === this.source.principal,
      'permission',
      'Legacy identity changed'
    )
    const entry = { canister, method, at: Date.now(), outcome: 'ok' as 'ok' | Failure }
    this.inventory.calls.push(entry)
    requireLegacy(this.inventory.calls.length <= 100000, 'limit', 'Legacy call limit')
    try {
      const raw = await this.transport.call(service, canister, method, args)
      const result = service === 'ledger' || service === 'minter' ? raw : unwrap(raw)
      requireLegacy(
        this.transport.identity() === this.source.principal,
        'permission',
        'Legacy identity changed during read'
      )
      return result
    } catch (error) {
      entry.outcome = errorCode(error)
      throw error
    }
  }
  add(
    key: string,
    kind: SourceObject['kind'],
    value: unknown,
    trust: SourceObject['trust'] = 'query_observation'
  ) {
    const bytes = pack(observation(value)),
      hash = digest(bytes)
    const previous = this.seen.get(key)
    requireLegacy(
      !previous || previous === hash,
      'corrupt',
      'Source changed during snapshot; collect a new snapshot'
    )
    if (previous) return
    this.total += bytes.length
    requireLegacy(
      this.total <= MAX_ARCHIVE && this.seen.size < 100000,
      'limit',
      'Legacy archive capacity exceeded'
    )
    this.seen.set(key, hash)
    this.inventory.objects.push({
      key,
      kind,
      bytes,
      digest: hash,
      trust,
      observedAt: Date.now()
    })
  }
  gap(source: string, cause: unknown) {
    this.inventory.gaps.push({
      source,
      code: errorCode(cause),
      detail:
        cause instanceof LegacyError
          ? cause.message
          : 'Legacy read failed; source availability is unknown'
    })
  }
  async collect(signal?: AbortSignal, onProgress?: (inventory: Inventory) => Promise<void>) {
    const check = () => requireLegacy(!signal?.aborted, 'cancelled', 'Inventory paused')
    check()
    const user = await this.call('message', this.source.message, 'get_user', [[]])
    requireLegacy(
      principal(user.id) === this.source.principal,
      'permission',
      'Legacy user mismatch'
    )
    this.add(`${this.source.message}/identity/${this.source.principal}`, 'identity', user)
    const profile = principal(user.profile_canister)
    try {
      this.add(
        `${profile}/profile/${this.source.principal}`,
        'profile',
        await this.call('profile', profile, 'get_profile', [
          [Principal.fromText(this.source.principal)]
        ])
      )
    } catch (cause) {
      this.gap(`${profile}/profile`, cause)
    }
    const state = await this.call('message', this.source.message, 'get_state')
    const shards = new Set<string>(
      [...state.channel_canisters, ...state.matured_channel_canisters].map(principal)
    )
    for (const canister of shards) {
      check()
      let ids: number[]
      try {
        ids = Array.from(await this.call('channel', canister, 'my_channel_ids'))
      } catch (cause) {
        this.gap(`${canister}/channels`, cause)
        continue
      }
      requireLegacy(
        ids.length <= 10000 && new Set(ids).size === ids.length,
        'corrupt',
        'Invalid channel list'
      )
      for (const id of ids) {
        check()
        const key = `${canister}/channel/${id}`
        try {
          const infos = await this.call('channel', canister, 'get_channel_if_update', [id, 0n])
          requireLegacy(infos.length === 1, 'missing', 'Channel snapshot missing')
          const info = infos[0]
          requireLegacy(
            info.id === id && principal(info.canister) === canister,
            'corrupt',
            'Channel source mismatch'
          )
          this.add(key, 'channel', info)
          const start = info.message_start,
            end = info.latest_message_id + 1
          requireLegacy(
            Number.isSafeInteger(start) &&
              start >= 1 &&
              Number.isSafeInteger(end) &&
              end >= start &&
              end <= 0x100000000 &&
              end - start <= 100000,
            'limit',
            'Invalid message range'
          )
          for (let cursor = start; cursor < end; cursor += 20) {
            check()
            const stop = Math.min(cursor + 20, end),
              pageKey = `${key}/range/${cursor}-${stop}`
            if (this.completed.has(pageKey)) continue
            const errorsBefore = this.inventory.gaps.length
            try {
              const messages = await this.call('channel', canister, 'list_messages', [
                id,
                [cursor],
                [stop]
              ])
              requireLegacy(
                Array.isArray(messages) && messages.length <= 20,
                'corrupt',
                'Invalid message page'
              )
              const found = new Set<number>()
              for (const message of messages) {
                requireLegacy(
                  Number.isInteger(message.id) &&
                    message.id >= cursor &&
                    message.id < stop &&
                    !found.has(message.id),
                  'corrupt',
                  'Duplicate or out-of-range message'
                )
                found.add(message.id)
                this.add(`${key}/message/${message.id}`, 'message', message)
              }
              // Explicit source range, not page length, advances the cursor.
              for (let n = cursor; n < stop; n++)
                if (!found.has(n)) {
                  const deleted = Array.from(info.deleted_messages as number[]).includes(n)
                  if (!deleted) {
                    try {
                      const message = await this.call('channel', canister, 'get_message', [
                        id,
                        n
                      ])
                      requireLegacy(
                        message.id === n,
                        'corrupt',
                        'Wrong single message response'
                      )
                      this.add(`${key}/message/${n}`, 'message', message)
                      continue
                    } catch (cause) {
                      this.gap(`${key}/message/${n}`, cause)
                      continue
                    }
                  }
                  this.gap(
                    `${key}/message/${n}`,
                    new LegacyError(
                      'missing',
                      deleted
                        ? 'Source declares deleted message'
                        : 'Message absent from observed range'
                    )
                  )
                }
            } catch (cause) {
              this.gap(`${key}/range/${cursor}-${stop}`, cause)
            }
            if (this.inventory.gaps.slice(errorsBefore).every((g) => g.code === 'missing'))
              this.completed.add(pageKey)
            await this.persist()
            await onProgress?.(this.inventory)
          }
          const latest = await this.call('channel', canister, 'get_channel_if_update', [
            id,
            info.updated_at
          ])
          if (latest.length)
            this.gap(
              key,
              new LegacyError('version', 'Channel changed during pre-migration snapshot')
            )
        } catch (cause) {
          this.gap(key, cause)
        }
      }
    }
    await this.persist()
    return this.inventory
  }
  async resources(signal?: AbortSignal) {
    const owner = Principal.fromText(this.source.principal)
    for (const ledger of this.source.allowed.ledger ?? []) {
      try {
        const balance = await this.call('ledger', ledger, 'icrc1_balance_of', [
          { owner, subaccount: [] }
        ])
        const decimals = await this.call('ledger', ledger, 'icrc1_decimals'),
          symbol = await this.call('ledger', ledger, 'icrc1_symbol')
        requireLegacy(
          typeof balance === 'bigint' &&
            balance >= 0n &&
            Number.isInteger(decimals) &&
            decimals <= 30 &&
            typeof symbol === 'string' &&
            symbol.length <= 32,
          'corrupt',
          'Invalid legacy ledger view'
        )
        this.add(`${ledger}/balance/${this.source.principal}`, 'entitlement', {
          type: 'balance',
          ledger,
          owner,
          balance,
          decimals,
          symbol,
          scope: 'default_subaccount_only',
          disposition: 'unresolved'
        })
      } catch (cause) {
        this.gap(`${ledger}/balance`, cause)
      }
    }
    for (const minter of this.source.allowed.minter ?? []) {
      try {
        const state = await this.call('minter', minter, 'get_state'),
          count = Number(state.next_block_height)
        requireLegacy(
          Number.isSafeInteger(count) && count >= 0 && count <= 10000,
          'limit',
          'Minter history exceeds reviewed scope'
        )
        for (let block = 0; block < count; block++) {
          requireLegacy(!signal?.aborted, 'cancelled', 'Resource inventory paused')
          const value = await this.call('minter', minter, 'get_block', [BigInt(block)])
          requireLegacy(
            Array.isArray(value) && value.length === 1,
            'missing',
            'Minter log slot missing'
          )
          if (value[0].linker.some((p: Principal) => p.toText() === this.source.principal))
            this.add(`${minter}/pol/${block}`, 'entitlement', {
              type: 'pol_log',
              block,
              record: value[0],
              settlement: 'unverified',
              disposition: 'unresolved'
            })
        }
        this.add(`${minter}/pending-scope`, 'entitlement', {
          type: 'pending_scope',
          available: false,
          reason: 'Legacy API does not enumerate prepared linkers or prove ledger settlement',
          disposition: 'unresolved'
        })
      } catch (cause) {
        this.gap(`${minter}/pol`, cause)
      }
    }
  }
  async setting(path: RecordValue, canister: string) {
    // Fall back only for a definite NotFound; never migrate, write or broaden a
    // permission failure. Return the actual historical ownership path.
    try {
      return { path, value: await this.call('cose', canister, 'setting_get', [path]) }
    } catch (cause) {
      if (!(cause instanceof LegacyError) || cause.code !== 'missing' || !path['user_owned'])
        throw cause
      const historical = { ...path, user_owned: false }
      return {
        path: historical,
        value: await this.call('cose', canister, 'setting_get', [historical])
      }
    }
  }
  async file(
    bucket: string,
    id: number,
    token: Uint8Array | null,
    signal?: AbortSignal,
    preserveIncomplete = false
  ) {
    const info = await this.call('bucket', bucket, 'get_file_info', [id, token ? [token] : []])
    const size = Number(info.size),
      count = Number(info.chunks)
    requireLegacy(
      Number.isSafeInteger(size) &&
        size >= 0 &&
        size <= MAX_ARCHIVE &&
        Number.isInteger(count) &&
        count >= 0 &&
        count <= Math.ceil(size / OSS_CHUNK),
      'corrupt',
      'Invalid OSS file dimensions'
    )
    if (!preserveIncomplete)
      requireLegacy(
        Number(info.filled) === size && count === Math.ceil(size / OSS_CHUNK),
        'missing',
        'Incomplete legacy upload'
      )
    const chunks: { index: number; bytes: Uint8Array }[] = [],
      missing: number[] = []
    for (let i = 0; i < Math.ceil(size / OSS_CHUNK); i++) {
      requireLegacy(!signal?.aborted, 'cancelled', 'File copy paused')
      const page = await this.call('bucket', bucket, 'get_file_chunks', [
        id,
        i,
        [1],
        token ? [token] : []
      ])
      if (preserveIncomplete && Array.isArray(page) && page.length === 0) {
        missing.push(i)
        continue
      }
      requireLegacy(
        Array.isArray(page) && page.length === 1 && page[0][0] === i,
        'missing',
        'Missing or reordered OSS chunk'
      )
      const chunk = binary(page[0][1])
      requireLegacy(
        chunk.length === Math.min(OSS_CHUNK, size - i * OSS_CHUNK),
        'corrupt',
        'Incorrect OSS chunk length'
      )
      chunks.push({ index: i, bytes: chunk })
    }
    const complete =
      missing.length === 0 && Number(info.filled) === size && count === chunks.length
    const bytes = new Uint8Array(complete ? size : 0)
    if (complete) chunks.forEach((chunk) => bytes.set(chunk.bytes, chunk.index * OSS_CHUNK))
    if (complete && info.hash?.length)
      requireLegacy(
        digest(bytes) === digestBytes(binary(info.hash[0])),
        'corrupt',
        'OSS ciphertext hash mismatch'
      )
    const after = await this.call('bucket', bucket, 'get_file_info', [
      id,
      token ? [token] : []
    ])
    requireLegacy(
      digest(pack(observation(info))) === digest(pack(observation(after))),
      'version',
      'OSS file changed during copy'
    )
    // This is one old COSE object. Transport chunks have no independent AEAD.
    return {
      info,
      bytes,
      digest: digest(bytes),
      complete,
      missing,
      present: complete ? [] : chunks,
      chunks: chunks.map((c) => ({
        index: c.index,
        size: c.bytes.length,
        digest: digest(c.bytes)
      }))
    }
  }
}
function digestBytes(bytes: Uint8Array) {
  return Array.from(bytes, (n) => n.toString(16).padStart(2, '0')).join('')
}
