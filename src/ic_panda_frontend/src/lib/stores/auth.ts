import { IS_LOCAL } from '$lib/constants'
import { anonymousIdentity, dynAgent, getAuthClient } from '$lib/utils/auth'
import { popupCenter } from '$lib/utils/window'
import { type Identity } from '@icp-sdk/core/agent'
import { derived, get, writable, type Readable } from 'svelte/store'

const IDENTITY_PROVIDER = 'https://id.ai/authorize'

export interface AuthStoreData {
  identity: Identity
}

// Fetch the root key for local development
export async function fetchRootKey() {
  if (IS_LOCAL) {
    await Promise.all([dynAgent.fetchRootKey(), dynAgent.syncTime()])
  }
}

export interface AuthStore extends Readable<AuthStoreData> {
  sync: () => Promise<void>
  getIdentity: () => Promise<Identity>
  signIn: () => Promise<void>
  signOut: () => Promise<void>
}

const initAuthStore = (): AuthStore => {
  const { subscribe, set } = writable<AuthStoreData>({
    identity: anonymousIdentity
  })

  return {
    subscribe,

    getIdentity: async () => {
      const authClient = getAuthClient()
      return authClient.getIdentity()
    },

    sync: async () => {
      const authClient = getAuthClient()
      const isAuthenticated = authClient.isAuthenticated()
      const identity = await authClient.getIdentity()
      dynAgent.setIdentity(identity)
      if (isAuthenticated) {
        set({ identity })
      }
    },

    signIn: async () => {
      const identityProvider = IDENTITY_PROVIDER
      const authClient = getAuthClient(
        {
          identityProvider,
          windowOpenerFeatures: popupCenter({
            width: 576,
            height: 625
          })
        },
        true
      )
      const identity = await authClient.signIn({
        // 7 days in nanoseconds
        maxTimeToLive: BigInt(7 * 24 * 60 * 60 * 1000 * 1000 * 1000)
      })
      dynAgent.setIdentity(identity)
      set({ identity })
    },

    signOut: async () => {
      const authClient = getAuthClient()
      await authClient.signOut()

      dynAgent.setIdentity(anonymousIdentity)
      set({
        identity: anonymousIdentity
      })
    }
  }
}

export const authStore = initAuthStore()

export interface AsyncReadable<T> extends Readable<T> {
  async(): Promise<T>
}

export async function asyncFactory<T>(
  factory: (id: Identity) => Promise<T>
): Promise<AsyncReadable<T>> {
  let id: Identity = anonymousIdentity
  let promise = factory(id)
  let value: T = await promise

  const r = derived(
    authStore,
    ($authStore, set) => {
      if ($authStore.identity !== id) {
        id = $authStore.identity
        promise = factory(id)
        promise.then(set)
      }
    },
    value
  )

  return {
    ...r,
    async: () => {
      get(r) // trigger the derived store to update inner value
      return promise
    }
  }
}
