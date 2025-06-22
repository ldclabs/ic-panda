import { type UserInfo } from '$lib/canisters/message'
import { type ProfileInfo } from '$lib/canisters/messageprofile'
import { MESSAGE_CANISTER_ID } from '$lib/constants'
import { KVStore } from '$lib/utils/store'
import type { Principal } from '@dfinity/principal'

const USERS_CACHE_EXP = 2 * 3600 * 1000

export const KVS = new KVStore('ICPanda', 1, [
  ['My'], // deprecated
  ['Keys'], // deprecated
  ['Users'],
  [
    'Profiles',
    {
      keyPath: '_id'
    }
  ],
  [
    'Channels', // deprecated
    {
      keyPath: ['_canister', 'id']
    }
  ],
  [
    'Messages', // deprecated
    {
      keyPath: ['_canister', 'channel', 'id']
    }
  ]
])

export async function getUser(
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

  let [ts, info] = (await KVS.get<[number, UserInfo]>('Users', k)) || [0, null]
  info = info && now - ts < USERS_CACHE_EXP ? info : null
  if (typeof user == 'string' && info && info.username[0] !== user) {
    // username has been changed
    info = null
  }
  return info
}

export async function getUser2(
  user: Principal | string
): Promise<[number, UserInfo | null]> {
  const k = typeof user == 'string' ? user : user.toText()
  if (k === MESSAGE_CANISTER_ID) {
    return [
      0,
      {
        id: user as Principal,
        username: ['_'],
        cose_canister: [],
        name: 'System',
        image: '',
        profile_canister: user as Principal
      }
    ]
  }

  return (await KVS.get<[number, UserInfo]>('Users', k)) || [0, null]
}

export async function setUser(now: number, info: UserInfo): Promise<void> {
  await KVS.set<[number, UserInfo]>('Users', [now, info], info.id.toText())

  if (info.username.length == 1) {
    await KVS.set<[number, UserInfo]>('Users', [now, info], info.username[0])
  }
}

export async function getProfile(user: Principal): Promise<ProfileInfo | null> {
  return await KVS.get<ProfileInfo>(
    'Profiles',
    user.toUint8Array() as BufferSource
  )
}

export async function setProfile(info: ProfileInfo): Promise<void> {
  await KVS.set<ProfileInfo>('Profiles', info)
}
