import { CoseAPI } from '$lib/canisters/cose'
import {
  messageCanisterAPIAsync,
  type ChannelECDHInput,
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
import {
  AesGcmKey,
  compareBytes,
  coseA256GCMDecrypt0,
  coseA256GCMEncrypt0,
  deriveA256GCMSecret,
  ECDHKey,
  encodeCBOR,
  generateECDHKey,
  hashPassword,
  iana,
  randomBytes
} from '$lib/utils/crypto'
import { KVStore } from '$lib/utils/store'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { derived, readable, type Readable } from 'svelte/store'
import { asyncFactory } from './auth'

const usersKV = new KVStore('ICPanda_Users')
const channelsKV = new KVStore('ICPanda_Channels')
const cacheExp = 2 * 24 * 3600 * 1000

export class MyMessageState {
  readonly principal: Principal
  readonly id: string
  readonly api: MessageCanisterAPI
  readonly coseAPI: CoseAPI | null = null
  readonly profileAPI: ProfileAPI | null = null
  readonly info: Readable<UserInfo>

  private _mk: MasterKey | null = null

  static async with(identity: Identity): Promise<MyMessageState> {
    const api = await messageCanisterAPIAsync()
    let coseAPI: CoseAPI | null = null
    let profileAPI: ProfileAPI | null = null

    if (api.myInfo) {
      profileAPI = await api.profileAPI(api.myInfo.profile_canister)
      if (api.myInfo.cose_canister.length == 1) {
        coseAPI = await api.coseAPI(api.myInfo.cose_canister[0])
      }
    }

    return new MyMessageState(identity.getPrincipal(), api, coseAPI, profileAPI)
  }

  constructor(
    principal: Principal,
    api: MessageCanisterAPI,
    coseAPI: CoseAPI | null,
    profileAPI: ProfileAPI | null
  ) {
    this.principal = principal
    this.id = principal.toText()
    this.api = api
    this.coseAPI = coseAPI
    this.profileAPI = profileAPI
    this.info = derived(api.myInfoStore, ($info, set) => {
      if ($info) {
        usersKV.setItem(this.id, $info)
        set($info)
      } else {
        usersKV.getItem<UserInfo>(this.id).then((info) => {
          if (info) {
            set(info)
          }
        })
      }
    })
  }

  ecdhAvailable(): boolean {
    return this.coseAPI != null
  }

  async masterKey(): Promise<MasterKey | null> {
    if (!this._mk || !this._mk.isUser(this.principal)) {
      const info = await usersKV.getItem<MasterKeyInfo>(`${this.id}:masterKey`)
      if (!info) {
        return null
      }
      this._mk = await MasterKey.fromInfo(this.principal, info)
    }

    return this._mk
  }

  async setMasterKey(
    kind: 'Local' | 'ECDH' | 'VetKey',
    keyId: Uint8Array,
    password: string,
    remoteSecret: Uint8Array,
    passwordExpire: number // milliseconds
  ): Promise<void> {
    this._mk = await MasterKey.from(
      this.principal,
      kind,
      keyId,
      password,
      this.id,
      remoteSecret,
      passwordExpire > 0 ? passwordExpire + Date.now() : 0
    )

    await usersKV.setItem(`${this.id}:masterKey`, this._mk.toInfo())
  }

  async saveMasterKey(): Promise<void> {
    if (!this._mk) {
      throw new Error('master key not set')
    }

    await usersKV.setItem(`${this.id}:masterKey`, this._mk.toInfo())
  }

  async loadECDHCoseEncryptedKey(keyId: Uint8Array): Promise<AesGcmKey> {
    if (!this.coseAPI) {
      throw new Error('coseAPI not available')
    }

    const ns = this.id.replaceAll('-', '_')
    const aad = this.principal.toUint8Array()
    const ecdh = generateECDHKey()
    const output = await this.coseAPI.ecdh_cose_encrypted_key(
      {
        ns,
        key: keyId,
        subject: [this.principal],
        version: 0,
        user_owned: true
      },
      {
        public_key: ecdh.getPublicKey(),
        nonce: randomBytes(12)
      }
    )
    const remoteECDH = ECDHKey.fromPublic(
      iana.EllipticCurveX25519,
      output.public_key as Uint8Array
    )
    const secret = ecdh.ecdh(remoteECDH)
    const data = await coseA256GCMDecrypt0(
      AesGcmKey.fromSecret(secret),
      output.payload as Uint8Array,
      aad
    )

    return AesGcmKey.fromBytes(data)
  }

  async loadChannelKEK(
    channelCanister: Principal,
    channelId: number
  ): Promise<Uint8Array> {
    const kek = await usersKV.getItem<Uint8Array>(
      `${this.id}:${channelCanister.toText()}:${channelId}:KEK`
    )

    if (kek) {
      return kek
    }

    if (!this.coseAPI) {
      throw new Error('Channel KEK not found')
    }

    const ns = this.id.replaceAll('-', '_')
    const output = await this.coseAPI.setting_get({
      ns,
      key: encodeCBOR([channelCanister.toUint8Array(), channelId]),
      subject: [this.principal],
      version: 0,
      user_owned: false
    })

    if (output.dek.length != 1) {
      throw new Error('Channel KEK not found')
    }

    await usersKV.setItem<Uint8Array>(
      `${this.id}:${channelCanister.toText()}:${channelId}:KEK`,
      output.dek[0] as Uint8Array
    )

    return output.dek[0] as Uint8Array
  }

  async loadMyChannels(): Promise<Readable<ChannelBasicInfo[]>> {
    const channels =
      (await usersKV.getItem<ChannelBasicInfo[]>(`${this.id}:myChannels`)) || []
    channels.sort(ChannelAPI.compareChannels)
    const cmap = new Map<string, ChannelBasicInfo>()
    for (const c of channels) {
      cmap.set(`${c.canister.toText()}:${c.id}`, c)
    }

    const state = (await this.api.state) || null
    return readable(channels, (set) => {
      let stopped = false
      const timers: any[] = []
      if (state) {
        const ids: Principal[] = [
          ...state.channel_canisters,
          ...state.matured_channel_canisters
        ]

        Promise.all(
          ids.map(async (id) => {
            if (stopped) {
              return
            }
            const api = await this.api.channelAPI(id)

            if (stopped) {
              return
            }

            let latest_message_at = 0n
            const task = () => {
              api.my_channels(latest_message_at).then((channels) => {
                for (const c of channels) {
                  if (latest_message_at < c.latest_message_at) {
                    latest_message_at = c.latest_message_at
                  }

                  cmap.set(`${c.canister.toText()}:${c.id}`, c)
                }
                channels = Array.from(cmap.values())
                channels.sort(ChannelAPI.compareChannels)
                set(channels)
              })
            }
            task()
            timers.push(setInterval(task, 60))
          })
        ).finally(() => {
          const channels = Array.from(cmap.values())
          channels.sort(ChannelAPI.compareChannels)
          usersKV.setItem<ChannelBasicInfo[]>(`${this.id}:myChannels`, channels)
        })
      }

      return () => {
        stopped = true
        timers.forEach((t) => clearInterval(t))
      }
    })
  }

  async loadProfile(
    user: Principal | string
  ): Promise<Readable<UserInfo & ProfileInfo>> {
    const now = Date.now()
    let info = await this.getCacheUserInfo(now, user)
    if (!info) {
      info =
        typeof user == 'string'
          ? await this.api.get_by_username(user)
          : await this.api.get_user(user)
      await this.setCacheUserInfo(now, info)
    }

    const profile = await usersKV.getItem<ProfileInfo>(
      `profiles:${info.profile_canister.toText()}`
    )

    const api = await this.api.profileAPI(info.profile_canister)
    return readable(
      { ...info, ...profile } as UserInfo & ProfileInfo,
      (set) => {
        api.get_profile(info.id).then(async (profile) => {
          await usersKV.setItem<ProfileInfo>(
            `profiles:${info.profile_canister.toText()}`,
            profile
          )
          set({ ...info, ...profile })
        })
      }
    )
  }

  async loadChannelInfo(
    canister: Principal,
    id: number
  ): Promise<Readable<null | ChannelInfo | Error>> {
    const key = `${canister.toText()}:${id}:info`
    const info = await channelsKV.getItem<ChannelInfo>(key)
    const api = await this.api.channelAPI(canister)
    return readable(info as null | ChannelInfo | Error, (set) => {
      const timers: any[] = []
      let updated_at = info ? info.updated_at : 0n
      const task = () => {
        api.get_channel_if_update(id, updated_at).then(async (channel) => {
          if (channel) {
            if (updated_at < channel.updated_at) {
              updated_at = channel.updated_at
            }

            await channelsKV.setItem<ChannelInfo>(key, channel)
            set(channel)
          } else if (!info) {
            set(new Error('Channel not found'))
          }
        })
      }

      task()
      timers.push(setInterval(task, 60))
      return () => {
        timers.forEach((t) => clearInterval(t))
      }
    })
  }

  async loadLatestMessage(
    canister: Principal,
    channelId: number,
    messageId: number
  ): Promise<Readable<Message | null>> {
    const api = await this.api.channelAPI(canister)
    const keyPrefix = `${canister.toText()}:${channelId}:`
    let message = await channelsKV.getItem<Message>(keyPrefix + messageId)
    let latestMessageId = message ? messageId + 1 : messageId

    return readable(message, (set) => {
      const timers: any[] = []
      const task = () => {
        api.list_messages(channelId, latestMessageId).then(async (msgs) => {
          if (msgs.length > 0) {
            for (const msg of msgs) {
              await channelsKV.setItem<Message>(keyPrefix + msg.id, msg)
              set(msg)
            }
          }
        })
      }

      task()
      timers.push(setInterval(task, 10))
      return () => {
        timers.forEach((t) => clearInterval(t))
      }
    })
  }

  async loadPrevMessages(
    canister: Principal,
    channelId: number,
    end: number // not included
  ): Promise<Message[]> {
    const api = await this.api.channelAPI(canister)
    const keyPrefix = `${canister.toText()}:${channelId}:`
    let start = end - 20
    if (start < 1) {
      start = 1
    }
    let messages: Message[] = []
    while (start < end) {
      let message = await channelsKV.getItem<Message>(keyPrefix + start)
      if (!message) {
        break
      }
      start += 1
      messages.push(message)
    }

    if (start < end) {
      let msgs = await api.list_messages(channelId, start, end)
      for (const m of msgs) {
        await channelsKV.setItem<Message>(keyPrefix + m.id, m)
        messages.push(m)
      }
    }

    return messages
  }

  async batchLoadUsersInfo(ids: Principal[]): Promise<UserInfo[]> {
    const rt: UserInfo[] = []
    const todo: Principal[] = []
    const now = Date.now()
    for (const id of ids) {
      const info = await this.getCacheUserInfo(now, id)
      if (info) {
        rt.push(info)
      } else {
        todo.push(id)
      }
    }
    const users = await this.api.batch_get_users(todo)
    for (const info of users) {
      rt.push(info)
      await this.setCacheUserInfo(now, info)
    }
    return rt
  }

  private async getCacheUserInfo(
    now: number,
    user: Principal | string
  ): Promise<UserInfo | null> {
    const key = typeof user == 'string' ? user : user.toText()
    let [ts, info] = (await usersKV.getItem<[number, UserInfo]>(
      `users:${key}`
    )) || [0, null]
    return info && now - ts < cacheExp ? info : null
  }

  private async setCacheUserInfo(now: number, info: UserInfo) {
    await usersKV.setItem<[number, UserInfo]>(`users:${info.id.toText()}`, [
      now,
      info
    ])

    if (info.username.length == 1) {
      await usersKV.setItem<[number, UserInfo]>(`users:${info.username[0]}`, [
        now,
        info
      ])
    }
  }
}

interface MasterKeyInfo {
  kind: 'Local' | 'ECDH' | 'VetKey'
  keyId: Uint8Array
  passwordSecrect: Uint8Array | null
  encryptedSecret: Uint8Array
  passwordExpireAt: number
}

export class MasterKey {
  readonly kind: 'Local' | 'ECDH' | 'VetKey'
  readonly keyId: Uint8Array
  private readonly aad: Uint8Array
  private readonly encryptedSecret: Uint8Array
  private passwordExpireAt: number
  private passwordSecrect: Uint8Array | null = null
  private secret: Uint8Array | null = null

  static async from(
    user: Principal,
    kind: 'Local' | 'ECDH' | 'VetKey',
    keyId: Uint8Array,
    password: string,
    salt: string,
    remoteSecret: Uint8Array,
    passwordExpireAt: number
  ): Promise<MasterKey> {
    const aad = user.toUint8Array()
    const passwordSecrect = hashPassword(password, salt)
    const secret = deriveA256GCMSecret(passwordSecrect, remoteSecret)
    const key = AesGcmKey.fromSecret(secret, keyId)
    const encryptedSecret = await coseA256GCMEncrypt0(
      AesGcmKey.fromSecret(passwordSecrect),
      key.toBytes(),
      aad
    )
    const mk = new MasterKey(
      kind,
      keyId,
      encryptedSecret,
      aad,
      passwordExpireAt
    )
    mk.passwordSecrect = passwordSecrect
    mk.secret = secret
    return mk
  }

  static async fromInfo(
    user: Principal,
    info: MasterKeyInfo
  ): Promise<MasterKey> {
    const mk = new MasterKey(
      info.kind,
      info.keyId,
      info.encryptedSecret,
      user.toUint8Array(),
      info.passwordExpireAt
    )
    if (info.passwordSecrect && info.passwordExpireAt >= Date.now()) {
      await mk._open(info.passwordSecrect)
    }

    return mk
  }

  constructor(
    kind: 'Local' | 'ECDH' | 'VetKey',
    keyId: Uint8Array,
    encryptedSecret: Uint8Array,
    aad: Uint8Array,
    passwordExpireAt: number
  ) {
    this.kind = kind
    this.keyId = keyId
    this.aad = aad
    this.encryptedSecret = encryptedSecret
    this.passwordExpireAt = passwordExpireAt
  }

  async open(
    password: string,
    salt: string,
    passwordExpire: number
  ): Promise<void> {
    this.passwordExpireAt = passwordExpire > 0 ? passwordExpire + Date.now() : 0
    return this._open(hashPassword(password, salt))
  }

  private async _open(passwordSecrect: Uint8Array): Promise<void> {
    this.passwordSecrect = passwordSecrect
    const data = await coseA256GCMDecrypt0(
      AesGcmKey.fromSecret(this.passwordSecrect),
      this.encryptedSecret,
      this.aad
    )
    const key = AesGcmKey.fromBytes(data)
    this.secret = key.getSecretKey()
  }

  isOpened(): boolean {
    return this.secret != null
  }
  cachedPassword(): boolean {
    return this.passwordExpireAt > 0
  }

  isUser(user: Principal): boolean {
    return compareBytes(this.aad, user.toUint8Array()) === 0
  }

  toInfo(): MasterKeyInfo {
    return {
      kind: this.kind,
      keyId: this.keyId,
      passwordSecrect:
        this.passwordExpireAt > Date.now() ? this.passwordSecrect : null,
      encryptedSecret: this.encryptedSecret,
      passwordExpireAt: this.passwordExpireAt
    }
  }

  toA256GCMKey(): AesGcmKey {
    if (!this.secret) {
      throw new Error('master key is not opened')
    }

    return AesGcmKey.fromSecret(this.secret, this.keyId)
  }

  async generateChannelKey(
    channelCanister: Principal,
    ecdhPub?: Uint8Array
  ): Promise<{
    dek: Uint8Array
    kek: Uint8Array
    ecdh: ChannelECDHInput | null
  }> {
    const mk = this.toA256GCMKey()
    const dek = AesGcmKey.fromSecret(randomBytes(32))
    const kek = AesGcmKey.fromSecret(randomBytes(32))
    const encryptedDEK = await coseA256GCMEncrypt0(
      kek,
      dek.toBytes(),
      channelCanister.toUint8Array()
    )
    const encryptedKEK = await coseA256GCMEncrypt0(
      mk,
      kek.toBytes(),
      channelCanister.toUint8Array()
    )

    let ecdhInput: ChannelECDHInput | null = null
    if (ecdhPub) {
      ecdhInput = await exchangeSecret(
        ecdhPub,
        kek.toBytes(),
        channelCanister.toUint8Array()
      )
    }

    return {
      dek: encryptedDEK,
      kek: encryptedKEK,
      ecdh: ecdhInput
    }
  }
}

const myMessageStateStore = asyncFactory((identity) =>
  MyMessageState.with(identity)
)

export const myMessageStateAsync = async () =>
  (await myMessageStateStore).async()

async function exchangeSecret(
  remotePub: Uint8Array,
  payload: Uint8Array,
  aad: Uint8Array
): Promise<ChannelECDHInput> {
  const key = generateECDHKey()
  const secret = key.ecdh(
    ECDHKey.fromPublic(iana.EllipticCurveX25519, remotePub)
  )
  const encrypted = await coseA256GCMEncrypt0(
    AesGcmKey.fromSecret(secret),
    payload,
    aad
  )
  return {
    ecdh_pub: [remotePub],
    ecdh_remote: [[key.getPublicKey(), encrypted]]
  }
}
