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
import {
  requestIdOf,
  type DerEncodedPublicKey,
  type Signature
} from '@dfinity/agent'
import {
  Delegation,
  DelegationChain,
  DelegationIdentity,
  Ed25519KeyIdentity,
  Ed25519PublicKey
} from '@dfinity/identity'
import type { Principal } from '@dfinity/principal'
import { base64ToBytes, bytesToBase64Url } from '@ldclabs/cose-ts/utils'
import { decode, encode } from 'cborg'
import { writable, type Readable } from 'svelte/store'

export interface AuthStoreData {
  identity: IdentityEx
}

export interface InternetIdentityAuthResponseSuccess {
  kind: 'authorize-client-success'
  delegations: {
    delegation: {
      pubkey: Uint8Array
      expiration: bigint
      targets?: Principal[]
    }
    signature: Uint8Array
  }[]
  userPublicKey: Uint8Array
  authnMethod: 'passkey' | 'pin' | 'recovery'
}

const IDENTITY_PROVIDER = IS_LOCAL
  ? 'http://rdmx6-jaaaa-aaaaa-aaadq-cai.localhost:4943'
  : 'https://identity.ic0.app'

const DERIVATION_ORIGIN = IS_LOCAL ? undefined : 'https://panda.fans'

const IC_REQUEST_AUTH_DELEGATION_DOMAIN_SEPARATOR = new TextEncoder().encode(
  '\x1Aic-request-auth-delegation'
)

export interface AuthStore extends Readable<AuthStoreData> {
  nameIdentityAPI: NameIdentityAPI
  get srcIdentity(): IdentityEx | null
  get identity(): IdentityEx | null
  ready(): Promise<void>
  sync: () => Promise<void>
  switch: (username: string) => Promise<void>
  signIn: () => Promise<void>
  signIn2: () => Promise<void>
  deepLinkSignIn: (deepLinkSignInRequest: string) => Promise<string>
  logout: (url: string) => Promise<void>
}

// Fetch the root key for local development
async function fetchRootKey() {
  if (IS_LOCAL) {
    await Promise.all([dynAgent.fetchRootKey(), dynAgent.syncTime()])
  }
}

function initAuthStore(): AuthStore {
  const authClientPromise = createAuthClient()
  let authnMethod = ''
  let authnOrign = ''
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

    ready: async () => {
      await fetchRootKey()
      await authClientPromise
    },

    sync: async () => {
      const authClient = await authClientPromise
      srcIdentity = await loadIdentity(authClient)
      if (srcIdentity) {
        srcAgent.setIdentity(srcIdentity)
      }

      const nameIdentity = await loadNameIdentity()
      if (nameIdentity) {
        authnMethod = 'dMsg' // dMsg name identity
      }

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

    signIn: () =>
      new Promise<void>(async (resolve, reject) => {
        // Important: authClientPromise should be resolved here
        // https://ffan0811.medium.com/window-open-returns-null-in-safari-and-firefox-after-allowing-pop-up-on-the-browser-4e4e45e7d926
        const authClient = await authClientPromise
        await authClient.login({
          derivationOrigin: DERIVATION_ORIGIN as string,
          maxTimeToLive: BigInt(EXPIRATION_MS) * 1000000n,
          onSuccess: (msg) => {
            authnMethod = msg.authnMethod
            authnOrign = DERIVATION_ORIGIN || location.origin
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
          onError: (err) => {
            console.error(err)
            reject(err)
          },
          identityProvider: IDENTITY_PROVIDER,
          windowOpenerFeatures: popupCenter({
            width: 576,
            height: 625
          })
        })
      }),

    signIn2: () =>
      new Promise<void>(async (resolve, reject) => {
        // Important: authClientPromise should be resolved here
        // https://ffan0811.medium.com/window-open-returns-null-in-safari-and-firefox-after-allowing-pop-up-on-the-browser-4e4e45e7d926
        const authClient = await authClientPromise
        await authClient.login({
          maxTimeToLive: BigInt(EXPIRATION_MS) * 1000000n,
          onSuccess: (msg) => {
            authnMethod = msg.authnMethod
            authnOrign = location.origin

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
          onError: (err) => {
            console.error(err)
            reject(err)
          },
          windowOpenerFeatures: popupCenter({
            width: 576,
            height: 625
          })
        })
      }),

    deepLinkSignIn: async (deepLinkSignInRequest: string) => {
      if (!identity) {
        throw new Error('No signed identity found')
      }

      if (!(identity.id instanceof DelegationIdentity)) {
        throw new Error('Identity is not a DelegationIdentity')
      }

      const request: DeepLinkSignInRequest = decodeCBORString(
        deepLinkSignInRequest
      )

      const expiration =
        BigInt(Date.now() + (request.m || EXPIRATION_MS)) * BigInt(1000000)
      const delegation: Delegation = new Delegation(
        toArrayBuffer(request.s),
        expiration // In nanoseconds.
      )
      const challenge = new Uint8Array([
        ...IC_REQUEST_AUTH_DELEGATION_DOMAIN_SEPARATOR,
        ...new Uint8Array(requestIdOf({ ...delegation }))
      ])
      const signature = await identity.id.sign(toArrayBuffer(challenge))
      const chain = identity.id.getDelegation()
      const res: DeepLinkSignInResponse = {
        u: new Uint8Array(chain.publicKey),
        d: [
          ...chain.delegations,
          {
            delegation,
            signature
          }
        ].map((delegation) => {
          return {
            d: delegation.delegation.targets
              ? {
                  p: new Uint8Array(delegation.delegation.pubkey),
                  e: delegation.delegation.expiration,
                  t: delegation.delegation.targets.map((t) => t.toUint8Array())
                }
              : {
                  p: new Uint8Array(delegation.delegation.pubkey),
                  e: delegation.delegation.expiration
                },
            s: new Uint8Array(delegation.signature)
          }
        }),
        a: authnMethod,
        o: authnOrign
      }

      return encodeCBORString(res)
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

// export async function signIn(g2: Boolean | null): Promise<{
//   result: 'ok' | 'cancelled' | 'error'
//   error?: unknown
// }> {
//   try {
//     if (g2) {
//       await authStore.signIn2()
//     } else {
//       await authStore.signIn()
//     }

//     return { result: 'ok' }
//   } catch (error: unknown) {
//     if (error === 'UserInterrupt') {
//       // We do not display an error if user explicitly cancelled the process of sign-in
//       return { result: 'cancelled' }
//     }

//     return { result: 'error', error }
//   } finally {
//   }
// }

function decodeCBORString<T>(payload: string): T {
  const data = base64ToBytes(payload)
  return decode(data)
}

function encodeCBORString<T>(obj: T): string {
  const data = encode(obj)
  return bytesToBase64Url(new Uint8Array(data))
}

// pub struct SignInRequest {
//     #[serde(rename = "s")]
//     pub session_pubkey: ByteBufB64,
//     #[serde(rename = "m")]
//     pub max_time_to_live: u64, // in miniseconds
// }
interface DeepLinkSignInRequest {
  s: Uint8Array
  m: number
}

// pub struct Delegation {
//     /// The delegated-to key.
//     #[serde(alias = "p")]
//     pub pubkey: ByteBufB64,
//     /// A nanosecond timestamp after which this delegation is no longer valid.
//     #[serde(alias = "e")]
//     pub expiration: u64,
//     /// If present, this delegation only applies to requests sent to one of these canisters.
//     #[serde(default, skip_serializing_if = "Option::is_none")]
//     #[serde(alias = "t")]
//     pub targets: Option<Vec<Principal>>,
// }
//
// pub struct SignedDelegation {
//     /// The signed delegation.
//     #[serde(alias = "d")]
//     pub delegation: Delegation,
//     /// The signature for the delegation.
//     #[serde(alias = "s")]
//     pub signature: ByteBufB64,
// }
//
// pub struct SignInResponse {
//     #[serde(rename = "u")]
//     pub user_pubkey: ByteBufB64,
//     #[serde(rename = "d")]
//     pub delegations: Vec<SignedDelegation>,
//     #[serde(rename = "a")]
//     pub authn_method: String,
//     #[serde(rename = "o")]
//     pub origin: String,
// }
interface DeepLinkSignInResponse {
  u: Uint8Array
  d: {
    d: { p: Uint8Array; e: bigint; t?: Uint8Array[] }
    s: Uint8Array
  }[]
  a: String
  o: String
}
