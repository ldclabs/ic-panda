import { IS_LOCAL } from '$lib/constants'
import {
  AnonymousIdentity,
  HttpAgent,
  type HttpAgentOptions,
  type Identity
} from '@icp-sdk/core/agent'
import { AuthClient, type AuthClientCreateOptions } from '@icp-sdk/auth/client'

let authClient: AuthClient | undefined

export function getAuthClient(
  options?: AuthClientCreateOptions,
  replace = false
): AuthClient {
  if (!authClient || replace) {
    authClient = new AuthClient({
      keyType: 'Ed25519',
      ...options,
      idleOptions: {
        disableIdle: true,
        disableDefaultIdleCallback: true,
        ...options?.idleOptions
      }
    })
  }
  return authClient
}

/**
 * In certain features, we want to execute jobs with the authenticated identity without getting it from the auth.store.
 * This is notably useful for Web Workers which do not have access to the window.
 */
export const loadIdentity = async (): Promise<Identity | undefined> => {
  const authClient = getAuthClient()
  const authenticated = authClient.isAuthenticated()
  const identity = await authClient.getIdentity()

  dynAgent.setIdentity(identity)
  // Not authenticated therefore we provide no identity as a result
  if (!authenticated) {
    return undefined
  }

  return identity
}

export class AuthAgent extends HttpAgent {
  private _id: Identity
  constructor(options: { identity: Identity } & HttpAgentOptions) {
    super(options)
    this._id = options.identity
  }

  get id() {
    return this._id
  }

  setIdentity(id: Identity) {
    this._id = id
    super.replaceIdentity(id)
  }
}

export const anonymousIdentity = new AnonymousIdentity()

export const dynAgent = new AuthAgent({
  identity: anonymousIdentity,
  host: IS_LOCAL ? 'http://localhost:4943/' : 'https://icp-api.io',
  verifyQuerySignatures: false,
  shouldFetchRootKey: IS_LOCAL
})

export const anonAgent = new AuthAgent({
  identity: anonymousIdentity,
  host: 'https://icp-api.io',
  verifyQuerySignatures: false,
  shouldFetchRootKey: IS_LOCAL
})
