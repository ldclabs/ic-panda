import { INTERNET_IDENTITY_CANISTER_ID, IS_LOCAL } from '$lib/constants'
import { anonymousIdentity, authClientPromise, dynAgent } from '$lib/utils/auth'
import { popupCenter } from '$lib/utils/window'
import { type Identity } from '@dfinity/agent'
import { nonNullish } from '@dfinity/utils'
import { derived, get, writable, type Readable } from 'svelte/store'

export interface AuthStoreData {
  identity: Identity
}

export interface AuthSignInParams {
  domain?: 'ic0.app' | 'internetcomputer.org'
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
  signIn: (params: AuthSignInParams) => Promise<void>
  signOut: () => Promise<void>
}

const initAuthStore = (): AuthStore => {
  const { subscribe, set } = writable<AuthStoreData>({
    identity: anonymousIdentity
  })

  return {
    subscribe,

    getIdentity: async () => {
      const authClient = await authClientPromise
      return authClient.getIdentity()
    },

    sync: async () => {
      const authClient = await authClientPromise
      const isAuthenticated = await authClient.isAuthenticated()
      dynAgent.setIdentity(authClient.getIdentity())
      if (isAuthenticated) {
        set({
          identity: authClient.getIdentity()
        })
      }
    },

    signIn: ({ domain }: AuthSignInParams) =>
      // eslint-disable-next-line no-async-promise-executor
      new Promise<void>(async (resolve, reject) => {
        const authClient = await authClientPromise

        const identityProvider =
          nonNullish(INTERNET_IDENTITY_CANISTER_ID) && IS_LOCAL
            ? `http://${INTERNET_IDENTITY_CANISTER_ID}.localhost:4943`
            : `https://identity.${domain ?? 'ic0.app'}`

        await authClient.login({
          // 7 days in nanoseconds
          maxTimeToLive: BigInt(7 * 24 * 60 * 60 * 1000 * 1000 * 1000),
          onSuccess: () => {
            dynAgent.setIdentity(authClient.getIdentity())

            set({
              identity: authClient.getIdentity()
            })

            resolve()
          },
          onError: reject,
          identityProvider,
          windowOpenerFeatures: popupCenter({
            width: 576,
            height: 625
          })
        })
      }),

    signOut: async () => {
      const authClient = await authClientPromise
      await authClient.logout()

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
