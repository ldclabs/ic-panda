import { IS_LOCAL } from '$lib/constants'
import {
  AnonymousIdentity,
  HttpAgent,
  type HttpAgentOptions,
  type Identity
} from '@dfinity/agent'
import { AuthClient } from '@dfinity/auth-client'

export const authClientPromise = AuthClient.create({
  keyType: 'Ed25519',
  idleOptions: {
    disableIdle: true,
    disableDefaultIdleCallback: true
  }
})

/**
 * In certain features, we want to execute jobs with the authenticated identity without getting it from the auth.store.
 * This is notably useful for Web Workers which do not have access to the window.
 */
export const loadIdentity = async (): Promise<Identity | undefined> => {
  const authClient = await authClientPromise
  const authenticated = await authClient.isAuthenticated()

  agent.setIdentity(authClient.getIdentity())
  // Not authenticated therefore we provide no identity as a result
  if (!authenticated) {
    return undefined
  }

  return authClient.getIdentity()
}

export function shortId(id: string, long: boolean = false): string {
  if (long) {
    return id.length > 28 ? id.slice(0, 14) + '...' + id.slice(-14) : id
  }
  return id.length > 14 ? id.slice(0, 7) + '...' + id.slice(-7) : id
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

export const agent = new AuthAgent({
  identity: anonymousIdentity,
  host: IS_LOCAL ? 'http://localhost:4943/' : 'https://icp-api.io',
  verifyQuerySignatures: false
})
