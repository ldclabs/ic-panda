import {
  idlFactory,
  type ChannelInfo,
  type ChannelKEKInput,
  type CreateChannelInput,
  type StateInfo,
  type UserInfo,
  type _SERVICE
} from '$declarations/ic_message/ic_message.did.js'
import { MESSAGE_CANISTER_ID } from '$lib/constants'
import { asyncFactory } from '$lib/stores/auth'
import { unwrapResult } from '$lib/types/result'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { readonly, writable, type Readable } from 'svelte/store'
import { createActor } from './actors'
import { CoseAPI } from './cose'
import { ChannelAPI } from './messagechannel'
import { ProfileAPI } from './messageprofile'

export {
  type ChannelECDHInput,
  type StateInfo,
  type UserInfo
} from '$declarations/ic_message/ic_message.did.js'

export class MessageCanisterAPI {
  readonly identity: Identity
  readonly principal: Principal
  readonly canisterId: Principal
  private actor: _SERVICE
  private $state: StateInfo | null = null
  private $myInfo: UserInfo | null = null
  private _state = writable<StateInfo | null>(null)
  private _myInfo = writable<UserInfo | null>(null)

  static async with(identity: Identity): Promise<MessageCanisterAPI> {
    const actor = await createActor<_SERVICE>({
      canisterId: MESSAGE_CANISTER_ID,
      idlFactory: idlFactory,
      identity
    })

    const api = new MessageCanisterAPI(identity, actor)
    await Promise.all([api.refreshState(), api.refreshMyInfo()])
    return api
  }

  constructor(identity: Identity, actor: _SERVICE) {
    this.identity = identity
    this.principal = identity.getPrincipal()
    this.canisterId = Principal.fromText(MESSAGE_CANISTER_ID)
    this.actor = actor
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

  async profileAPI(canister: Principal): Promise<ProfileAPI> {
    return ProfileAPI.with(this.identity, canister)
  }

  async channelAPI(canister: Principal): Promise<ChannelAPI> {
    return ChannelAPI.with(this.identity, canister)
  }

  async coseAPI(canister: Principal): Promise<CoseAPI> {
    return CoseAPI.with(this.identity, canister)
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

  async batch_get_users(ids: Principal[]): Promise<UserInfo[]> {
    const res = await this.actor.batch_get_users(ids)
    return unwrapResult(res, 'call batch_get_users failed')
  }

  async create_channel(input: CreateChannelInput): Promise<ChannelInfo> {
    const res = await this.actor.create_channel(input)
    return unwrapResult(res, 'call create_channel failed')
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
}

const messageCanisterAPIStore = asyncFactory((identity) =>
  MessageCanisterAPI.with(identity)
)

export const messageCanisterAPIAsync = async () =>
  (await messageCanisterAPIStore).async()
