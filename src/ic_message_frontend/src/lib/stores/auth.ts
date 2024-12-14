import { NameIdentityAPI } from '$lib/canisters/nameidentity'
import { IS_LOCAL } from '$lib/constants'
import {
  anonymousIdentity,
  createAgent,
  createAuthClient,
  dynAgent,
  EXPIRATION_MS,
  IdentityEx,
  loadIdentity,
  loadNameIdentity,
  logout,
  setNameIdentity
} from '$lib/utils/auth'
import { encodeCBOR, toArrayBuffer } from '$lib/utils/crypto'
import { popupCenter } from '$lib/utils/window'
import type { DerEncodedPublicKey, Identity, Signature } from '@dfinity/agent'
import {
  Delegation,
  DelegationChain,
  Ed25519KeyIdentity,
  Ed25519PublicKey
} from '@dfinity/identity'
import { writable, type Readable } from 'svelte/store'

export interface AuthStoreData {
  identity: Identity
}

export interface AuthSignInParams {
  domain?: 'ic0.app' | 'internetcomputer.org'
}

const IDENTITY_PROVIDER = IS_LOCAL
  ? 'http://rdmx6-jaaaa-aaaaa-aaadq-cai.localhost:4943'
  : 'https://identity.ic0.app'

const DERIVATION_ORIGIN = IS_LOCAL ? undefined : 'https://panda.fans'

// Fetch the root key for local development
export async function fetchRootKey() {
  if (IS_LOCAL) {
    await Promise.all([dynAgent.fetchRootKey(), dynAgent.syncTime()])
  }
}

export interface AuthStore extends Readable<AuthStoreData> {
  nameIdentityAPI: NameIdentityAPI
  get srcIdentity(): IdentityEx | null
  get identity(): IdentityEx | null
  sync: () => Promise<void>
  switch: (username: string) => Promise<void>
  signIn: () => Promise<void>
  logout: (url: string) => Promise<void>
}

function initAuthStore(): AuthStore {
  let identity: IdentityEx | null = null
  let srcIdentity: IdentityEx | null = null
  // srcAgent is used to sign in with username
  const srcAgent = createAgent(anonymousIdentity)
  if (IS_LOCAL) {
    Promise.all([srcAgent.fetchRootKey(), srcAgent.syncTime()])
  }

  const { subscribe, set } = writable<AuthStoreData>({
    identity: anonymousIdentity
  })

  const nameIdentityAPI = new NameIdentityAPI(srcAgent)
  const store = {
    subscribe,
    nameIdentityAPI,
    get srcIdentity() {
      return srcIdentity
    },
    get identity() {
      return identity
    },

    sync: async () => {
      srcIdentity = await loadIdentity()
      if (srcIdentity) {
        srcAgent.setIdentity(srcIdentity)
      }

      const nameIdentity = await loadNameIdentity()
      identity = nameIdentity || srcIdentity
      dynAgent.setIdentity(identity || anonymousIdentity)
      set({
        identity: identity || anonymousIdentity
      })
    },

    switch: async (username: string = '') => {
      if (!srcIdentity) {
        return store.signIn()
      }

      if (!username) {
        await setNameIdentity(null)
        window.location.assign('/_/messages')
        return
      }

      const sessionKey = Ed25519KeyIdentity.generate()
      const pubkey = new Uint8Array(
        (sessionKey.getPublicKey() as Ed25519PublicKey).toDer()
      )
      const msg = encodeCBOR([
        username.toLocaleLowerCase(),
        srcIdentity.getPrincipal().toUint8Array()
      ])
      const sig = await sessionKey.sign(toArrayBuffer(msg))
      const res = await nameIdentityAPI.sign_in(
        username,
        pubkey,
        new Uint8Array(sig)
      )
      const signedDelegation = await nameIdentityAPI.get_delegation(
        res.seed as Uint8Array,
        pubkey,
        res.expiration
      )

      const chain = DelegationChain.fromDelegations(
        [
          {
            delegation: new Delegation(
              toArrayBuffer(signedDelegation.delegation.pubkey),
              signedDelegation.delegation.expiration,
              signedDelegation.delegation.targets[0]
            ),
            signature: toArrayBuffer(
              signedDelegation.signature,
              '__signature__'
            ) as Signature
          }
        ],
        toArrayBuffer(
          res.user_key,
          '__derEncodedPublicKey__'
        ) as DerEncodedPublicKey
      )

      await setNameIdentity({
        username,
        identity: sessionKey,
        chain
      })

      window.location.assign('/_/messages')
    },

    signIn: async () => {
      const authClient = await createAuthClient()
      return new Promise<void>((resolve, reject) => {
        authClient.login({
          derivationOrigin: DERIVATION_ORIGIN as string,
          maxTimeToLive: BigInt(EXPIRATION_MS) * 1000000n,
          onSuccess: () => {
            srcIdentity = new IdentityEx(
              authClient.getIdentity(),
              Date.now() + EXPIRATION_MS
            )
            identity = srcIdentity
            dynAgent.setIdentity(identity)
            srcAgent.setIdentity(identity)

            set({
              identity
            })

            resolve()
          },
          onError: reject,
          identityProvider: IDENTITY_PROVIDER,
          windowOpenerFeatures: popupCenter({
            width: 576,
            height: 625
          })
        })
      })
    },

    logout: async (url: string) => {
      dynAgent.setIdentity(anonymousIdentity)
      srcAgent.setIdentity(anonymousIdentity)
      set({
        identity: anonymousIdentity
      })
      await logout(url)
    }
  }

  return store
}

export const authStore = initAuthStore()

export async function signIn(): Promise<{
  result: 'ok' | 'cancelled' | 'error'
  error?: unknown
}> {
  try {
    await authStore.signIn()

    return { result: 'ok' }
  } catch (error: unknown) {
    if (error === 'UserInterrupt') {
      // We do not display an error if user explicitly cancelled the process of sign-in
      return { result: 'cancelled' }
    }

    return { result: 'error', error }
  } finally {
  }
}
