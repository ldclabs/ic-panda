import {
  idlFactory,
  type ChannelInfo,
  type ChannelKEKInput,
  type ChannelTopupInput,
  type CreateChannelInput,
  type StateInfo,
  type UpdateKVInput,
  type UserInfo,
  type _SERVICE
} from '$declarations/ic_message/ic_message.did.js'
import { MESSAGE_CANISTER_ID } from '$lib/constants'
import { unwrapResult } from '$lib/types/result'
import { agent } from '$lib/utils/auth'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { readonly, writable, type Readable } from 'svelte/store'
import { createActor } from './actors'
import { CoseAPI } from './cose'
import { ChannelAPI } from './messagechannel'
import { ProfileAPI } from './messageprofile'

export {
  type ChannelECDHInput,
  type ChannelInfo,
  type StateInfo,
  type UpdateKVInput,
  type UserInfo
} from '$declarations/ic_message/ic_message.did.js'

export class MessageCanisterAPI {
  readonly canisterId: Principal
  private actor: _SERVICE
  private $state: StateInfo | null = null
  private $myInfo: UserInfo | null = null
  private _state = writable<StateInfo | null>(null)
  private _myInfo = writable<UserInfo | null>(null)
  private _profileAPIs = new Map<string, ProfileAPI>()
  private _channelAPIs = new Map<string, ChannelAPI>()
  private _coseAPIs = new Map<string, CoseAPI>()

  constructor() {
    this.canisterId = Principal.fromText(MESSAGE_CANISTER_ID)
    this.actor = createActor<_SERVICE>({
      canisterId: MESSAGE_CANISTER_ID,
      idlFactory: idlFactory
    })
  }

  get identity(): Identity {
    return agent.id
  }

  get principal(): Principal {
    return agent.id.getPrincipal()
  }

  get stateStore(): Readable<StateInfo | null> {
    return readonly(this._state)
  }

  get myInfoStore(): Readable<UserInfo | null> {
    return readonly(this._myInfo)
  }

  get state(): StateInfo | null {
    return this.$state
  }

  get myInfo(): UserInfo | null {
    return this.$myInfo
  }

  profileAPI(canister: Principal): ProfileAPI {
    const key = canister.toText()
    if (!this._profileAPIs.has(key)) {
      this._profileAPIs.set(key, new ProfileAPI(canister))
    }

    return this._profileAPIs.get(key)!
  }

  channelAPI(canister: Principal): ChannelAPI {
    const key = canister.toText()
    if (!this._channelAPIs.has(key)) {
      this._channelAPIs.set(key, new ChannelAPI(canister))
    }

    return this._channelAPIs.get(key)!
  }

  coseAPI(canister: Principal): CoseAPI {
    const key = canister.toText()
    if (!this._coseAPIs.has(key)) {
      this._coseAPIs.set(key, new CoseAPI(canister))
    }

    return this._coseAPIs.get(key)!
  }

  async refreshAllState(): Promise<void> {
    await Promise.all([this.refreshState(), this.refreshMyInfo()])
  }

  async refreshState(): Promise<void> {
    const state = await this.actor.get_state()
    this.$state = unwrapResult(state, 'call get_state failed')
    this._state.set(this.$state)
  }

  async refreshMyInfo(): Promise<void> {
    if (this.principal.isAnonymous()) {
      return
    }

    try {
      const info = await this.actor.get_user([])
      this.$myInfo = unwrapResult(info, 'call get_user failed')
      this._myInfo.set(this.$myInfo)
    } catch (e) {}
  }

  async my_iv(): Promise<Uint8Array | null> {
    if (!this.principal.isAnonymous()) {
      try {
        const rt = await this.actor.my_iv()
        return unwrapResult<Uint8Array | number[], string>(
          rt,
          'call my_iv failed'
        ) as Uint8Array
      } catch (e) {}
    }
    return null
  }

  async batch_get_users(ids: Principal[]): Promise<UserInfo[]> {
    const res = await this.actor.batch_get_users(ids)
    return unwrapResult(res, 'call batch_get_users failed')
  }

  async create_channel(input: CreateChannelInput): Promise<ChannelInfo> {
    const res = await this.actor.create_channel(input)
    return unwrapResult(res, 'call create_channel failed')
  }

  async topup_channel(input: ChannelTopupInput): Promise<ChannelInfo> {
    const res = await this.actor.topup_channel(input)
    return unwrapResult(res, 'call topup_channel failed')
  }

  async get_by_username(username: string): Promise<UserInfo> {
    const res = await this.actor.get_by_username(username)
    return unwrapResult(res, 'call get_by_username failed')
  }

  async get_user(id: Principal): Promise<UserInfo> {
    const res = await this.actor.get_user([id])
    return unwrapResult(res, 'call get_user failed')
  }

  async register_username(
    username: string,
    name: string = ''
  ): Promise<UserInfo> {
    const res = await this.actor.register_username(
      username,
      name == '' ? [] : [name]
    )
    return unwrapResult(res, 'call register_username failed')
  }

  async transfer_username(to: Principal): Promise<null> {
    const res = await this.actor.transfer_username(to)
    return unwrapResult(res, 'call transfer_username failed')
  }

  async save_channel_kek(input: ChannelKEKInput): Promise<null> {
    const res = await this.actor.save_channel_kek(input)
    return unwrapResult(res, 'call save_channel_kek failed')
  }

  async search_username(username: string): Promise<string[]> {
    const res = await this.actor.search_username(username)
    return unwrapResult(res, 'call search_username failed')
  }

  async update_my_image(url: string): Promise<null> {
    const res = await this.actor.update_my_image(url)
    return unwrapResult(res, 'call update_my_image failed')
  }

  async update_my_name(name: string): Promise<UserInfo> {
    const res = await this.actor.update_my_name(name)
    return unwrapResult(res, 'call update_my_name failed')
  }

  async update_my_ecdh(
    ecdhPub: Uint8Array,
    encryptedECDH: Uint8Array
  ): Promise<null> {
    const res = await this.actor.update_my_ecdh(ecdhPub, encryptedECDH)
    return unwrapResult(res, 'call update_my_ecdh failed')
  }

  async update_my_kv(input: UpdateKVInput): Promise<null> {
    const res = await this.actor.update_my_kv(input)
    return unwrapResult(res, 'call update_my_kv failed')
  }
}

export const messageCanisterAPI = new MessageCanisterAPI()
