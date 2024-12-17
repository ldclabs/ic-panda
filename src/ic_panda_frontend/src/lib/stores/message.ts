import { type MessageCanisterAPI, type UserInfo } from '$lib/canisters/message'
import { type ProfileInfo } from '$lib/canisters/messageprofile'
import { unwrapOption } from '$lib/types/result'
import { dynAgent } from '$lib/utils/auth'
import { Principal } from '@dfinity/principal'
import { readable, type Readable } from 'svelte/store'
import { getProfile, getUser, setProfile, setUser } from './kvstore'
import { MessageAgent } from './message_agent'

export interface DisplayUserInfo {
  _id: string
  username: string
  name: string
  image: string
  src?: UserInfo
}

export class MyMessageState {
  readonly id: string
  readonly ns: string
  readonly principal: Principal
  readonly api: MessageCanisterAPI
  readonly agent: MessageAgent

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
    return self
  }

  private constructor(agent: MessageAgent) {
    this.principal = agent.principal
    this.id = agent.id
    this.ns = this.id.replaceAll('-', '_')
    this.api = agent.api
    this.agent = agent
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
  if (rt.username.toLocaleUpperCase() === 'PANDA') {
    rt.image = '/_assets/logo.svg'
  }
  return rt
}
