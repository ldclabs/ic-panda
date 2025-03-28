import {
  idlFactory,
  type AddMessageInput,
  type AddMessageOutput,
  type ChannelBasicInfo,
  type ChannelInfo,
  type ChannelSetting,
  type DownloadFilesToken,
  type Message,
  type StateInfo,
  type UpdateChannelInput,
  type UpdateChannelMemberInput,
  type UpdateChannelStorageInput,
  type UpdateMySettingInput,
  type UploadFileInput,
  type UploadFileOutput,
  type _SERVICE
} from '$declarations/ic_message_channel/ic_message_channel.did.js'
import { unwrapResult } from '$lib/types/result'
import { Principal } from '@dfinity/principal'
import { createActor } from './actors'

export {
  type ChannelBasicInfo,
  type ChannelECDHInput,
  type ChannelInfo,
  type ChannelSetting,
  type Message,
  type StateInfo,
  type UpdateChannelInput,
  type UpdateChannelStorageInput
} from '$declarations/ic_message_channel/ic_message_channel.did.js'

interface CompareChannel {
  latest_message_at: bigint
}

export class ChannelAPI {
  readonly canisterId: Principal
  private actor: _SERVICE

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

  constructor(canister: Principal) {
    this.canisterId = canister
    this.actor = createActor<_SERVICE>({
      canisterId: canister,
      idlFactory: idlFactory
    })
  }

  async get_state(): Promise<StateInfo> {
    const res = await this.actor.get_state()
    return unwrapResult(res, 'call get_state failed')
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

  async delete_message(channel: number, id: number): Promise<null> {
    const res = await this.actor.delete_message({ id, channel })
    return unwrapResult(res, 'call delete_message failed')
  }

  async my_channel_ids(): Promise<number[]> {
    const res = await this.actor.my_channel_ids()
    const rt = unwrapResult(res, 'call my_channel_ids failed')
    return rt instanceof Uint32Array ? Array.from(rt) : rt
  }

  async my_channels(updated_at: bigint = 0n): Promise<ChannelBasicInfo[]> {
    const res = await this.actor.my_channels_if_update([updated_at])
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

  async update_storage(input: UpdateChannelStorageInput): Promise<Message> {
    const res = await this.actor.update_storage(input)
    return unwrapResult(res, 'call update_storage failed')
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

  async update_my_setting(
    input: UpdateMySettingInput
  ): Promise<ChannelSetting> {
    const res = await this.actor.update_my_setting(input)
    return unwrapResult(res, 'call update_my_setting failed')
  }

  async upload_file_token(input: UploadFileInput): Promise<UploadFileOutput> {
    const res = await this.actor.upload_file_token(input)
    return unwrapResult(res, 'call upload_file_token failed')
  }

  async upload_image_token(input: UploadFileInput): Promise<UploadFileOutput> {
    const res = await this.actor.upload_image_token(input)
    return unwrapResult(res, 'call upload_image_token failed')
  }

  async download_files_token(channel: number): Promise<DownloadFilesToken> {
    const res = await this.actor.download_files_token(channel)
    return unwrapResult(res, 'call download_files_token failed')
  }
}
