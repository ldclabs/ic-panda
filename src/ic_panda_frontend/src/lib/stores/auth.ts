import { LOCAL_INTERNET_IDENTITY_CANISTER_ID, IS_LOCAL } from '$lib/constants'
import type { OptionIdentity } from '$lib/types/identity'
import { createAuthClient } from '$lib/utils/auth'
import { popupCenter } from '$lib/utils/window'
import type { AuthClient } from '@dfinity/auth-client'
import { nonNullish } from '@dfinity/utils'
import { writable, type Readable } from 'svelte/store'
import type { Identity } from '@dfinity/agent'

export interface AuthStoreData {
  identity: OptionIdentity
}

export interface AuthSignInParams {
  domain?: 'ic0.app' | 'internetcomputer.org'
}

export interface AuthStore extends Readable<AuthStoreData> {
  sync: () => Promise<void>
  getIdentity: () => Promise<Identity>
  signIn: (params: AuthSignInParams) => Promise<void>
  signOut: () => Promise<void>
}

const initAuthStore = (): AuthStore => {
  const authClientPromise = createAuthClient()
  const { subscribe, set, update } = writable<AuthStoreData>({
    identity: undefined,
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

      set({
        identity: isAuthenticated ? authClient.getIdentity() : null,
      })
    },

    signIn: ({ domain }: AuthSignInParams) =>
      // eslint-disable-next-line no-async-promise-executor
      new Promise<void>(async (resolve, reject) => {
        const authClient = await authClientPromise

        const identityProvider =
          nonNullish(LOCAL_INTERNET_IDENTITY_CANISTER_ID) && IS_LOCAL
            ? `http://${LOCAL_INTERNET_IDENTITY_CANISTER_ID}.localhost:4943`
            : `https://identity.${domain ?? 'ic0.app'}`

        await authClient.login({
          onSuccess: () => {
            update((state: AuthStoreData) => ({
              ...state,
              identity: authClient?.getIdentity(),
            }))

            resolve()
          },
          onError: reject,
          identityProvider,
          windowOpenerFeatures: popupCenter({
            width: 576,
            height: 625,
          }),
        })
      }),

    signOut: async () => {
      const authClient = await authClientPromise
      await authClient.logout()

      update((state: AuthStoreData) => ({
        ...state,
        identity: null,
      }))
    },
  }
}

export const authStore = initAuthStore()
