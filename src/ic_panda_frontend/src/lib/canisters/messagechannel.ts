import {
  idlFactory,
  type StateInfo,
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
}
