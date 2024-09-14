import { KVStore } from '$lib/utils/store'

// keys in 'My' store
export const KEY_NOTIFY_PERM = ':NotifyPerm' // denied | granted | default
export const KEY_REFRESH_MY_CHANNELS_AT = ':RefreshMyChannelsAt' // ms
export const KEY_MY_CHANNELS = ':Channels'

export const KVS = new KVStore('ICPanda', 1, [
  ['My'],
  ['Keys'],
  ['Users'],
  [
    'Profiles',
    {
      keyPath: '_id'
    }
  ],
  [
    'Channels',
    {
      keyPath: ['_canister', 'id']
    }
  ],
  [
    'Messages',
    {
      keyPath: ['_canister', 'channel', 'id']
    }
  ]
])

// full key: `${principal}:CH:${canister}:${id}`
export function myChannelsRange(principal: string) {
  return IDBKeyRange.bound(`${principal}:CH:`, `${principal}:CH;`, true, true)
}

export function myChannelKey(principal: string, canister: string, id: number) {
  return `${principal}:CH:${canister}:${id}`
}
