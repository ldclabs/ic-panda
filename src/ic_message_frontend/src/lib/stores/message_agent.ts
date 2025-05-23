import { CoseAPI } from '$lib/canisters/cose'
import {
  messageCanisterAPI,
  type MessageCanisterAPI,
  type UserInfo
} from '$lib/canisters/message'
import {
  ChannelAPI,
  type ChannelBasicInfo,
  type ChannelInfo,
  type Message
} from '$lib/canisters/messagechannel'
import { ProfileAPI, type ProfileInfo } from '$lib/canisters/messageprofile'
import { TokenLedgerAPI } from '$lib/canisters/tokenledger'
import { type FilePayload } from '$lib/types/message'
import { dynAgent } from '$lib/utils/auth'
import { decodeCBOR, utf8ToBytes } from '$lib/utils/crypto'
import { KVStore } from '$lib/utils/store'
import { type TokenInfo } from '$lib/utils/token'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { Claims } from '@ldclabs/cose-ts/cwt'
import {
  derived,
  readable,
  readonly,
  writable,
  type Readable
} from 'svelte/store'
import { setProfile as kvsSetProfile, setUser as kvsSetUser } from './kvstore'

export type CachedMessage = Message & { canister: Principal; channel: number }

export interface SyncAt {
  _sync_at: number
}

// should be running in Web Workers.
export class MessageAgent extends EventTarget {
  readonly id: string
  readonly identity: Identity
  readonly principal: Principal
  readonly api: MessageCanisterAPI

  private _db: KVStore
  private _profileAPI: ProfileAPI | null = null
  private _coseAPI: CoseAPI | null = null
  private _user = writable<UserInfo | null>(null)
  private _profile = writable<ProfileInfo | null>(null)

  // keys in 'My' store
  static KEY_NOTIFY_PERM = 'NotifyPerm' // denied | granted | default
  static KEY_REFRESH_MY_CHANNELS_AT = 'RefreshMyChannelsAt' // ms

  static channelBasicInfo(
    info: ChannelBasicInfo | ChannelBasicInfo
  ): ChannelBasicInfo {
    return {
      id: info.id,
      gas: info.gas,
      updated_at: info.updated_at,
      name: info.name,
      paid: info.paid,
      canister: info.canister,
      image: info.image,
      latest_message_at: info.latest_message_at,
      latest_message_by: info.latest_message_by,
      latest_message_id: info.latest_message_id,
      my_setting: info.my_setting
    }
  }

  static isChannelInfo(info: ChannelBasicInfo | ChannelInfo | null): boolean {
    return (
      (info &&
        Object.getPrototypeOf(info) == Object.prototype &&
        Array.isArray((info as ChannelInfo).managers)) ||
      false
    )
  }

  static async create(): Promise<MessageAgent> {
    const cli = new MessageAgent()
    await cli.api.refreshAllState()
    if (cli.api.myInfo) {
      // save latest user info
      await cli.setUser(cli.api.myInfo)
    }
    return cli
  }

  private constructor() {
    super()

    this.identity = dynAgent.id
    this.principal = dynAgent.id.getPrincipal()
    this.id = this.principal.toText()
    this.api = messageCanisterAPI
    this._db = new KVStore('ICPanda_' + this.id, 1, [
      ['My'],
      ['Keys'],
      [
        'Channels', // ChannelInfo | ChannelBasicInfo
        {
          keyPath: ['_canister', 'id']
        }
      ],
      [
        'Messages',
        {
          keyPath: ['_canister', 'channel', 'id']
        }
      ]
    ])
  }

  get hasCOSE(): boolean {
    return this._coseAPI != null
  }

  // ------------ api ------------

  get profileAPI(): ProfileAPI {
    if (!this._profileAPI) {
      throw new Error('Profile API not initialized')
    }
    return this._profileAPI
  }

  get coseAPI(): CoseAPI {
    if (!this._coseAPI) {
      throw new Error('username is required to continue')
    }
    return this._coseAPI
  }

  channelAPI(canister: Principal): ChannelAPI {
    return this.api.channelAPI(canister)
  }

  // ------------ Keys ------------

  async getMasterKeys<T>(): Promise<T[]> {
    const val = await this._db.get<T[]>('Keys', 'MK')
    return val || []
  }

  async setMasterKeys<T>(vals: T[]): Promise<void> {
    await this._db.set<T[]>('Keys', vals, 'MK')
  }

  async getECDHKey(): Promise<Uint8Array | null> {
    const val = await this._db.get<Uint8Array>('Keys', 'EK')
    return val || null
  }

  async setECDHKey(val: Uint8Array): Promise<void> {
    await this._db.set<Uint8Array>('Keys', val, 'EK')
  }

  async getKEK(canister: Principal, id: number): Promise<Uint8Array | null> {
    const val = await this._db.get<Uint8Array>(
      'Keys',
      `KEK:${canister.toText()}:${id}`
    )
    return val || null
  }

  async setKEK(
    canister: Principal,
    id: number,
    val: Uint8Array
  ): Promise<void> {
    await this._db.set<Uint8Array>(
      'Keys',
      val,
      `KEK:${canister.toText()}:${id}`
    )
  }

  async getChannelStorageToken(
    canister: Principal,
    id: number
  ): Promise<[Uint8Array, number, [Principal, number]]> {
    let rt = await this._db.get<[Uint8Array, number, [Principal, number]]>(
      'Keys',
      `CST:${canister.toText()}:${id}`
    )

    if (rt && rt[1] >= Date.now()) {
      return rt
    }

    const api = this.api.channelAPI(canister)
    const token = await api.download_files_token(id)
    const access_token = Uint8Array.from(token.access_token)
    const [_protectedBytes, _unprotected, payload, _signature] = decodeCBOR(
      access_token
    ) as [Uint8Array, any, Uint8Array, Uint8Array]
    const claims = Claims.fromBytes(payload)
    rt = [access_token, (claims.exp - 300) * 1000, token.storage]
    await this._db.set('Keys', rt, `CST:${canister.toText()}:${id}`)
    return rt
  }

  // ------------ User ------------

  async getKV(): Promise<Map<string, Uint8Array>> {
    const kv = await this._db.get<Map<string, Uint8Array>>('My', 'KV')
    return kv || (await this.refreshKV())
  }

  async refreshKV(): Promise<Map<string, Uint8Array>> {
    let kv: Map<string, Uint8Array> | null = null

    if (this._coseAPI) {
      const ns = this.id.replaceAll('-', '_')
      const output = await this._coseAPI
        .setting_get({
          ns,
          key: utf8ToBytes('KV'),
          subject: [this.principal],
          version: 0,
          user_owned: false
        })
        .catch((e) => null)

      if (output?.payload[0]) {
        kv = decodeCBOR(output.payload[0] as Uint8Array)
      }
      if (kv) {
        await this.setKV(kv)
      }
    }

    return kv || new Map()
  }

  async setKV(kv: Map<string, Uint8Array>): Promise<Map<string, Uint8Array>> {
    await this._db.set<Map<string, Uint8Array>>('My', kv, 'KV')
    return kv
  }

  async getLocal<T>(key: string): Promise<T | null> {
    const val = await this._db.get<T>('My', `Local:${key}`)
    return val || null
  }

  async setLocal<T>(key: string, val: T): Promise<void> {
    await this._db.set<T>('My', val, `Local:${key}`)
  }

  async getUser(): Promise<UserInfo> {
    const val = await this._db.get<UserInfo>('My', 'User')
    return val || (await this.fetchUser())
  }

  subscribeUser(): Readable<UserInfo | null> {
    return readonly(this._user)
  }

  async fetchUser(): Promise<UserInfo> {
    await this.api.refreshMyInfo()
    const val = this.api.myInfo
    if (!val) {
      throw new Error('User not exists')
    }
    return this.setUser(val)
  }

  async setUser(info: UserInfo): Promise<UserInfo> {
    const val = { ...info }
    if (!this._profileAPI) {
      this._profileAPI = this.api.profileAPI(info.profile_canister)
    }
    if (!this._coseAPI && info.cose_canister[0]) {
      this._coseAPI = this.api.coseAPI(info.cose_canister[0])
    }
    await this._db.set<UserInfo>('My', val, 'User')
    await kvsSetUser(Date.now(), val)
    this._user.set(val)
    return val
  }

  async getUploadingFile(
    canister: Principal,
    id: number
  ): Promise<FilePayload | null> {
    const val = await this._db.get<FilePayload>(
      'My',
      `File:${canister.toText()}:${id}`
    )
    return val || null
  }

  async setUploadingFile(
    canister: Principal,
    id: number,
    val: FilePayload
  ): Promise<void> {
    await this._db.set('My', val, `File:${canister.toText()}:${id}`)
  }

  async deleteUploadingFile(canister: Principal, id: number): Promise<void> {
    await this._db.delete('My', `File:${canister.toText()}:${id}`)
  }

  // ------------ Profile ------------

  async getProfile(): Promise<ProfileInfo> {
    const val = await this._db.get<ProfileInfo>('My', 'Profile')
    if (val) {
      // refresh profile in background
      this.fetchProfile().catch(() => {})
    }
    return val || (await this.fetchProfile())
  }

  async subscribeProfile(): Promise<Readable<null | (ProfileInfo & UserInfo)>> {
    const profile = await this.getProfile().catch(() => null)
    if (profile) {
      this._profile.set(profile)
    }

    return derived(
      [this._user, this._profile],
      ([$user, $profile], set: (value: ProfileInfo & UserInfo) => void) => {
        if ($user && $profile) {
          set({ ...$user, ...$profile })
        }
      },
      null
    )
  }

  async fetchProfile(): Promise<ProfileInfo> {
    const api = this.profileAPI
    await api.refreshMyProfile()
    const val = this.profileAPI.myProfile
    if (!val) {
      throw new Error('Profile not exists')
    }
    return this.setProfile(val)
  }

  async setProfile(info: ProfileInfo): Promise<ProfileInfo> {
    const val = { ...info }
    await this._db.set<ProfileInfo>('My', val, 'Profile')
    await kvsSetProfile(val)
    this._profile.set(val)
    return val
  }

  async loadTokens(tokenIDs: Principal[]): Promise<TokenInfo[]> {
    const tokens: TokenInfo[] = []
    if (tokenIDs.length === 0) {
      return tokens
    }

    for (const id of tokenIDs) {
      const ledgerID = id.toText()
      let token = await this._db.get<TokenInfo>('My', `Token:${ledgerID}`)
      if (!token) {
        token = await this.fetchToken(ledgerID)
      }
      if (token) {
        tokens.push(token)
      }
    }

    return tokens
  }

  async fetchToken(ledgerID: string): Promise<TokenInfo | null> {
    try {
      const id = Principal.fromText(ledgerID)
      const api = await TokenLedgerAPI.fromID(id)
      await this._db.set<TokenInfo>(
        'My',
        Object.assign({}, api.token),
        `Token:${id.toText()}`
      )
      return api.token
    } catch (e) {
      return null
    }
  }

  async cleanTokenCache(ledgerID: string): Promise<void> {
    await this._db.delete('My', `Token:${ledgerID}`)
  }

  // ------------ Channels ------------

  async getChannel(
    canister: Principal,
    id: number
  ): Promise<ChannelInfo & SyncAt> {
    let val = await this._db.get<ChannelInfo & SyncAt>('Channels', [
      canister.toUint8Array() as BufferSource,
      id
    ])
    if (!MessageAgent.isChannelInfo(val)) {
      val = await this.fetchChannel(canister, id)
    }
    return val!
  }

  async subscribeChannel(
    canister: Principal,
    id: number,
    signal?: AbortSignal
  ): Promise<Readable<ChannelInfo & SyncAt>> {
    const channel = await this.getChannel(canister, id)
    const eventType = `Channel:${canister.toText()}/${id}`
    return readable(channel, (set) => {
      const listener = (ev: Event) => set((ev as CustomEvent).detail)
      this.addEventListener(
        eventType,
        listener,
        signal ? { signal } : undefined
      )
      return () => this.removeEventListener(eventType, listener)
    })
  }

  async subscribeMyChannels(
    signal?: AbortSignal
  ): Promise<Readable<ChannelBasicInfo[]>> {
    let channels = await this.loadMyChannels()
    if (channels.length === 0) {
      await this.fetchMyChannels(signal)
      channels = await this.loadMyChannels()
    }
    channels.sort(ChannelAPI.compareChannels)
    return readable(channels, (_set, update) => {
      const updateListener = (ev: Event) => {
        const info = MessageAgent.channelBasicInfo((ev as CustomEvent).detail)

        update((prev) => {
          const idx = prev.findIndex(
            (c) =>
              c.canister.compareTo(info.canister) === 'eq' && c.id == info.id
          )
          if (idx >= 0) {
            prev[idx] = info
          } else {
            prev.push(info)
          }

          prev.sort(ChannelAPI.compareChannels)
          return [...prev]
        })
      }
      const removeListener = (ev: Event) => {
        const { canister, id } = (ev as CustomEvent).detail
        update((prev) =>
          prev.filter(
            (c) => c.canister.compareTo(canister) !== 'eq' || c.id !== id
          )
        )
      }

      this.addEventListener(
        'Channel',
        updateListener,
        signal ? { signal } : undefined
      )

      this.addEventListener(
        'RemoveChannel',
        removeListener,
        signal ? { signal } : undefined
      )

      return () => {
        this.removeEventListener('Channel', updateListener)
        this.removeEventListener('RemoveChannel', removeListener)
      }
    })
  }

  async loadMyChannels(): Promise<ChannelBasicInfo[]> {
    const iter = await this._db.iterate('Channels')
    const rt: ChannelBasicInfo[] = []
    for await (const cursor of iter) {
      rt.push(MessageAgent.channelBasicInfo(cursor.value))
    }
    return rt
  }

  async fetchMyChannels(signal?: AbortSignal): Promise<void> {
    const state = this.api.state
    if (!state) {
      throw new Error('Wait for message state')
    }

    const canisters: Principal[] = [
      ...state.channel_canisters,
      ...state.matured_channel_canisters
    ]

    await this.setLocal(MessageAgent.KEY_REFRESH_MY_CHANNELS_AT, Date.now())
    await Promise.all(
      canisters.map(async (canister) => {
        const api = this.api.channelAPI(canister)
        const ids = await api.my_channel_ids()
        const canisterId = canister.toUint8Array()

        let latest_message_at = 0n
        const iter = await this._db.iterate(
          'Channels',
          IDBKeyRange.bound(
            [canisterId, 0],
            [canisterId, 0xffffffff],
            true,
            true
          )
        )

        for await (const cursor of iter) {
          const info = cursor.value as ChannelInfo
          if (!ids.includes(info.id)) {
            // delete channel and it's messages
            await this.removeChannel(canister, info.id)
            continue
          }

          if (latest_message_at < info.latest_message_at) {
            latest_message_at = info.latest_message_at
          }
          if (latest_message_at < info.my_setting.updated_at) {
            latest_message_at = info.my_setting.updated_at
          }
        }

        if (signal?.aborted) {
          return
        }

        const channels = await api.my_channels(latest_message_at)
        for (const channel of channels) {
          await this.setChannel(channel)
        }
      })
    )
  }

  async fetchChannel(
    canister: Principal,
    id: number,
    updated_at: bigint = 0n
  ): Promise<ChannelInfo & SyncAt> {
    const api = this.api.channelAPI(canister)
    const channel = await api.get_channel_if_update(id, updated_at)
    if (!channel) {
      throw new Error('Channel not found')
    }
    const canisterId = canister.toUint8Array()
    if (channel.message_start > 1) {
      // delete truncated messages
      await this._db.delete(
        'Messages',
        IDBKeyRange.bound(
          [canisterId, id, 0],
          [canisterId, id, channel.message_start],
          true,
          true
        )
      )
    }

    for (const mid of channel.deleted_messages) {
      await this.updateDeletedMessage(canisterId, id, mid)
    }

    return (await this.setChannel(channel)) as ChannelInfo & SyncAt
  }

  async setChannel(
    channel: ChannelInfo | ChannelBasicInfo
  ): Promise<ChannelInfo | ChannelBasicInfo> {
    let val = await this._db.get<ChannelInfo>('Channels', [
      channel.canister.toUint8Array() as BufferSource,
      channel.id
    ])
    val = { ...val, ...channel, _sync_at: Date.now() } as ChannelInfo & SyncAt
    await this._db.set('Channels', val)
    this.dispatchEvent(
      new CustomEvent(`Channel:${val.canister.toText()}/${val.id}`, {
        detail: val
      })
    )
    this.dispatchEvent(new CustomEvent('Channel', { detail: val }))

    return val
  }

  // delete channel and it's messages
  async removeChannel(canister: Principal, id: number): Promise<void> {
    const canisterKey = canister.toUint8Array() as BufferSource
    await this._db.delete('Channels', [canisterKey, id])
    await this._db.delete(
      'Messages',
      IDBKeyRange.bound(
        [canisterKey, id, 0],
        [canisterKey, id, 0xffffffff],
        true,
        true
      )
    )
    this.dispatchEvent(
      new CustomEvent('RemoveChannel', { detail: { canister, id } })
    )
  }

  // ------------ Message ------------

  async loadMessages(
    canister: Principal,
    channelId: number,
    start: number, // maybe included
    end: number // not included
  ): Promise<CachedMessage[]> {
    const prefix = [canister.toUint8Array(), channelId]
    if (start < 1) {
      start = 1
    }

    if (end <= start) {
      return []
    }

    if (end - start > 20) {
      start = end - 20
    }

    let messages: CachedMessage[] = []
    const iter = await this._db.iterate(
      'Messages',
      IDBKeyRange.bound([...prefix, start], [...prefix, end], false, true)
    )

    let i = start
    for await (const cursor of iter) {
      if ((cursor.key as [Uint8Array, number, number])[2] !== i) {
        break
      }
      messages.push(cursor.value)
      i += 1
    }
    if (i < end) {
      const items = await this.fetchLatestMessages(canister, channelId, i, end)
      if (items.length > 0) {
        messages = [...messages, ...items]
      }
    }

    return messages
  }

  // should call fetchLatestMessages to get latest messages
  subscribeLatestMessage(
    canister: Principal,
    channel: number,
    start: number,
    signal?: AbortSignal
  ): Readable<CachedMessage | null> {
    const eventType = `Message:${canister.toText()}/${channel}`
    let nextId = start
    return readable<CachedMessage | null>(
      null,
      (set: (value: CachedMessage) => void) => {
        const listener = (ev: Event) => {
          let info = (ev as CustomEvent).detail as CachedMessage
          if (nextId <= info.id) {
            nextId = info.id + 1
            set(info)
          }
        }

        this.addEventListener(
          eventType,
          listener,
          signal ? { signal } : undefined
        )
        return () => this.removeEventListener(eventType, listener)
      }
    )
  }

  async fetchLatestMessages(
    canister: Principal,
    channel: number,
    start: number,
    end: number = 0
  ): Promise<CachedMessage[]> {
    const api = this.api.channelAPI(canister)
    const items = (await api.list_messages(channel, start, end)).map((msg) => {
      return { ...msg, canister, channel }
    })
    await this._db.setMany<CachedMessage>('Messages', items)

    const eventType = `Message:${canister.toText()}/${channel}`
    for (const item of items) {
      this.dispatchEvent(
        new CustomEvent(eventType, {
          detail: item
        })
      )
    }
    return items
  }

  async updateDeletedMessage(
    canister: Uint8Array,
    channel: number,
    messageId: number
  ): Promise<void> {
    const msg = await this._db.get<CachedMessage>('Messages', [
      canister as BufferSource,
      channel,
      messageId
    ])
    if (msg) {
      msg.payload = new Uint8Array()
      await this._db.set<CachedMessage>('Messages', msg)
    }
  }

  async clearCachedMessages(): Promise<void> {
    await this._db.clear('Messages')
  }
}
