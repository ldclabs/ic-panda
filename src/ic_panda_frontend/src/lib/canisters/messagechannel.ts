import {
  idlFactory,
  type AddMessageInput,
  type AddMessageOutput,
  type ChannelBasicInfo,
  type ChannelInfo,
  type Message,
  type UpdateChannelInput,
  type UpdateChannelMemberInput,
  type UpdateMySettingInput,
  type _SERVICE
} from '$declarations/ic_message_channel/ic_message_channel.did.js'
import { unwrapResult } from '$lib/types/result'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { createActor } from './actors'

export {
  type ChannelBasicInfo,
  type ChannelECDHInput,
  type ChannelInfo,
  type ChannelSetting,
  type Message,
  type UpdateChannelInput
} from '$declarations/ic_message_channel/ic_message_channel.did.js'

interface CompareChannel {
  latest_message_at: bigint
}

export class ChannelAPI {
  readonly principal: Principal
  readonly canisterId: Principal
  private actor: _SERVICE

  static async with(
    identity: Identity,
    canister: Principal
  ): Promise<ChannelAPI> {
    const actor = await createActor<_SERVICE>({
      canisterId: canister,
      idlFactory: idlFactory,
      identity
    })

    const api = new ChannelAPI(identity.getPrincipal(), canister, actor)
    return api
  }

  static compareChannels(a: CompareChannel, b: CompareChannel): number {
    return Number(b.latest_message_at - a.latest_message_at)
  }

  static channelParam(channel: { id: number; canister: Principal }): string {
    return `${channel.canister.toText()}/${channel.id}`
  }

  static parseChannelParam(param: string): {
    id: number
    canister: Principal | null
  } {
    const ss = param.split('/')
    if (ss.length == 2) {
      const rt = {
        id: parseInt(ss[1] || ''),
        canister: Principal.fromText(ss[0] || '')
      }
      if (rt.id > 0) {
        return rt
      }
    }
    return { id: 0, canister: null }
  }

  constructor(principal: Principal, canister: Principal, actor: _SERVICE) {
    this.principal = principal
    this.canisterId = canister
    this.actor = actor
  }

  async add_message(input: AddMessageInput): Promise<AddMessageOutput> {
    const res = await this.actor.add_message(input)
    return unwrapResult(res, 'call add_message failed')
  }

  async batch_get_channels(ids: number[]): Promise<ChannelBasicInfo[]> {
    const res = await this.actor.batch_get_channels(ids)
    return unwrapResult(res, 'call batch_get_channels failed')
  }

  async get_channel_if_update(
    id: number,
    updated_at: bigint
  ): Promise<ChannelInfo | null> {
    const res = await this.actor.get_channel_if_update(id, updated_at)
    const info = unwrapResult(res, 'call get_channel_if_update failed')
    return info.length == 1 ? info[0] : null
  }

  async get_message(channel: number, id: number): Promise<Message> {
    const res = await this.actor.get_message(channel, id)
    return unwrapResult(res, 'call get_message failed')
  }

  async list_messages(
    channel: number,
    start: number = 0,
    end: number = 0
  ): Promise<Message[]> {
    const res = await this.actor.list_messages(channel, [start], [end])
    return unwrapResult(res, 'call list_messages failed')
  }

  async my_channels(
    latest_message_at: bigint = 0n
  ): Promise<ChannelBasicInfo[]> {
    const res = await this.actor.my_channels_if_update([latest_message_at])
    return unwrapResult(res, 'call my_channels_if_update failed')
  }

  async remove_member(input: UpdateChannelMemberInput): Promise<null> {
    const res = await this.actor.remove_member(input)
    return unwrapResult(res, 'call remove_member failed')
  }

  async leave_channel(id: number, delete_channel: boolean): Promise<null> {
    const res = await this.actor.leave_channel(
      {
        id,
        ecdh: [],
        mute: [],
        last_read: []
      },
      delete_channel
    )
    return unwrapResult(res, 'call leave_channel failed')
  }

  async update_channel(input: UpdateChannelInput): Promise<Message> {
    const res = await this.actor.update_channel(input)
    return unwrapResult(res, 'call update_channel failed')
  }

  async update_manager(
    input: UpdateChannelMemberInput
  ): Promise<[bigint, Message | null]> {
    const res = await this.actor.update_manager(input)
    const [updated_at, message] = unwrapResult(
      res,
      'call update_manager failed'
    )
    return [updated_at, message.length == 1 ? message[0] : null]
  }

  async update_member(
    input: UpdateChannelMemberInput
  ): Promise<[bigint, Message | null]> {
    const res = await this.actor.update_member(input)
    const [updated_at, message] = unwrapResult(res, 'call update_member failed')
    return [updated_at, message.length == 1 ? message[0] : null]
  }

  async update_my_setting(input: UpdateMySettingInput): Promise<null> {
    const res = await this.actor.update_my_setting(input)
    return unwrapResult(res, 'call update_my_setting failed')
  }
}
