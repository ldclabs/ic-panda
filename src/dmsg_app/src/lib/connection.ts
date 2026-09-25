import type { Identity } from '@icp-sdk/core/agent'
import type { DelegationIdentity } from '@icp-sdk/core/identity'
import { login, services } from './services/ic'
import { AccountClient } from './services/account'
import { config } from './config'
import { ensure } from './errors'
import { session } from './session.svelte'

type Connection = { identity: Identity; api: Awaited<ReturnType<typeof services>> }
// Page memory only: delegations are dropped on lock and never written to storage.
const accounts = new Map<string, { connection: Promise<Connection>; expiresAt: number }>()
window.addEventListener('dmsg-cleared', () => accounts.clear())

/** A separately chosen identity, such as a payer or legacy owner. Always prompts. */
export async function connectIdentity(
  origin: string,
  targets?: string[]
): Promise<Connection> {
  ensure(session.meta, 'LOCKED', '请先解锁工作台。')
  const identity = await login(session.crypto, session.meta.transportPublic, origin, targets)
  return { identity, api: await services(identity) }
}
const expiry = (identity: Identity) =>
  Math.min(
    ...(identity as DelegationIdentity)
      .getDelegation()
      .delegations.map((d) => Number(d.delegation.expiration / 1_000_000n))
  )
/** The workspace account's delegation, shared by every panel until lock or expiry.
 * `fresh` prompts again; `bound: false` allows an identity not yet bound to it. */
export async function connectAccount(
  origin: string,
  options: { fresh?: boolean; bound?: boolean } = {}
) {
  const meta = session.meta
  ensure(meta, 'LOCKED', '请先解锁工作台。')
  const key = `${meta.transportPublic}:${origin}`
  let cached = accounts.get(key)
  if (options.fresh || !cached || cached.expiresAt < Date.now() + 60000) {
    const connection = connectIdentity(origin)
    cached = { connection, expiresAt: Number.MAX_SAFE_INTEGER }
    accounts.set(key, cached)
    try {
      cached.expiresAt = expiry((await connection).identity)
    } catch (error) {
      accounts.delete(key)
      throw error
    }
  }
  const { identity, api } = await cached.connection
  const account = new AccountClient(
    api.user!,
    api.agent,
    identity.getPrincipal(),
    session.crypto,
    meta,
    config.canisters.user
  )
  if (options.bound !== false) {
    ensure(meta.account, 'AUTH_REQUIRED', '请先建立正式工作区。')
    if ((await account.connectedAccount()) !== meta.account.id) {
      accounts.delete(key)
      throw new Error('登录身份与当前工作区不一致。')
    }
  }
  return { identity, api, account }
}
