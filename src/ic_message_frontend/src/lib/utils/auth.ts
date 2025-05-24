import { IS_LOCAL } from '$lib/constants'
import {
  AnonymousIdentity,
  HttpAgent,
  type HttpAgentOptions,
  type HttpAgentRequest,
  type Identity
} from '@dfinity/agent'
import { AuthClient, IdbStorage } from '@dfinity/auth-client'
import {
  DelegationChain,
  DelegationIdentity,
  Ed25519KeyIdentity,
  isDelegationValid
} from '@dfinity/identity'
import type { Principal } from '@dfinity/principal'

export const EXPIRATION_MS = 1000 * 60 * 60 * 24 * 30 // 30 days

export class IdentityEx implements Identity {
  constructor(
    public readonly id: Identity,
    public readonly expiration: number, // in milliseconds
    public readonly username: string = '' // this is name identity if username exists
  ) {
    this.id = id
    this.expiration = id.getPrincipal().isAnonymous()
      ? Number.MAX_SAFE_INTEGER
      : expiration
    this.username = username
  }

  get isExpired() {
    return Date.now() >= this.expiration - 1000 * 60 * 5 // 3 minutes before expiration
  }

  get isAuthenticated() {
    return !this.id.getPrincipal().isAnonymous() && !this.isExpired
  }

  getPrincipal(): Principal {
    return this.id.getPrincipal()
  }

  transformRequest(request: HttpAgentRequest): Promise<unknown> {
    if (this.isExpired) {
      setTimeout(() => logout('/'), 5000)
      throw new Error('Identity expired, please sign in again')
    }

    return this.id.transformRequest(request)
  }
}

export const anonymousIdentity = new IdentityEx(new AnonymousIdentity(), 0)

// II auth storage
const storage = new IdbStorage()

// should create a new authClient for each login
export function createAuthClient(): Promise<AuthClient> {
  return AuthClient.create({
    keyType: 'Ed25519',
    idleOptions: {
      disableIdle: true,
      disableDefaultIdleCallback: true
    }
  })
}

const KEY_STORAGE_NAME_IDENTITY = 'dmsg_delegation_identity'
export interface NameDelegationIdentity {
  username: string
  identity: Ed25519KeyIdentity
  chain: DelegationChain
}

interface JsonnableNameDelegationIdentity {
  username: string
  identity: string
  chain: string
}

export async function loadNameIdentity(): Promise<IdentityEx | null> {
  try {
    const obj = await storage.get<JsonnableNameDelegationIdentity>(
      KEY_STORAGE_NAME_IDENTITY
    )
    if (obj) {
      const key = Ed25519KeyIdentity.fromJSON(obj.identity)
      const chain = DelegationChain.fromJSON(obj.chain)
      if (isDelegationValid(chain)) {
        const expiration = getDelegationExpiration(chain)
        const id = DelegationIdentity.fromDelegation(key, chain)
        return new IdentityEx(id, expiration, obj.username)
      }
      await storage.remove(KEY_STORAGE_NAME_IDENTITY)
    }
  } catch (e) {
    console.error(e)
    await storage.remove(KEY_STORAGE_NAME_IDENTITY)
  }

  return null
}

export async function setNameIdentity(
  id: NameDelegationIdentity | null
): Promise<void> {
  if (id) {
    const obj: JsonnableNameDelegationIdentity = {
      username: id.username,
      identity: JSON.stringify(id.identity.toJSON()),
      chain: JSON.stringify(id.chain.toJSON())
    }
    await storage.set(KEY_STORAGE_NAME_IDENTITY, obj)
  } else {
    await storage.remove(KEY_STORAGE_NAME_IDENTITY)
  }
}

export async function loadIdentity(
  client?: AuthClient
): Promise<IdentityEx | null> {
  const authClient = client || (await createAuthClient())
  const authenticated = await authClient.isAuthenticated()

  // Not authenticated therefore we provide no identity as a result
  if (authenticated) {
    const expiration = await tryGetDelegationExpiration()
    return new IdentityEx(authClient.getIdentity(), expiration)
  }

  return null
}

const KEY_STORAGE_DELEGATION = 'delegation'
async function tryGetDelegationExpiration(): Promise<number> {
  let expiration = Date.now() + EXPIRATION_MS

  try {
    const delegation = await storage.get(KEY_STORAGE_DELEGATION)
    if (delegation) {
      const chain = DelegationChain.fromJSON(delegation)
      expiration = getDelegationExpiration(chain)
    }
  } catch (e) {}

  return expiration
}

function getDelegationExpiration(chain: DelegationChain): number {
  let expiration = Date.now() + EXPIRATION_MS
  for (const { delegation } of chain.delegations) {
    // prettier-ignore
    const ex = Number(delegation.expiration / BigInt(1000000))
    if (ex < expiration) {
      expiration = ex
    }
  }
  return expiration
}

export async function logout(url?: string): Promise<void> {
  dynAgent.setIdentity(anonymousIdentity)
  await setNameIdentity(null)
  const authClient = await createAuthClient()
  await authClient.logout()
  url && window.location.assign(url) // force reload to clear all auth state!!
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

  isAnonymous() {
    return this._id.getPrincipal().isAnonymous()
  }

  setIdentity(id: Identity) {
    this._id = id
    super.replaceIdentity(id)
  }
}

export function createAgent(identity: Identity): AuthAgent {
  return new AuthAgent({
    identity,
    host: IS_LOCAL ? 'http://localhost:4943/' : 'https://icp-api.io',
    verifyQuerySignatures: false
  })
}

export const dynAgent = createAgent(anonymousIdentity)
export const anonAgent = new AuthAgent({
  identity: anonymousIdentity,
  host: 'https://icp-api.io',
  verifyQuerySignatures: false
})
