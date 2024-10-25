import { CoseAPI } from '$lib/canisters/cose'
import {
  messageCanisterAPI,
  type MessageCanisterAPI,
  type UserInfo
} from '$lib/canisters/message'
import { ChannelAPI } from '$lib/canisters/messagechannel'
import { ProfileAPI, type ProfileInfo } from '$lib/canisters/messageprofile'
import { agent } from '$lib/stores/auth'
import { decodeCBOR, utf8ToBytes } from '$lib/utils/crypto'
import { KVStore } from '$lib/utils/store'
import type { Identity } from '@dfinity/agent'
import type { Principal } from '@dfinity/principal'
import { derived, readonly, writable, type Readable } from 'svelte/store'

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

    this.identity = agent.id
    this.principal = agent.id.getPrincipal()
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
    this._user.set(val)
    return val
  }

  // ------------ Profile ------------

  async getProfile(): Promise<ProfileInfo> {
    const val = await this._db.get<ProfileInfo>('My', 'Profile')
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
    this._profile.set(val)
    return val
  }
}
