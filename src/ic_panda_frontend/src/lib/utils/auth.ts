import type { Identity } from '@dfinity/agent'
import { AuthClient } from '@dfinity/auth-client'
import { Principal } from '@dfinity/principal'

export const createAuthClient = (): Promise<AuthClient> =>
  AuthClient.create({
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
  const authClient = await createAuthClient()
  const authenticated = await authClient.isAuthenticated()

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

export function restorePrincipalObject(obj: any): any {
  if (!obj || obj instanceof Principal || obj instanceof Uint8Array) {
    return obj
  }

  if (Array.isArray(obj)) {
    for (let i = 0; i < obj.length; i++) {
      obj[i] = restorePrincipalObject(obj[i])
    }
    return obj
  }

  if (typeof obj === 'object') {
    if (obj._isPrincipal === true) {
      return Principal.from(obj)
    }

    for (const key of Object.keys(obj)) {
      obj[key] = restorePrincipalObject(obj[key])
    }
  }

  return obj
}
