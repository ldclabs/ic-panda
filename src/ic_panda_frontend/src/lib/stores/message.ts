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
import {
  ProfileAPI,
  type ProfileInfo,
  type UpdateProfileInput
} from '$lib/canisters/messageprofile'
import { MESSAGE_CANISTER_ID } from '$lib/constants'
import { unwrapOption } from '$lib/types/result'
import {
  AesGcmKey,
  compareBytes,
  coseA256GCMDecrypt0,
  coseA256GCMEncrypt0,
  decodeCBOR,
  deriveA256GCMSecret,
  ECDHKey,
  encodeCBOR,
  generateECDHKey,
  hashPassword,
  iana,
  randomBytes,
  utf8ToBytes
} from '$lib/utils/crypto'
import { KVStore } from '$lib/utils/store'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { derived, readable, type Readable } from 'svelte/store'
import { asyncFactory } from './auth'

const KVS = new KVStore('ICPanda', 1, [
  ['My'],
  ['Keys'],
  ['Users'],
  [
    'Profiles',
    {
      keyPath: '_id'
    }
  ],
  [
    'Channels',
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

const cacheExp = 2 * 24 * 3600 * 1000

export function getCurrentTimestamp(ts: bigint): string {
  return new Date(Number(ts)).toLocaleString()
}

export interface MessageInfo {
  id: number
  reply_to: number
  kind: number
  created_by: Principal
  created_time: string
  created_user: DisplayUserInfo
  canister: Principal
  message: string
  error: string
  src?: Message
}

export interface DisplayUserInfo {
  id: string
  username: string
  name: string
  image: string
}

export class MyMessageState {
  readonly principal: Principal
  readonly id: string
  readonly api: MessageCanisterAPI
  readonly coseAPI: CoseAPI | null = null
  readonly profileAPI: ProfileAPI | null = null
  readonly info: Readable<UserInfo>

  private _mk: MasterKey | null = null
  private _ek: ECDHKey | null = null
  private _myChannels = new Map<string, ChannelBasicInfo>()
  private _channelDEKs = new Map<String, AesGcmKey>()

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
        this.setCacheUserInfo(Date.now(), $info)
        set($info)
      } else {
        KVS.get<UserInfo>('Users', this.id).then((info) => {
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
      this._mk = null
      const info = await KVS.get<MasterKeyInfo>('Keys', `${this.id}:MK`)
      if (info) {
        this._mk = await MasterKey.fromInfo(this.principal, info)
      }
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

    await KVS.set('Keys', this._mk.toInfo(), `${this.id}:MK`)
  }

  async fetchECDHCoseEncryptedKey(keyId: Uint8Array): Promise<AesGcmKey> {
    if (!this.coseAPI) {
      throw new Error('COSE API not available')
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

  async initStaticECDHKey(): Promise<void> {
    const mk = (await this.masterKey())?.toA256GCMKey()
    if (!mk) {
      throw new Error('Master key not found')
    }

    const aad = this.principal.toUint8Array()
    try {
      const encrypted0 = await this.loadStaticECDHKey()
      const data = await coseA256GCMDecrypt0(mk, encrypted0, aad)
      this._ek = ECDHKey.fromBytes(data)
    } catch (e) {
      this._ek = generateECDHKey()
      this._ek.setKid(encodeCBOR(String(Date.now())))
      const encrypted0 = await coseA256GCMEncrypt0(
        mk,
        this._ek.toBytes(),
        aad,
        mk.kid
      )
      await this.saveStaticECDHKey(this._ek.getPublicKey(), encrypted0)
    }
  }

  async loadStaticECDHKey(): Promise<Uint8Array> {
    const encrypted0 = await KVS.get<Uint8Array>('Keys', `${this.id}:EK`)

    if (encrypted0) {
      return encrypted0
    }

    if (!this.coseAPI) {
      throw new Error('COSE API not available')
    }

    const ns = this.id.replaceAll('-', '_')
    const output = await this.coseAPI.setting_get({
      ns,
      key: utf8ToBytes('StaticECDH'),
      subject: [this.principal],
      version: 0,
      user_owned: false
    })

    if (output.dek.length != 1) {
      throw new Error('Static ECDH not found')
    }

    await KVS.set<Uint8Array>(
      'Keys',
      output.dek[0] as Uint8Array,
      `${this.id}:EK`
    )

    return output.dek[0] as Uint8Array
  }

  async saveStaticECDHKey(
    ecdh_pub: Uint8Array,
    encrypted0: Uint8Array
  ): Promise<void> {
    await KVS.set<Uint8Array>('Keys', encrypted0 as Uint8Array, `${this.id}:EK`)

    if (this.coseAPI) {
      await this.api.update_my_ecdh(ecdh_pub, encrypted0)
    }
  }

  async decryptChannelDEK(
    canister: Principal,
    id: number,
    encrypted0: Uint8Array
  ): Promise<AesGcmKey> {
    let dek = this._channelDEKs.get(`${canister.toText()}:${id}`)
    console.log('decryptChannelDEK', dek, encrypted0)
    if (!dek) {
      const mk = (await this.masterKey())?.toA256GCMKey()
      if (!mk) {
        throw new Error('Master key not found')
      }
      const aad = new Uint8Array()
      let data = await this.loadChannelKEK(canister, id)
      data = await coseA256GCMDecrypt0(mk, data, aad)
      const kek = AesGcmKey.fromBytes(data)
      data = await coseA256GCMDecrypt0(kek, encrypted0, aad)
      dek = AesGcmKey.fromBytes(data)
      this._channelDEKs.set(`${canister.toText()}:${id}`, dek)
    }

    return dek
  }

  async loadChannelKEK(canister: Principal, id: number): Promise<Uint8Array> {
    const encrypted0 = await KVS.get<Uint8Array>(
      'Keys',
      `${this.id}:${canister.toText()}:${id}:KEK`
    )

    if (encrypted0) {
      return encrypted0
    }

    if (!this.coseAPI) {
      throw new Error('COSE API not available')
    }

    const ns = this.id.replaceAll('-', '_')
    const output = await this.coseAPI.setting_get({
      ns,
      key: encodeCBOR([canister.toUint8Array(), id]),
      subject: [this.principal],
      version: 0,
      user_owned: false
    })
    console.log('setting', output)

    if (output.dek.length != 1) {
      throw new Error('Channel KEK not found')
    }

    await KVS.set<Uint8Array>(
      'Keys',
      output.dek[0] as Uint8Array,
      `${this.id}:${canister.toText()}:${id}:KEK`
    )

    return output.dek[0] as Uint8Array
  }

  async saveChannelKEK(
    canister: Principal,
    id: number,
    encrypted0: Uint8Array
  ): Promise<void> {
    await KVS.set<Uint8Array>(
      'Keys',
      encrypted0 as Uint8Array,
      `${this.id}:${canister.toText()}:${id}:KEK`
    )

    if (this.coseAPI) {
      await this.api.save_channel_kek({
        id: id,
        canister: canister,
        kek: encrypted0
      })
    }
  }

  private async initMyChannels(): Promise<void> {
    if (this._myChannels.size == 0) {
      const channels =
        (await KVS.get<ChannelBasicInfo[]>('My', `${this.id}:Channels`)) || []
      // channels.sort(ChannelAPI.compareChannels)
      for (const info of channels) {
        this._myChannels.set(`${info.canister.toText()}:${info.id}`, info)
      }
    }
  }

  async addMyChannel(info: ChannelBasicInfo | ChannelInfo): Promise<void> {
    await this.initMyChannels()

    this._myChannels.set(`${info.canister.toText()}:${info.id}`, {
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
    })

    const channels = Array.from(this._myChannels.values())
    channels.sort(ChannelAPI.compareChannels)
    await KVS.set<ChannelBasicInfo[]>('My', channels, `${this.id}:Channels`)
  }

  async removeMyChannel(canister: Principal, id: number): Promise<void> {
    await this.initMyChannels()

    this._myChannels.delete(`${canister.toText()}:${id}`)
    const channels = Array.from(this._myChannels.values())
    channels.sort(ChannelAPI.compareChannels)
    await KVS.set<ChannelBasicInfo[]>('My', channels, `${this.id}:Channels`)
  }

  async loadMyChannelsStream(): Promise<Readable<ChannelBasicInfo[]>> {
    const state = this.api.state
    if (!state) {
      throw new Error('Wait for message state')
    }

    await this.initMyChannels()
    const channels = Array.from(this._myChannels.values())
    channels.sort(ChannelAPI.compareChannels)

    return readable(channels, (set) => {
      let stopped = false
      const timers: any[] = []
      const ids: Principal[] = [
        ...state.channel_canisters,
        ...state.matured_channel_canisters
      ]

      Promise.all(
        ids.map(async (id) => {
          const api = await this.api.channelAPI(id)
          if (stopped) {
            return
          }

          let latest_message_at = 0n
          const task = () => {
            console.log('task: loadMyChannels', latest_message_at, id.toText())
            api.my_channels(latest_message_at).then((channels) => {
              if (channels.length > 0) {
                for (const info of channels) {
                  if (latest_message_at < info.latest_message_at) {
                    latest_message_at = info.latest_message_at
                  }

                  this._myChannels.set(
                    `${info.canister.toText()}:${info.id}`,
                    info
                  )
                }

                channels = Array.from(this._myChannels.values())
                channels.sort(ChannelAPI.compareChannels)
                KVS.set<ChannelBasicInfo[]>(
                  'My',
                  channels,
                  `${this.id}:Channels`
                )
                set(channels)
              }
            })
          }

          task()
          timers.push(setInterval(task, 60000))
        })
      )

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

    const profile = await KVS.get<ProfileInfo>(
      'Profiles',
      info.id.toUint8Array()
    )

    const api = await this.api.profileAPI(info.profile_canister)
    return readable(
      { ...info, ...profile } as UserInfo & ProfileInfo,
      (set) => {
        api.get_profile(info.id).then(async (profile) => {
          await KVS.set<ProfileInfo>('Profiles', profile)
          set({ ...info, ...profile })
        })
      }
    )
  }

  async updateProfile(
    profile_canister: Principal,
    input: UpdateProfileInput
  ): Promise<void> {
    const api = await this.api.profileAPI(profile_canister)
    const profile = await api.update_profile(input)
    await KVS.set<ProfileInfo>('Profiles', profile)
  }

  async loadChannelInfo(
    canister: Principal,
    id: number
  ): Promise<Readable<ChannelInfo>> {
    const k = [canister.toUint8Array(), id]
    const api = await this.api.channelAPI(canister)
    let info = await KVS.get<ChannelInfo>('Channels', k)
    const refresh = !!info
    if (!info) {
      info = await api.get_channel_if_update(id, 0n)
      if (!info) {
        throw new Error('Channel not found')
      }

      await KVS.set<ChannelInfo>('Channels', info)
    }

    return readable(info, (set) => {
      if (refresh) {
        api.get_channel_if_update(id, info.updated_at).then(async (channel) => {
          if (channel) {
            await KVS.set<ChannelInfo>('Channels', channel)
            set(channel)
          }
        })
      }
    })
  }

  async loadLatestMessageStream(
    dek: AesGcmKey,
    canister: Principal,
    channelId: number,
    messageId: number
  ): Promise<Readable<MessageInfo | null>> {
    const api = await this.api.channelAPI(canister)
    let message = await KVS.get<Message>('Messages', [
      canister.toUint8Array(),
      channelId,
      messageId
    ])
    let latestMessageId = message ? messageId + 1 : messageId
    const info = (await this.messagesToInfo(dek, message))[0] || null

    return readable(info, (set) => {
      let stopped = false
      let timer: any = null
      const task = () => {
        api.list_messages(channelId, latestMessageId).then(async (msgs) => {
          if (msgs.length > 0) {
            for (const msg of msgs) {
              latestMessageId = msg.id + 1
              await KVS.set<Message>('Messages', msg)
            }
            const infos = await this.messagesToInfo(dek, msgs)

            for (const info of infos) {
              await Promise.resolve()
              set(info)
            }

            timer = !stopped && setTimeout(task, 3000)
          } else {
            timer = !stopped && setTimeout(task, 9000)
          }
        })
      }

      task()
      return () => {
        stopped = true
        clearTimeout(timer)
      }
    })
  }

  async loadPrevMessages(
    dek: AesGcmKey,
    canister: Principal,
    channelId: number,
    end: number // not included
  ): Promise<MessageInfo[]> {
    const api = await this.api.channelAPI(canister)
    const prefix = [canister.toUint8Array(), channelId]
    let start = end - 20
    if (start < 1) {
      start = 1
    }
    if (end === start) {
      return []
    }

    let messages: Message[] = []
    const iter = await KVS.iterate(
      'Messages',
      IDBKeyRange.bound([...prefix, start], [...prefix, end], false, true)
    )

    let i = start
    for await (const cursor of iter) {
      if (cursor.key !== i) {
        break
      }
      i += 1
      messages.push(cursor.value)
    }

    if (i < end) {
      let items = await api.list_messages(channelId, i, end)
      await KVS.add<Message>('Messages', items)
      messages = [...messages, ...items]
    }

    return await this.messagesToInfo(dek, messages)
  }

  async messagesToInfo(
    dek: AesGcmKey,
    msgs: Message | Message[] | null
  ): Promise<MessageInfo[]> {
    if (!msgs) {
      return []
    }
    if (!Array.isArray(msgs)) {
      msgs = [msgs]
    }

    const aad = new Uint8Array()
    const usersMap = new Map<String, UserInfo>()
    const users = await this.batchLoadUsersInfo(msgs.map((m) => m.created_by))
    for (const info of users) {
      usersMap.set(info.id.toText(), info)
    }
    const list = []
    for (const msg of msgs) {
      const info = usersMap.get(msg.created_by.toText())
      const m = {
        id: msg.id,
        reply_to: msg.reply_to,
        kind: msg.kind,
        created_by: msg.created_by,
        created_time: getCurrentTimestamp(msg.created_at),
        created_user: toDisplayUserInfo(info),
        canister: msg.canister,
        message: '',
        error: '',
        src: msg
      }

      try {
        const payload =
          msg.kind == 1
            ? (msg.payload as Uint8Array)
            : await coseA256GCMDecrypt0(dek, msg.payload as Uint8Array, aad)
        m.message = decodeCBOR(payload)
      } catch (e) {
        m.error = 'Failed to decrypt message'
      }

      list.push(m)
    }

    return list
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
    const k = typeof user == 'string' ? user : user.toText()
    if (k === MESSAGE_CANISTER_ID) {
      return {
        id: user as Principal,
        username: ['_'],
        cose_canister: [],
        name: 'System',
        image: '',
        profile_canister: user as Principal
      }
    }

    let [ts, info] = (await KVS.get<[number, UserInfo]>('Users', k)) || [
      0,
      null
    ]
    return info && now - ts < cacheExp ? info : null
  }

  private async setCacheUserInfo(now: number, info: UserInfo) {
    await KVS.set<[number, UserInfo]>('Users', [now, info], info.id.toText())

    if (info.username.length == 1) {
      await KVS.set<[number, UserInfo]>('Users', [now, info], info.username[0])
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
    managers: Array<[Principal, Uint8Array | null]> = []
  ): Promise<{
    dek: Uint8Array
    kek: Uint8Array
    managers: Array<[Principal, ChannelECDHInput]>
  }> {
    const mk = this.toA256GCMKey()
    const aad = new Uint8Array()
    const dek = AesGcmKey.fromSecret(randomBytes(32))
    const kek = AesGcmKey.fromSecret(randomBytes(32))
    const kekBytes = kek.toBytes()
    const encryptedDEK = await coseA256GCMEncrypt0(kek, dek.toBytes(), aad)
    const encryptedKEK = await coseA256GCMEncrypt0(mk, kekBytes, aad)

    const out: [Principal, ChannelECDHInput][] = await Promise.all(
      managers.map(async ([id, ecdhPub]) => {
        if (ecdhPub) {
          const ecdhInput = await exchangeSecret(ecdhPub, kekBytes, aad)
          return [id, ecdhInput]
        } else {
          return [
            id,
            {
              ecdh_remote: [],
              ecdh_pub: []
            } as ChannelECDHInput
          ]
        }
      })
    )

    return {
      dek: encryptedDEK,
      kek: encryptedKEK,
      managers: out
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

export function toDisplayUserInfo(info?: UserInfo) {
  if (!info) {
    return {
      id: '',
      username: '',
      name: 'Unknown',
      image: ''
    }
  }

  const id = info.id.toText()
  return {
    id,
    username: unwrapOption(info.username) || '',
    name: info.name || 'Unknown',
    image: info.image
  }
}
