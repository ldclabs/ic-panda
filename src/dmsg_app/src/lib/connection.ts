import type { Identity } from '@icp-sdk/core/agent'
import type { DelegationIdentity } from '@icp-sdk/core/identity'
import { locateAccount, login, services, userActor } from './services/ic'
import { AccountClient } from './services/account'
import { ContentClient } from './services/content'
import { CloudClient } from './services/relay'
import { config } from './config'
import { ensure } from './errors'
import { session } from './session.svelte'

/** The workspace's login origin: the one bound at setup, else the first configured. */
export const loginOrigin = () => session.meta?.loginOrigin ?? config.derivationOrigins[0]
type Connection = { identity: Identity; api: Awaited<ReturnType<typeof services>> }
// Page memory only: delegations are dropped on lock and never written to storage.
const accounts = new Map<string, { connection: Promise<Connection>; expiresAt: number }>()
window.addEventListener('dmsg-cleared', () => accounts.clear())

/** A separately chosen identity, such as a payer or legacy owner. Always prompts. */
export async function connectIdentity(
  origin: string,
  targets?: string[]
): Promise<Connection> {
  const identity = await login(session.crypto, origin, targets)
  return { identity, api: await services(identity) }
}
const expiry = (identity: Identity) =>
  Math.min(
    ...(identity as DelegationIdentity)
      .getDelegation()
      .delegations.map((d) => Number(d.delegation.expiration / 1_000_000n))
  )
/** The login shared by every panel until lock or expiry. `fresh` prompts again. */
export async function connectLogin(origin: string, fresh = false): Promise<Connection> {
  let cached = accounts.get(origin)
  if (fresh || !cached || cached.expiresAt < Date.now() + 60000) {
    const connection = connectIdentity(origin)
    cached = { connection, expiresAt: Number.MAX_SAFE_INTEGER }
    accounts.set(origin, cached)
    try {
      cached.expiresAt = expiry((await connection).identity)
    } catch (error) {
      accounts.delete(origin)
      throw error
    }
  }
  return cached.connection
}
/** The workspace account's client at the account's own user home. The
 * workspace metadata is readable before unlock, so this also serves unlock. */
export async function connectAccount(
  origin: string,
  options: { fresh?: boolean; bound?: boolean; home?: string } = {}
) {
  const meta = session.meta
  ensure(meta, 'LOCKED', '此浏览器还没有工作台。')
  const { identity, api } = await connectLogin(origin, options.fresh)
  let home = options.home ?? meta.account?.homeUser
  if (!home) {
    const located = await locateAccount(api.agent)
    ensure(located, 'AUTH_REQUIRED', '此登录身份还没有账户。')
    home = located.home
  }
  const account = new AccountClient(
    userActor(api.agent, home),
    api.agent,
    identity.getPrincipal(),
    session.crypto,
    meta,
    home
  )
  if (options.bound !== false) {
    ensure(meta.account, 'AUTH_REQUIRED', '请先绑定账户。')
    if ((await account.connectedAccount()) !== meta.account.id) {
      accounts.delete(origin)
      throw new Error('登录身份与当前工作区不一致。')
    }
  }
  return { identity, api, account }
}
/** Verify the account's cloud snapshot and submit pending local versions.
 * The cloud holds the only copy of the vault outside this device. */
export async function syncContent(origin: string) {
  const id = session.meta?.account?.id
  ensure(id && config.relayOrigin, 'UNAVAILABLE', '请先绑定账户并配置云端服务。')
  const { account } = await connectAccount(origin)
  const result = await new ContentClient(
    account,
    new CloudClient({ origin: config.relayOrigin, environment: config.environment }),
    id
  ).sync()
  await session.refresh()
  return result
}
