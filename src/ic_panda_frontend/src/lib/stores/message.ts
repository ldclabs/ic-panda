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
  type ChannelSetting,
  type Message
} from '$lib/canisters/messagechannel'
import {
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
  hmac3_256,
  iana,
  randomBytes,
  utf8ToBytes
} from '$lib/utils/crypto'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import {
  derived,
  readable,
  readonly,
  writable,
  type Readable
} from 'svelte/store'
import { asyncFactory } from './auth'
import {
  KEY_REFRESH_MY_CHANNELS_AT,
  KVS,
  myChannelKey,
  myChannelsRange
} from './kvstore'

const usersCacheExp = 2 * 3600 * 1000
const PWD_HASH_KEY = 'pwd_hash'
const KEY_ID = encodeCBOR('ICPanda_Messages_Master_Key')

export function getCurrentTimeString(ts: bigint): string {
  const now = Date.now()
  const t = Number(ts)
  if (t >= now - 24 * 3600 * 1000) {
    return new Date(t).toLocaleTimeString()
  } else if (t >= now - 7 * 24 * 3600 * 1000) {
    return new Date(t).toLocaleDateString(undefined, { weekday: 'long' })
  }
  return new Date(t).toLocaleDateString()
}

export function mergeMySetting(
  old: ChannelSetting,
  ncs: Partial<ChannelSetting>
): ChannelSetting {
  const lastRead = old.last_read
  const rt = { ...old, ...ncs }
  if (rt.last_read < lastRead) {
    rt.last_read = lastRead
    rt.unread = ncs.unread ? ncs.unread : rt.unread - (lastRead - rt.last_read)
    if (rt.unread < 0) {
      rt.unread = 0
    }
  }

  return rt
}

type MessageCacheInfo = Message & { canister: Principal; channel: number }

export interface MessageInfo {
  id: number
  reply_to: number
  kind: number
  created_by: Principal
  created_time: string
  created_user: DisplayUserInfo
  canister: Principal
  channel: number
  message: string
  error: string
  src?: Message
}

export interface DisplayUserInfo {
  _id: string
  username: string
  name: string
  image: string
  src?: UserInfo
}

// 0: none, 1: request key, 2: key filled
export type ECDHRequestStatus = 0 | 1 | 2

export type DisplayUserInfoEx = DisplayUserInfo & {
  is_manager: boolean
  ecdh_request: ECDHRequestStatus
}

export type ChannelBasicInfoEx = ChannelBasicInfo & {
  channelId: string
  latest_message_time: string
  latest_message_user: DisplayUserInfo
}

type ChannelModel = ChannelInfo & { _get_by: string }

export type ChannelInfoEx = ChannelInfo & {
  _kek: Uint8Array | null
  _managers: string[]
}

export class MyMessageState {
  readonly id: string
  readonly principal: Principal
  readonly api: MessageCanisterAPI
  readonly info: Readable<UserInfo | null>

  private _coseAPI: CoseAPI | null = null
  private _mks: MasterKey[] = []
  private _ek: ECDHKey | null = null
  private _myInfo: UserInfo | null = null
  private _myProfile: ProfileInfo | null = null
  private _myChannels = new Map<string, ChannelBasicInfo>() // keep the latest channel setting
  private _myChannelsStream = writable<ChannelBasicInfo[]>([])
  private _channelDEKs = new Map<String, AesGcmKey>()

  static async with(identity: Identity): Promise<MyMessageState> {
    const api = await messageCanisterAPIAsync()
    const self = new MyMessageState(identity.getPrincipal(), api)
    self.refreshAllState(false)
    return self
  }

  constructor(principal: Principal, api: MessageCanisterAPI) {
    this.principal = principal
    this.id = principal.toText()
    this.api = api
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

  masterKeyKind(): 'Local' | 'ECDH' | 'VetKey' {
    return this._coseAPI != null ? 'ECDH' : 'Local'
  }

  async refreshAllState(force: boolean = true): Promise<void> {
    if (force) {
      await Promise.all([this.api.refreshMyInfo(), this.api.refreshState()])
    }
    const now = Date.now()
    this._myInfo = this.api.myInfo
    if (!this._myInfo) {
      this._myInfo = await this.getCacheUserInfo(now, this.principal)
    }
    if (this._myInfo) {
      await this.setCacheUserInfo(now, this._myInfo)

      const profileAPI = await this.api.profileAPI(
        this._myInfo.profile_canister
      )
      await profileAPI.refreshMyProfile()
      this._myProfile = profileAPI.myProfile
      if (!this._myProfile) {
        this._myProfile = await KVS.get<ProfileInfo>('My', `${this.id}:Profile`)
      } else {
        await KVS.set<ProfileInfo>('My', this._myProfile, `${this.id}:Profile`)
      }

      if (this._myInfo.cose_canister.length == 1) {
        this._coseAPI = await this.api.coseAPI(this._myInfo.cose_canister[0])
      }

      this.loadMyKV()
    }
  }

  async masterKey(): Promise<MasterKey | null> {
    const mk = this._mks.at(-1)
    if (!mk || !mk.isUser(this.principal)) {
      let keys = (await KVS.get<MasterKeyInfo[]>('Keys', `${this.id}:MK`)) || []
      if (!Array.isArray(keys)) {
        keys = [keys]
      }
      this._mks = await Promise.all(
        keys.map((key) => MasterKey.fromInfo(this.principal, key))
      )
    }

    return this._mks.at(-1) || null
  }

  async mustMasterKey(): Promise<MasterKey> {
    const mk = await this.masterKey()
    if (!mk) {
      throw new Error('Master key not ready')
    }
    return mk
  }

  async mustStaticECDHKey(): Promise<ECDHKey> {
    if (!this._ek) {
      await this.initStaticECDHKey()
    }
    if (!this._ek) {
      throw new Error('ECDH key not ready')
    }
    return this._ek
  }

  async setMasterKey(
    kind: 'Local' | 'ECDH' | 'VetKey',
    password: string,
    remoteSecret: Uint8Array,
    passwordExpire: number // milliseconds
  ): Promise<MasterKey> {
    if (this._mks.length > 10) {
      throw new Error('Too many master keys')
    }

    const pwdHash = await this.getPasswordHash()
    const mk = await MasterKey.from(
      this.principal,
      kind,
      pwdHash,
      password,
      this.id,
      remoteSecret,
      passwordExpire > 0 ? passwordExpire + Date.now() : 0
    )

    this._mks.push(mk)

    await KVS.set(
      'Keys',
      this._mks.map((k) => k.toInfo()),
      `${this.id}:MK`
    )
    return mk
  }

  async migrateKeys(): Promise<void> {
    const mk = await this.mustMasterKey()
    if (mk.kind != 'Local') {
      throw new Error('Master key kind not supported')
    }
    if (!mk.isOpened()) {
      throw new Error('Master key not opened')
    }

    const state = this.api.state
    if (!state) {
      throw new Error('Message state not ready')
    }
    if (!this._myInfo) {
      throw new Error('User info not ready')
    }
    const cose_canister = this._myInfo.cose_canister[0]
    if (!cose_canister) {
      throw new Error('username not ready')
    }

    // save local keys to COSE after username is set
    this._coseAPI = await this.api.coseAPI(cose_canister)
    const aad = new Uint8Array()
    const remoteMK = await this.fetchECDHCoseEncryptedKey()
    const newMK = await mk.toNewMasterKey(
      'ECDH',
      KEY_ID,
      remoteMK.getSecretKey()
    )
    await this.initMyChannels()
    const channels = Array.from(this._myChannels.values())
    const mKey = mk.toA256GCMKey()
    const nmKey = newMK.toA256GCMKey()
    for (const ch of channels) {
      const encrypted0 = await await this.loadChannelKEK(
        ch.canister,
        ch.id
      ).catch((e) => null)
      if (!encrypted0) {
        continue
      }

      try {
        let data = await coseA256GCMDecrypt0(mKey, encrypted0, aad)
        data = await coseA256GCMEncrypt0(nmKey, data, aad)
        await this.saveChannelKEK(ch.canister, ch.id, data)
      } catch (err) {
        console.error('migrateKeys', err)
      }
    }

    this._mks.push(newMK)
    await KVS.set(
      'Keys',
      this._mks.map((k) => k.toInfo()),
      `${this.id}:MK`
    )

    await this.initStaticECDHKey()
  }

  async fetchECDHCoseEncryptedKey(): Promise<AesGcmKey> {
    if (!this._coseAPI) {
      throw new Error('COSE API not ready')
    }

    const ns = this.id.replaceAll('-', '_')
    const aad = this.principal.toUint8Array()
    const ecdh = generateECDHKey()
    const output = await this._coseAPI.ecdh_cose_encrypted_key(
      {
        ns,
        key: KEY_ID,
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
    const mk = (await this.mustMasterKey()).toA256GCMKey()
    const aad = this.principal.toUint8Array()
    try {
      const encrypted0 = await this.loadStaticECDHKey()
      const data = await coseA256GCMDecrypt0(mk, encrypted0, aad)
      this._ek = ECDHKey.fromBytes(data)
    } catch (err) {
      console.error('initStaticECDHKey', err)
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

    if (!this._coseAPI) {
      throw new Error('COSE API not available')
    }

    const ns = this.id.replaceAll('-', '_')
    const output = await this._coseAPI.setting_get({
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

    if (this._coseAPI) {
      await this.api.update_my_ecdh(ecdh_pub, encrypted0)
    }
  }

  async loadMyKV(): Promise<Map<string, Uint8Array>> {
    let kv = await KVS.get<Map<string, Uint8Array>>('My', `${this.id}:KV`)

    if (kv) {
      return kv
    }

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

      if (output?.payload.length === 1) {
        kv = decodeCBOR(output.payload[0] as Uint8Array)
      }

      if (kv) {
        await KVS.set<Map<string, Uint8Array>>('My', kv, `${this.id}:KV`)
      }
    }

    return kv || new Map<string, Uint8Array>()
  }

  async getPasswordHash(): Promise<Uint8Array | null> {
    const kv = await this.loadMyKV().catch((e) => null)
    return kv?.get(PWD_HASH_KEY) || null
  }

  async savePasswordHash(hash: Uint8Array): Promise<void> {
    if (this._coseAPI) {
      await this.api.update_my_kv({
        upsert_kv: [[PWD_HASH_KEY, hash]],
        remove_kv: []
      })
    }
  }

  async decryptChannelDEK(info: ChannelInfoEx): Promise<AesGcmKey> {
    let dek = this._channelDEKs.get(`${info.canister.toText()}:${info.id}`)
    if (!dek) {
      if (!info._kek) {
        throw new Error('Channel KEK not ready')
      }

      const mk = (await this.mustMasterKey()).toA256GCMKey()
      const aad = new Uint8Array()
      let data = await coseA256GCMDecrypt0(mk, info._kek, aad)
      const kek = AesGcmKey.fromBytes(data)
      data = await coseA256GCMDecrypt0(kek, info.dek as Uint8Array, aad)
      dek = AesGcmKey.fromBytes(data)
      this._channelDEKs.set(`${info.canister.toText()}:${info.id}`, dek)
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

    if (!this._coseAPI) {
      throw new Error('COSE API not available')
    }

    const ns = this.id.replaceAll('-', '_')
    const output = await this._coseAPI.setting_get({
      ns,
      key: encodeCBOR([canister.toUint8Array(), id]),
      subject: [this.principal],
      version: 0,
      user_owned: false
    })

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

    if (this._coseAPI) {
      await this.api.save_channel_kek({
        id: id,
        canister: canister,
        kek: encrypted0
      })
    }
  }

  async requestKEK(info: ChannelInfo): Promise<void> {
    const pub = unwrapOption(info.my_setting.ecdh_pub) as Uint8Array
    const remote = unwrapOption(info.my_setting.ecdh_remote) as [
      Uint8Array,
      Uint8Array
    ]
    if (pub || remote) {
      throw new Error('KEK exchange parameters exist')
    }

    const ek = await this.mustStaticECDHKey()
    const api = await this.api.channelAPI(info.canister)
    const setting = await api.update_my_setting({
      id: info.id,
      ecdh: [{ ecdh_pub: [ek.getPublicKey()], ecdh_remote: [] }],
      mute: [],
      last_read: []
    })
    this.freshMyChannelSetting(info.canister, info.id, setting)
  }

  async acceptKEK(info: ChannelInfoEx): Promise<void> {
    const pub = unwrapOption(info.my_setting.ecdh_pub) as Uint8Array
    const remote = unwrapOption(info.my_setting.ecdh_remote) as [
      Uint8Array,
      Uint8Array
    ]
    if (!pub || !remote) {
      throw new Error('Invalid KEK exchange parameters')
    }

    const mk = (await this.mustMasterKey()).toA256GCMKey()
    const ek = await this.mustStaticECDHKey()
    if (compareBytes(ek.getPublicKey(), pub) !== 0) {
      throw new Error('ECDH public key not match')
    }

    const aad = new Uint8Array()
    const secret = ek.ecdh(
      ECDHKey.fromPublic(iana.EllipticCurveX25519, remote[0])
    )

    const payload = await coseA256GCMDecrypt0(
      AesGcmKey.fromSecret(secret),
      remote[1],
      aad
    )

    const encryptedKEK = await coseA256GCMEncrypt0(mk, payload, aad)
    await this.saveChannelKEK(info.canister, info.id, encryptedKEK)

    const api = await this.api.channelAPI(info.canister)
    const setting = await api.update_my_setting({
      id: info.id,
      ecdh: [{ ecdh_pub: [], ecdh_remote: [] }],
      mute: [],
      last_read: []
    })

    this.freshMyChannelSetting(info.canister, info.id, setting)
    info._kek = encryptedKEK
  }

  async adminExchangeKEK(info: ChannelInfoEx): Promise<void> {
    if (info.ecdh_request.length == 0) {
      return
    }

    const mk = (await this.mustMasterKey()).toA256GCMKey()
    const kek = await this.loadChannelKEK(info.canister, info.id)
    const aad = new Uint8Array()
    const payload = await coseA256GCMDecrypt0(mk, kek, aad)
    const api = await this.api.channelAPI(info.canister)
    for (const [member, [pub, remote]] of info.ecdh_request) {
      if (remote.length > 0) {
        // exchange already done
        continue
      }

      const ecdhInput = await exchangeSecret(pub as Uint8Array, payload, aad)
      if (info._managers.includes(member.toText())) {
        await api.update_manager({
          id: info.id,
          member,
          ecdh: ecdhInput
        })
      } else {
        await api.update_member({
          id: info.id,
          member,
          ecdh: ecdhInput
        })
      }
    }
  }

  async adminAddMembers(
    info: ChannelInfoEx,
    kind: 'Manager' | 'Member',
    members: Array<[Principal, Uint8Array | null]>
  ): Promise<void> {
    if (members.length == 0) {
      return
    }

    const mk = (await this.mustMasterKey()).toA256GCMKey()
    const kek = await this.loadChannelKEK(info.canister, info.id)
    const aad = new Uint8Array()
    const payload = await coseA256GCMDecrypt0(mk, kek, aad)
    const api = await this.api.channelAPI(info.canister)
    for (const [member, pub] of members) {
      const input = {
        id: info.id,
        member,
        ecdh: {
          ecdh_remote: [],
          ecdh_pub: []
        } as ChannelECDHInput
      }
      if (pub) {
        input.ecdh = await exchangeSecret(pub, payload, aad)
      }
      if (kind == 'Manager') {
        await api.update_manager(input)
      } else {
        await api.update_member(input)
      }
    }
  }

  async channelMembers(
    channel: ChannelInfoEx,
    me: UserInfo
  ): Promise<DisplayUserInfoEx[]> {
    const ids = [...channel.managers, ...channel.members]
    const todo = new Set(ids.map((id) => id.toText()))
    const users = await this.batchLoadUsersInfo(ids)
    const ecdh_request = new Map<string, ECDHRequestStatus>()
    for (const [member, [_, remote]] of channel.ecdh_request) {
      ecdh_request.set(member.toText(), remote.length > 0 ? 2 : 1)
    }

    const myInfo = toDisplayUserInfo(me) as DisplayUserInfoEx
    myInfo.is_manager = channel._managers.includes(myInfo._id)
    myInfo.ecdh_request = ecdh_request.get(myInfo._id) || 0
    const members: DisplayUserInfoEx[] = []
    for (const user of users) {
      const info = toDisplayUserInfo(user) as DisplayUserInfoEx
      todo.delete(info._id)
      if (info._id != myInfo._id) {
        info.is_manager = channel._managers.includes(info._id)
        info.ecdh_request = ecdh_request.get(info._id) || 0
        members.push(info)
      }
    }
    members.sort((a, b) => {
      if (a.is_manager) {
        if (!b.is_manager) {
          return -1
        }
        return a.name.localeCompare(b.name)
      }
      if (b.is_manager) {
        return 1
      }
      return a.name.localeCompare(b.name)
    })
    members.unshift(myInfo)
    for (const id of todo) {
      members.push({
        _id: id,
        username: '',
        name: id,
        image: '',
        is_manager: false,
        ecdh_request: 0
      })
    }
    return members
  }

  private async initMyChannels(): Promise<void> {
    if (this._myChannels.size == 0) {
      const iter = await KVS.iterate('My', myChannelsRange(this.id))
      for await (const cursor of iter) {
        const info = cursor.value as ChannelBasicInfo
        this._myChannels.set(`${info.canister.toText()}:${info.id}`, info)
      }
    }
  }

  freshMyChannelSetting(
    canister: Principal,
    id: number,
    setting?: Partial<ChannelSetting>
  ): ChannelSetting | null {
    const channel = this._myChannels.get(`${canister.toText()}:${id}`)
    if (channel && setting) {
      channel.my_setting = mergeMySetting(channel.my_setting, setting)
      return channel.my_setting
    }

    return (setting as ChannelSetting) || null
  }

  informMyChannelsStream(): void {
    const channels = Array.from(this._myChannels.values())
    channels.sort(ChannelAPI.compareChannels)
    this._myChannelsStream.set(channels)
  }

  async addMyChannel(info: ChannelInfo): Promise<void> {
    await this.initMyChannels()

    info.my_setting = this.freshMyChannelSetting(
      info.canister,
      info.id,
      info.my_setting
    )!
    const basic: ChannelBasicInfo = {
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
    this._myChannels.set(`${info.canister.toText()}:${info.id}`, basic)

    await KVS.set<ChannelModel>('Channels', { ...info, _get_by: this.id })
    await KVS.set(
      'My',
      basic,
      myChannelKey(this.id, info.canister.toText(), info.id)
    )
    this.informMyChannelsStream()
  }

  async removeMyChannel(canister: Principal, id: number): Promise<void> {
    await this.initMyChannels()

    this._myChannels.delete(`${canister.toText()}:${id}`)

    const key = [canister.toUint8Array(), id]
    await KVS.delete('Channels', key)
    await KVS.delete('My', myChannelKey(this.id, canister.toText(), id))
    await KVS.delete(
      'Messages',
      IDBKeyRange.bound([...key, 0], [...key, 4294967295], false, true)
    )
    this.informMyChannelsStream()
  }

  async refreshMyChannel(info: ChannelInfoEx): Promise<ChannelInfoEx> {
    const api = await this.api.channelAPI(info.canister)
    const ninfo = (await api.get_channel_if_update(info.id, 0n)) as ChannelModel
    if (!ninfo) {
      throw new Error('Channel not found')
    }
    ninfo._get_by = this.id
    await this.addMyChannel(ninfo)
    let kek = info._kek
    if (!kek) {
      try {
        kek = await this.loadChannelKEK(info.canister, info.id)
      } catch (e) {}
    }

    return {
      ...ninfo,
      _kek: kek,
      _managers: ninfo.managers.map((m) => m.toText())
    }
  }

  async refreshMyChannels(signal: AbortSignal): Promise<void> {
    const state = this.api.state
    if (!state) {
      throw new Error('Wait for message state')
    }

    await this.initMyChannels()
    const canisters: Principal[] = [
      ...state.channel_canisters,
      ...state.matured_channel_canisters
    ]

    await await KVS.set<number>(
      'My',
      Date.now(),
      this.id + KEY_REFRESH_MY_CHANNELS_AT
    )
    await Promise.all(
      canisters.map(async (canister) => {
        const api = await this.api.channelAPI(canister)

        let latest_message_at = 0n
        const prefix = canister.toText()
        for (const [key, info] of this._myChannels) {
          if (key.startsWith(prefix)) {
            if (latest_message_at < info.latest_message_at) {
              latest_message_at = info.latest_message_at
            }
          }
        }

        if (signal.aborted) {
          return
        }

        let channels = await api.my_channels(latest_message_at)
        if (channels.length > 0) {
          for (const channel of channels) {
            channel.my_setting = this.freshMyChannelSetting(
              channel.canister,
              channel.id,
              channel.my_setting
            )!
            this._myChannels.set(`${prefix}:${channel.id}`, channel)
            await KVS.set(
              'My',
              channel,
              myChannelKey(this.id, prefix, channel.id)
            )
          }
        }

        await this.informMyChannelsStream()
      })
    )
  }

  async loadMyChannelsStream(): Promise<Readable<ChannelBasicInfoEx[]>> {
    await this.initMyChannels()

    const usersMap = new Map<String, UserInfo>()
    const channelMapfFn = (c: ChannelBasicInfo) => {
      const info = usersMap.get(c.latest_message_by.toText())
      return {
        ...c,
        channelId: ChannelAPI.channelParam(c),
        latest_message_time: getCurrentTimeString(c.latest_message_at),
        latest_message_user: toDisplayUserInfo(info)
      } as ChannelBasicInfoEx
    }

    const channels = Array.from(this._myChannels.values())
    channels.sort(ChannelAPI.compareChannels)

    return readonly(
      derived(
        this._myChannelsStream,
        (channels, set) => {
          this.batchLoadUsersInfo(
            channels.map((c) => c.latest_message_by)
          ).then(async (users) => {
            for (const info of users) {
              usersMap.set(info.id.toText(), info)
            }
            set(channels.map(channelMapfFn))
          })
        },
        channels.map(channelMapfFn)
      )
    )
  }

  async loadMyProfile(): Promise<UserInfo & ProfileInfo> {
    if (!this._myInfo) {
      throw new Error('User info not ready')
    }
    if (!this._myProfile) {
      throw new Error('Profile info not ready')
    }

    return { ...this._myInfo, ...this._myProfile }
  }

  async loadProfile(
    user: Principal | string
  ): Promise<Readable<UserInfo & ProfileInfo>> {
    if (!user) {
      throw new Error('Invalid Username')
    }

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

  async tryLoadProfile(
    user: Principal | string
  ): Promise<(UserInfo & ProfileInfo) | null> {
    if (user) {
      try {
        const now = Date.now()
        let info = await this.getCacheUserInfo(now, user)
        if (!info) {
          info =
            typeof user == 'string'
              ? await this.api.get_by_username(user)
              : await this.api.get_user(user)
          await this.setCacheUserInfo(now, info)
        }

        let profile = await KVS.get<ProfileInfo>(
          'Profiles',
          info.id.toUint8Array()
        )
        if (!profile) {
          const api = await this.api.profileAPI(info.profile_canister)
          profile = await api.get_profile(info.id)
          await KVS.set<ProfileInfo>('Profiles', profile)
        }

        return { ...info, ...profile }
      } catch (err) {}
    }

    return null
  }

  async tryFetchProfile(
    id: Principal
  ): Promise<(UserInfo & ProfileInfo) | null> {
    try {
      const now = Date.now()
      const user = await this.api.get_user(id)
      await this.setCacheUserInfo(now, user)
      const api = await this.api.profileAPI(user.profile_canister)
      const profile = await api.get_profile(user.id)
      await KVS.set<ProfileInfo>('Profiles', profile)

      return { ...user, ...profile }
    } catch (err) {}
    return null
  }

  async updateProfile(
    input: UpdateProfileInput
  ): Promise<UserInfo & ProfileInfo> {
    if (!this._myInfo) {
      throw new Error('My user info not ready')
    }

    const api = await this.api.profileAPI(this._myInfo.profile_canister)
    this._myProfile = await api.update_profile(input)
    await KVS.set<ProfileInfo>('My', this._myProfile, `${this.id}:Profile`)
    return { ...this._myInfo, ...this._myProfile }
  }

  async loadChannelInfo(
    canister: Principal,
    id: number
  ): Promise<Readable<ChannelInfoEx>> {
    const api = await this.api.channelAPI(canister)
    let info = await KVS.get<ChannelModel>('Channels', [
      canister.toUint8Array(),
      id
    ])

    if (info) {
      const isManager = info.managers.some(
        (m) => m.compareTo(this.principal) === 'eq'
      )

      if (
        !isManager &&
        !info.members.some((m) => m.compareTo(this.principal) === 'eq')
      ) {
        info = null
      }
    }

    let refresh = !!info
    if (!info) {
      info = (await api.get_channel_if_update(id, 0n)) as ChannelModel
      if (!info) {
        throw new Error('Channel not found')
      }

      info._get_by = this.id
      await this.addMyChannel(info)
    } else if (info._get_by != this.id) {
      refresh = true
      info.ecdh_request = []
      info.my_setting = {
        ecdh_pub: [],
        ecdh_remote: [],
        mute: false,
        last_read: 1,
        unread: 0,
        updated_at: 0n
      }
    } else {
      const channel = this._myChannels.get(`${canister.toText()}:${id}`)
      if (channel) {
        info.my_setting = mergeMySetting(info.my_setting, channel.my_setting)
      }
    }

    let kek: Uint8Array | null = null
    try {
      kek = await this.loadChannelKEK(info.canister, info.id)
    } catch (err) {}

    return readable<ChannelInfoEx>(
      {
        ...info,
        _kek: kek,
        _managers: info.managers.map((m) => m.toText())
      },
      (set) => {
        if (refresh) {
          api
            .get_channel_if_update(id, info.updated_at)
            .then(async (channel) => {
              if (channel) {
                if (!kek) {
                  try {
                    kek = await this.loadChannelKEK(info.canister, info.id)
                  } catch (err) {}
                }

                await this.addMyChannel(channel)
                set({
                  ...channel,
                  _kek: kek,
                  _managers: info.managers.map((m) => m.toText())
                })
              }
            })
        }
      }
    )
  }

  async loadLatestMessageStream(
    canister: Principal,
    channelId: number,
    dek: AesGcmKey,
    messageId: number,
    isActive: () => boolean
  ): Promise<Readable<MessageInfo | null>> {
    const api = await this.api.channelAPI(canister)
    let message = await KVS.get<MessageCacheInfo>('Messages', [
      canister.toUint8Array(),
      channelId,
      messageId
    ])
    let latestMessageId = message ? messageId + 1 : messageId
    const info =
      (await this.messagesToInfo(canister, channelId, dek, message))[0] || null

    return readable(info, (set) => {
      let stopped = false
      let timer: any = null
      const task = async () => {
        const msgs = isActive()
          ? await api.list_messages(channelId, latestMessageId)
          : []
        if (msgs.length > 0) {
          for (const msg of msgs) {
            latestMessageId = msg.id + 1
            await KVS.set<MessageCacheInfo>('Messages', {
              ...msg,
              canister,
              channel: channelId
            })
          }
          const infos = await this.messagesToInfo(
            canister,
            channelId,
            dek,
            msgs
          )

          for (const info of infos) {
            await Promise.resolve()
            set(info)
          }

          timer = !stopped && setTimeout(task, 3000)
        } else {
          timer = !stopped && setTimeout(task, 7000)
        }
      }

      task()
      return () => {
        stopped = true
        clearTimeout(timer)
      }
    })
  }

  async loadMessages(
    canister: Principal,
    channelId: number,
    dek: AesGcmKey,
    start: number, // maybe included
    end: number // not included
  ): Promise<MessageInfo[]> {
    const api = await this.api.channelAPI(canister)
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

    let messages: MessageCacheInfo[] = []
    const iter = await KVS.iterate(
      'Messages',
      IDBKeyRange.bound([...prefix, start], [...prefix, end], false, true)
    )

    let i = start
    for await (const cursor of iter) {
      if ((cursor.key as [Uint8Array, number, number])[2] !== i) {
        break
      }
      i += 1
      messages.push(cursor.value)
    }

    if (i < end) {
      let items = (await api.list_messages(
        channelId,
        i,
        end
      )) as MessageCacheInfo[]
      items = items.map((msg) => {
        return {
          ...msg,
          canister,
          channel: channelId
        }
      })

      await KVS.setMany<MessageCacheInfo>('Messages', items)
      messages = [...messages, ...items]
    }

    return await this.messagesToInfo(canister, channelId, dek, messages)
  }

  async updateMyLastRead(
    canister: Principal,
    channelId: number,
    lastRead: number
  ): Promise<void> {
    const api = await this.api.channelAPI(canister)
    const setting = await api.update_my_setting({
      id: channelId,
      ecdh: [],
      mute: [],
      last_read: [lastRead]
    })

    this.freshMyChannelSetting(canister, channelId, setting)
  }

  async messagesToInfo(
    canister: Principal,
    channelId: number,
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
        created_time: getCurrentTimeString(msg.created_at),
        created_user: toDisplayUserInfo(info),
        canister: canister,
        channel: channelId,
        message: '',
        error: '',
        src: msg
      }

      try {
        const payload =
          msg.kind == 1
            ? (msg.payload as Uint8Array)
            : await coseA256GCMDecrypt0(dek, msg.payload as Uint8Array, aad)
        m.message = decodeMessage(payload)
      } catch (err) {
        m.error = `Failed to decrypt message: ${err}`
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
    return info && now - ts < usersCacheExp ? info : null
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
    pwdHash: Uint8Array | null,
    password: string,
    salt: string,
    remoteSecret: Uint8Array,
    passwordExpireAt: number
  ): Promise<MasterKey> {
    const aad = user.toUint8Array()
    const passwordSecrect = hashPassword(password, salt)
    if (pwdHash) {
      const hash = hmac3_256(KEY_ID, passwordSecrect)
      if (compareBytes(pwdHash, hash) !== 0) {
        throw new Error('Invalid password')
      }
    }

    const secret = deriveA256GCMSecret(passwordSecrect, remoteSecret)
    const key = AesGcmKey.fromSecret(secret, KEY_ID)
    const encryptedSecret = await coseA256GCMEncrypt0(
      AesGcmKey.fromSecret(passwordSecrect),
      key.toBytes(),
      aad
    )

    const mk = new MasterKey(
      kind,
      KEY_ID,
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

  passwordHash(): Uint8Array {
    if (!this.passwordSecrect) {
      throw new Error('master key is not opened')
    }

    return hmac3_256(KEY_ID, this.passwordSecrect)
  }

  async toNewMasterKey(
    kind: 'ECDH' | 'VetKey',
    keyId: Uint8Array,
    remoteSecret: Uint8Array
  ): Promise<MasterKey> {
    if (!this.secret || !this.passwordSecrect) {
      throw new Error('master key is not opened')
    }

    const secret = deriveA256GCMSecret(this.passwordSecrect, remoteSecret)
    const key = AesGcmKey.fromSecret(secret, keyId)
    const encryptedSecret = await coseA256GCMEncrypt0(
      AesGcmKey.fromSecret(this.passwordSecrect),
      key.toBytes(),
      this.aad
    )
    const mk = new MasterKey(
      kind,
      keyId,
      encryptedSecret,
      this.aad,
      this.passwordExpireAt
    )
    mk.passwordSecrect = this.passwordSecrect
    mk.secret = secret
    return mk
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
      _id: '',
      username: '',
      name: 'Unknown',
      image: ''
    }
  }

  const _id = info.id.toText()
  return {
    _id,
    username: unwrapOption(info.username) || '',
    name: info.name || 'Unknown',
    image: info.image,
    src: info
  }
}

type MessagePayload = string | [string, number, Uint8Array]

function decodeMessage(payload: Uint8Array): string {
  const rt = decodeCBOR<MessagePayload>(payload)
  if (Array.isArray(rt)) {
    return rt[0]
  }
  return rt
}
