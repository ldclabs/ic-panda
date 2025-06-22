import { type SettingPath } from '$lib/canisters/cose'
import {
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
import { type ProfileInfo } from '$lib/canisters/messageprofile'
import { MASTER_KEY_ID } from '$lib/constants'
import { MessageDetail } from '$lib/types/message'
import { unwrapOption } from '$lib/types/result'
import { dynAgent } from '$lib/utils/auth'
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
  hmac3_256,
  iana,
  randomBytes,
  utf8ToBytes
} from '$lib/utils/crypto'
import { getCurrentTimeString } from '$lib/utils/helper'
import { Principal } from '@dfinity/principal'
import { derived, readable, type Readable } from 'svelte/store'
import { getProfile, getUser, setProfile, setUser } from './kvstore'
import { MessageAgent, type SyncAt } from './message_agent'

const PWD_HASH_KEY = 'pwd_hash'
const KEY_ID = encodeCBOR(MASTER_KEY_ID)

export const MAX_MESSAGE_ID = 0xffffffff

export function mergeMySetting(
  old: ChannelSetting,
  ncs: Partial<ChannelSetting>,
  lastRead: number
): ChannelSetting {
  const rt = { ...old, ...ncs }
  if (rt.last_read < old.last_read) {
    rt.last_read = old.last_read
  }
  rt.unread = ncs.unread
    ? ncs.unread
    : rt.unread - (rt.last_read - old.last_read)
  if (rt.unread < 0 || rt.last_read >= lastRead) {
    rt.unread = 0
  }

  return rt
}

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
  isDeleted: boolean
  detail: MessageDetail<any> | null
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

export type ChannelInfoEx = ChannelInfo &
  SyncAt & {
    _kek: Uint8Array | null
    _managers: string[]
  }

export class MyMessageState {
  readonly id: string
  readonly ns: string
  readonly principal: Principal
  readonly api: MessageCanisterAPI
  readonly agent: MessageAgent

  private _mks: MasterKey[] = []
  private _ek: ECDHKey | null = null
  private _channelDEKs = new Map<String, AesGcmKey>()

  private static _instance: MyMessageState | null = null

  static async load(): Promise<MyMessageState> {
    if (
      MyMessageState._instance &&
      MyMessageState._instance.principal.compareTo(
        dynAgent.id.getPrincipal()
      ) === 'eq'
    ) {
      return MyMessageState._instance
    }

    const mAgent = await MessageAgent.create()
    const self = new MyMessageState(mAgent)
    MyMessageState._instance = self
    await self.init()
    return self
  }

  private constructor(agent: MessageAgent) {
    this.principal = agent.principal
    this.id = agent.id
    this.ns = this.id.replaceAll('-', '_')
    this.api = agent.api
    this.agent = agent
  }

  masterKeyKind(): 'Local' | 'ECDH' | 'VetKey' {
    return this.agent.hasCOSE ? 'VetKey' : 'Local'
  }

  isReady(): boolean {
    const mk = this._mks.at(-1)
    return (mk && mk.isUser(this.principal) && mk.isOpened()) || false
  }

  isReady2(): boolean {
    const mk = this._mks.at(-1)
    return (
      (mk &&
        mk.isUser(this.principal) &&
        mk.isOpened() &&
        this.masterKeyKind() === mk.kind) ||
      false
    )
  }

  private async init(): Promise<void> {}

  async myIV(): Promise<Uint8Array> {
    // should not be cached
    const myIV = await this.api.my_iv()
    if (!myIV) {
      throw new Error('Invalid initialization vector')
    }
    return myIV
  }

  async masterKey(myIV: Uint8Array): Promise<MasterKey | null> {
    let mk = this._mks.at(-1)
    if (!mk || !mk.isUser(this.principal)) {
      let keys = await this.agent.getMasterKeys<MasterKeyInfo>()
      this._mks = keys.map((key) => MasterKey.fromInfo(this.principal, key))

      if (this._mks.length == 0 && this.agent.hasCOSE) {
        try {
          mk = await this.fetchOrInitMasterKeyWithVetkey(myIV)
          this._mks.push(mk)
          await this.agent.setMasterKeys(this._mks.map((k) => k.toInfo()))
        } catch (_err) {}
      }
    }

    mk = this._mks.at(-1)
    if (mk) {
      await mk.tryOpen(myIV)
    }

    return mk || null
  }

  async mustMasterKey(): Promise<MasterKey> {
    if (!this.isReady()) {
      throw new Error('Master key not ready')
    }
    return this._mks.at(-1)!
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
    passwordExpire: number, // milliseconds
    myIV: Uint8Array
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
      passwordExpire > 0 ? passwordExpire + Date.now() : 0,
      myIV
    )

    this._mks.push(mk)
    return mk
  }

  async resetMasterKey(
    kind: 'Local' | 'ECDH' | 'VetKey',
    password: string,
    remoteSecret: Uint8Array,
    passwordExpire: number, // milliseconds
    myIV: Uint8Array
  ): Promise<MasterKey> {
    const mk = await MasterKey.from(
      this.principal,
      kind,
      null,
      password,
      this.id,
      remoteSecret,
      passwordExpire > 0 ? passwordExpire + Date.now() : 0,
      myIV
    )

    this._mks.length = 0
    this._mks.push(mk)
    return mk
  }

  async saveMasterKeys(): Promise<void> {
    if (this._mks.length > 0) {
      await this.agent.setMasterKeys(this._mks.map((k) => k.toInfo()))
    }
  }

  async migrateKeys(myIV: Uint8Array): Promise<void> {
    const mk = await this.mustMasterKey()
    if (!mk.isOpened()) {
      throw new Error('Master key not opened')
    }

    if (mk.kind != 'Local' && mk.kind != 'ECDH') {
      return
    }

    const mKey = mk.toA256GCMKey()
    const aad = new Uint8Array()

    // reuse existing ECDH master key
    const newSecret = mk.kind == 'ECDH' ? mKey.getSecret() : randomBytes(32)

    const nKey = AesGcmKey.fromSecret(newSecret, KEY_ID)
    const newMK = await MasterKey.fromVetkey(nKey, myIV, mk.aad)
    await this.initMasterKeyWithVetkey(nKey)

    this._mks.push(newMK)
    await this.saveMasterKeys()

    if (mk.kind == 'Local') {
      // migrate existing channel KEKs
      const channels = await this.agent.loadMyChannels()
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
          data = await coseA256GCMEncrypt0(nKey, data, aad)
          await this.saveChannelKEK(ch.canister, ch.id, data)
        } catch (err) {
          console.error('migrateKeys', err)
        }
      }
    }

    await this.initStaticECDHKey(mk.kind == 'Local' ? mKey : undefined)
  }

  async fetchECDHCoseEncryptedKey(): Promise<AesGcmKey> {
    const coseAPI = this.agent.coseAPI
    const aad = this.principal.toUint8Array()
    const ecdh = generateECDHKey()
    const output = await coseAPI.ecdh_cose_encrypted_key(
      {
        ns: this.ns,
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

  async fetchOrInitMasterKeyWithVetkey(myIV: Uint8Array): Promise<MasterKey> {
    const coseAPI = this.agent.coseAPI
    const path: SettingPath = {
      ns: this.ns,
      key: KEY_ID,
      subject: [this.principal],
      version: 0,
      user_owned: true
    }
    const aad = this.principal.toUint8Array()
    const output = await coseAPI.setting_try_get(path)

    if (!output) {
      const pwdHash = await this.getPasswordHash()
      if (pwdHash) {
        throw new Error('ECDH master key exists')
      }

      const newSecret = randomBytes(32)
      const nKey = AesGcmKey.fromSecret(newSecret, KEY_ID)
      const newMK = await MasterKey.fromVetkey(nKey, myIV, aad)
      await this.initMasterKeyWithVetkey(nKey)
      return newMK
    }

    const encrypted0 = output.dek[0] as Uint8Array
    if (!encrypted0) {
      throw new Error('Master key not found')
    }
    const [vk, _dpk] = await coseAPI.vetkey(path)
    const kekSecret = vk.deriveSymmetricKey('', 32)

    const data = await coseA256GCMDecrypt0(
      AesGcmKey.fromSecret(kekSecret),
      encrypted0,
      aad
    )
    const key = AesGcmKey.fromBytes(data)

    return await MasterKey.fromVetkey(key, myIV, aad)
  }

  async initMasterKeyWithVetkey(mKey: AesGcmKey): Promise<void> {
    const coseAPI = this.agent.coseAPI
    const path: SettingPath = {
      ns: this.ns,
      key: KEY_ID,
      subject: [this.principal],
      version: 0,
      user_owned: true
    }
    const aad = this.principal.toUint8Array()
    const [vk, _dpk] = await coseAPI.vetkey(path)
    const kekSecret = vk.deriveSymmetricKey('', 32)

    const encrypted0 = await coseA256GCMEncrypt0(
      AesGcmKey.fromSecret(kekSecret),
      mKey.toBytes(),
      aad,
      KEY_ID
    )

    await coseAPI.setting_create(path, {
      dek: [encrypted0],
      status: [],
      payload: [],
      desc: [],
      tags: []
    })
  }

  async initStaticECDHKey(prevMK?: AesGcmKey): Promise<void> {
    const mk = (await this.mustMasterKey()).toA256GCMKey()
    const aad = this.principal.toUint8Array()
    try {
      let encrypted0 = await this.loadStaticECDHKey()

      try {
        const data = await coseA256GCMDecrypt0(prevMK || mk, encrypted0, aad)
        this._ek = ECDHKey.fromBytes(data)
      } catch (err) {
        if (prevMK) {
          const data = await coseA256GCMDecrypt0(mk, encrypted0, aad)
          this._ek = ECDHKey.fromBytes(data)
        } else {
          throw err
        }
      }

      if (prevMK && this._ek) {
        encrypted0 = await coseA256GCMEncrypt0(
          mk,
          this._ek.toBytes(),
          aad,
          mk.kid
        )
        await this.saveStaticECDHKey(this._ek.getPublicKey(), encrypted0)
      }
    } catch (err) {
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
    let ek = await this.agent.getECDHKey()
    if (this.agent.hasCOSE) {
      // always fetch the latest ECDH key from COSE
      const coseAPI = this.agent.coseAPI
      const output = await coseAPI.setting_get_or_migrate({
        ns: this.ns,
        key: utf8ToBytes('StaticECDH'),
        subject: [this.principal],
        version: 0,
        user_owned: true
      })

      ek = output.dek[0] as Uint8Array
      await this.agent.setECDHKey(ek)
    }

    if (!ek) {
      throw new Error('Static ECDH not found')
    }

    return ek
  }

  async saveStaticECDHKey(
    ecdh_pub: Uint8Array,
    encrypted0: Uint8Array
  ): Promise<void> {
    await this.agent.setECDHKey(encrypted0)

    if (this.agent.hasCOSE) {
      await this.agent.coseAPI.setting_upsert(
        {
          ns: this.ns,
          key: utf8ToBytes('StaticECDH'),
          subject: [this.principal],
          version: 0,
          user_owned: true
        },
        {
          dek: [encrypted0],
          status: [],
          desc: [],
          tags: [],
          payload: []
        }
      )
    }

    const profile = await this.agent.fetchProfile()
    if (
      !profile.ecdh_pub[0] ||
      compareBytes(profile.ecdh_pub[0] as Uint8Array, ecdh_pub) !== 0
    ) {
      await this.agent.profileAPI.update_profile_ecdh_pub(ecdh_pub)
    }
  }

  // DEPRECATED
  async getPasswordHash(): Promise<Uint8Array | null> {
    const kv = await this.agent.getKV()
    return kv.get(PWD_HASH_KEY) || null
  }

  // DEPRECATED
  async savePasswordHash(hash: Uint8Array): Promise<void> {
    if (this.agent.hasCOSE) {
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
        throw new Error('Channel encryption key not ready')
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
    let kek = await this.agent.getKEK(canister, id)

    if (kek) {
      return kek
    }
    const output = await this.agent.coseAPI.setting_get_or_migrate({
      ns: this.ns,
      key: encodeCBOR([canister.toUint8Array(), id]),
      subject: [this.principal],
      version: 0,
      user_owned: true
    })

    kek = output.dek[0] as Uint8Array
    if (!kek) {
      throw new Error('Channel encryption key not found')
    }

    await this.agent.setKEK(canister, id, kek)
    return kek
  }

  async saveChannelKEK(
    canister: Principal,
    id: number,
    kek: Uint8Array
  ): Promise<void> {
    await this.agent.setKEK(canister, id, kek)

    if (this.agent.hasCOSE) {
      await this.agent.coseAPI.setting_upsert(
        {
          ns: this.ns,
          key: encodeCBOR([canister.toUint8Array(), id]),
          subject: [this.principal],
          version: 0,
          user_owned: true
        },
        {
          dek: [kek],
          status: [],
          desc: [],
          tags: [],
          payload: []
        }
      )
    }
  }

  async requestKEK(info: ChannelInfo): Promise<void> {
    const ek = await this.mustStaticECDHKey()
    const api = this.api.channelAPI(info.canister)
    const setting = await api.update_my_setting({
      id: info.id,
      ecdh: [{ ecdh_pub: [ek.getPublicKey()], ecdh_remote: [] }],
      mute: [],
      last_read: []
    })
    this.updateMyChannelSetting(info.canister, info.id, setting)
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

    const api = this.api.channelAPI(info.canister)
    const setting = await api.update_my_setting({
      id: info.id,
      ecdh: [{ ecdh_pub: [], ecdh_remote: [] }],
      mute: [],
      last_read: []
    })

    this.updateMyChannelSetting(info.canister, info.id, setting)
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
    const api = this.api.channelAPI(info.canister)
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
    const api = this.api.channelAPI(info.canister)
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

  async adminAddManager(info: ChannelInfoEx, member: Principal): Promise<void> {
    const api = this.api.channelAPI(info.canister)
    const input = {
      id: info.id,
      member,
      ecdh: {
        ecdh_remote: [],
        ecdh_pub: []
      } as ChannelECDHInput
    }

    await api.update_manager(input)
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

  async updateMyChannelSetting(
    canister: Principal,
    id: number,
    setting?: Partial<ChannelSetting>,
    latestMessageId?: number
  ): Promise<void> {
    const channel = await this.agent.getChannel(canister, id)

    if (setting) {
      channel.my_setting = mergeMySetting(
        channel.my_setting,
        setting,
        latestMessageId || channel.latest_message_id
      )
      await this.agent.setChannel(channel)
    }
  }

  async removeChannel(canister: Principal, id: number): Promise<void> {
    await this.agent.removeChannel(canister, id)
  }

  async refreshChannel(info: ChannelInfoEx): Promise<ChannelInfoEx> {
    const ninfo = await this.agent.fetchChannel(info.canister, info.id, 0n)
    if (!ninfo) {
      throw new Error('Channel not found')
    }

    let kek = info._kek
    if (!kek) {
      kek = await this.loadChannelKEK(info.canister, info.id).catch((e) => null)
    }
    return {
      ...ninfo,
      _kek: kek,
      _managers: ninfo.managers.map((m) => m.toText())
    }
  }

  async refreshMyChannels(signal: AbortSignal): Promise<void> {
    if (dynAgent.isAnonymous()) return
    this.agent.fetchMyChannels(signal)
  }

  async loadMyChannelsStream(): Promise<Readable<ChannelBasicInfoEx[]>> {
    const stream = await this.agent.subscribeMyChannels()
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

    return derived(stream, (channels, set) => {
      this.batchLoadUsersInfo(channels.map((c) => c.latest_message_by)).then(
        async (users) => {
          for (const info of users) {
            usersMap.set(info.id.toText(), info)
          }
          set(channels.map(channelMapfFn))
        }
      )
    })
  }

  async loadChannelInfo(
    canister: Principal,
    id: number
  ): Promise<ChannelInfoEx> {
    const channel = await this.agent.getChannel(canister, id)
    const kek = await this.loadChannelKEK(canister, id).catch((e) => null)
    return {
      ...channel,
      _kek: kek,
      _managers: channel.managers.map((m) => m.toText())
    }
  }

  async loadProfile(
    user: Principal | string
  ): Promise<Readable<UserInfo & ProfileInfo>> {
    if (!user) {
      throw new Error('Invalid username')
    }

    const now = Date.now()
    let info = await getUser(now, user)
    if (!info) {
      info =
        typeof user == 'string'
          ? await this.api.get_by_username(user)
          : await this.api.get_user(user)
      await setUser(now, info)
    }

    const profile = await getProfile(info.id)

    const api = await this.api.profileAPI(info.profile_canister)
    return readable(
      { ...info, ...profile } as UserInfo & ProfileInfo,
      (set) => {
        api.get_profile(info.id).then(async (profile) => {
          await setProfile(profile)
          set({ ...info, ...profile })
        })
      }
    )
  }

  async tryLoadUser(user: Principal | string): Promise<UserInfo | null> {
    if (user) {
      try {
        const now = Date.now()
        let info = await getUser(now, user)
        if (!info) {
          info =
            typeof user == 'string'
              ? await this.api.get_by_username(user)
              : await this.api.get_user(user)
          await setUser(now, info)
        }

        return info
      } catch (err) {}
    }

    return null
  }

  async tryLoadProfile(
    user: Principal | string
  ): Promise<(UserInfo & ProfileInfo) | null> {
    if (user) {
      try {
        const userInfo = await this.tryLoadUser(user)
        if (!userInfo) {
          return null
        }

        let profile = await getProfile(userInfo.id)
        if (!profile) {
          const api = await this.api.profileAPI(userInfo.profile_canister)
          profile = await api.get_profile(userInfo.id)
          await setProfile(profile)
        }

        return { ...userInfo, ...profile }
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
      await setUser(now, user)
      const api = await this.api.profileAPI(user.profile_canister)
      const profile = await api.get_profile(user.id)
      await setProfile(profile)

      return { ...user, ...profile }
    } catch (err) {}
    return null
  }

  async loadLatestMessageStream(
    canister: Principal,
    channelId: number,
    messageId: number,
    dek: AesGcmKey,
    signal?: AbortSignal
  ): Promise<Readable<MessageInfo | null>> {
    const stream = this.agent.subscribeLatestMessage(
      canister,
      channelId,
      messageId,
      signal
    )

    return derived(stream, ($msg, set) => {
      if ($msg) {
        this.messagesToInfo(canister, channelId, dek, $msg).then((msgs) => {
          msgs[0] && set(msgs[0])
        })
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
    const messages = await this.agent.loadMessages(
      canister,
      channelId,
      start,
      end
    )

    return await this.messagesToInfo(canister, channelId, dek, messages)
  }

  async deleteMessage(
    canister: Principal,
    channelId: number,
    messageId: number
  ): Promise<void> {
    const api = this.api.channelAPI(canister)
    await api.delete_message(channelId, messageId)
    await this.agent.updateDeletedMessage(
      canister.toUint8Array(),
      channelId,
      messageId
    )
  }

  async clearCachedMessages(): Promise<void> {
    await this.agent.clearCachedMessages()
  }

  async updateMyLastRead(
    canister: Principal,
    channel: number,
    lastRead: number
  ): Promise<void> {
    const api = this.api.channelAPI(canister)
    const setting = await api.update_my_setting({
      id: channel,
      ecdh: [],
      mute: [],
      last_read: [lastRead]
    })

    await this.updateMyChannelSetting(canister, channel, setting)
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
      const m: MessageInfo = {
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
        isDeleted: false,
        detail: null,
        src: msg
      }

      if (msg.payload.length === 0) {
        m.error = `Message is deleted by ${m.created_user.name}`
        m.isDeleted = true
      } else {
        try {
          const payload =
            msg.kind == 1
              ? (msg.payload as Uint8Array)
              : await coseA256GCMDecrypt0(dek, msg.payload as Uint8Array, aad)
          m.detail = MessageDetail.from(payload)
          m.message = m.detail.message
          m.error = m.detail.error
        } catch (err) {
          m.error = `Failed to decrypt message: ${err}`
        }
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
      const info = await getUser(now, id)
      if (info) {
        rt.push(info)
      } else {
        todo.push(id)
      }
    }
    if (todo.length > 0) {
      const users = await this.api.batch_get_users(todo)
      for (const info of users) {
        rt.push(info)
        await setUser(now, info)
      }
    }
    return rt
  }
}

interface MasterKeyInfo {
  kind: 'Local' | 'ECDH' | 'VetKey'
  keyId: Uint8Array
  passwordSecrect: Uint8Array
  encryptedSecret: Uint8Array
  passwordExpireAt: number
  version: number
}

export class MasterKey {
  readonly kind: 'Local' | 'ECDH' | 'VetKey'
  readonly keyId: Uint8Array
  readonly version: number
  readonly aad: Uint8Array
  private passwordExpireAt: number
  private passwordSecrect: Uint8Array
  private readonly encryptedSecret: Uint8Array
  private aesGcmKey: AesGcmKey | null = null

  static async from(
    user: Principal,
    kind: 'Local' | 'ECDH' | 'VetKey',
    pwdHash: Uint8Array | null,
    password: string,
    salt: string,
    remoteSecret: Uint8Array,
    passwordExpireAt: number,
    myIV: Uint8Array
  ): Promise<MasterKey> {
    const aad = user.toUint8Array()
    const passwordSecrect = hashPassword(password, salt)
    if (pwdHash) {
      const hash = hmac3_256(KEY_ID, passwordSecrect)
      if (compareBytes(pwdHash, hash) !== 0) {
        throw new Error('Invalid password')
      }
    }

    const kek = deriveA256GCMSecret(passwordSecrect, myIV)
    const secret = deriveA256GCMSecret(passwordSecrect, remoteSecret)
    const key = AesGcmKey.fromSecret(secret, KEY_ID)
    const encryptedSecret = await coseA256GCMEncrypt0(
      AesGcmKey.fromSecret(kek),
      key.toBytes(),
      aad
    )

    const mk = new MasterKey(
      kind,
      KEY_ID,
      2,
      encryptedSecret,
      aad,
      passwordExpireAt,
      passwordSecrect
    )

    mk.aesGcmKey = key
    return mk
  }

  static async fromVetkey(
    mKey: AesGcmKey,
    myIV: Uint8Array,
    aad: Uint8Array
  ): Promise<MasterKey> {
    const passwordSecrect = new Uint8Array()
    const kek = deriveA256GCMSecret(passwordSecrect, myIV)
    const encryptedSecret = await coseA256GCMEncrypt0(
      AesGcmKey.fromSecret(kek),
      mKey.toBytes(),
      aad
    )

    const mk = new MasterKey(
      'VetKey',
      KEY_ID,
      2,
      encryptedSecret,
      aad,
      0,
      passwordSecrect
    )
    mk.aesGcmKey = mKey
    return mk
  }

  static fromInfo(user: Principal, info: MasterKeyInfo): MasterKey {
    return new MasterKey(
      info.kind,
      info.keyId,
      info.version || 1,
      info.encryptedSecret,
      user.toUint8Array(),
      info.passwordExpireAt,
      info.passwordSecrect
    )
  }

  constructor(
    kind: 'Local' | 'ECDH' | 'VetKey',
    keyId: Uint8Array,
    version: number,
    encryptedSecret: Uint8Array,
    aad: Uint8Array,
    passwordExpireAt: number,
    passwordSecrect: Uint8Array
  ) {
    this.kind = kind
    this.keyId = keyId
    this.version = version
    this.aad = aad
    this.encryptedSecret = encryptedSecret
    this.passwordExpireAt = passwordExpireAt
    this.passwordSecrect = passwordSecrect
  }

  async tryOpen(myIV: Uint8Array): Promise<void> {
    if (this.kind == 'VetKey' || this.passwordExpireAt >= Date.now()) {
      try {
        await this._openV2(this.passwordSecrect, myIV)
      } catch (_err) {}
    }
  }

  async open(
    password: string,
    salt: string,
    passwordExpire: number,
    myIV: Uint8Array
  ): Promise<void> {
    this.passwordExpireAt = passwordExpire > 0 ? passwordExpire + Date.now() : 0
    const passwordSecrect =
      this.kind == 'VetKey' ? new Uint8Array() : hashPassword(password, salt)

    await this._openV2(passwordSecrect, myIV)
  }

  private async _openV2(
    passwordSecrect: Uint8Array,
    myIV: Uint8Array
  ): Promise<void> {
    const kek = deriveA256GCMSecret(passwordSecrect, myIV)
    const data = await coseA256GCMDecrypt0(
      AesGcmKey.fromSecret(kek),
      this.encryptedSecret,
      this.aad
    )
    const key = AesGcmKey.fromBytes(data)
    this.passwordSecrect = passwordSecrect
    this.aesGcmKey = key
  }

  isOpened(): boolean {
    return this.aesGcmKey != null
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
      version: this.version,
      passwordSecrect: this.passwordSecrect,
      encryptedSecret: this.encryptedSecret,
      passwordExpireAt: this.passwordExpireAt
    }
  }

  toA256GCMKey(): AesGcmKey {
    if (!this.aesGcmKey) {
      throw new Error('master key is not opened')
    }

    return this.aesGcmKey
  }

  passwordHash(): Uint8Array {
    return hmac3_256(KEY_ID, this.passwordSecrect)
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
  const rt = {
    _id,
    username: unwrapOption(info.username) || '',
    name: info.name || 'Unknown',
    image: info.image,
    src: info
  }
  return rt
}
