import {
  messageCanisterAPIAsync,
  type MessageCanisterAPI,
  type UserInfo
} from '$lib/canisters/message'
import { KVStore } from '$lib/utils/store'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { derived, type Readable } from 'svelte/store'
import { asyncFactory } from './auth'

const usersKV = new KVStore('ICPanda_Users')

export class MyMessageState {
  readonly principal: Principal
  readonly id: string
  readonly api: MessageCanisterAPI
  readonly info: Readable<UserInfo>

  static async with(identity: Identity): Promise<MyMessageState> {
    const api = await messageCanisterAPIAsync()
    return new MyMessageState(identity.getPrincipal(), api)
  }

  constructor(principal: Principal, api: MessageCanisterAPI) {
    this.principal = principal
    this.id = principal.toText()
    this.api = api
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
}

const myMessageStateStore = asyncFactory((identity) =>
  MyMessageState.with(identity)
)

export const myMessageStateAsync = async () =>
  (await myMessageStateStore).async()
