import { toast } from '../components/toast.svelte'
import { t } from '../i18n/index.svelte'
import { call, code } from '../rpc'

/**
 * The session actions the sidebar and the command palette share. Their
 * outcome reaches the windows as daemon events; a failure shows as a notice.
 */
async function attempt(work: () => Promise<unknown>) {
  try {
    await work()
  } catch (error) {
    toast(() => `${t('error')}: ${code(error)}`)
  }
}

/** Drop the login and every key now, as the idle lock would. */
export const lock = () => attempt(() => call('session/lock'))

/** Open the account again: the OS keychain when fast unlock is on, else a login in the system browser. */
export const unlock = (status: any) =>
  attempt(() => (status?.fast_unlock ? call('session/unlock', { fast: true }) : window.dmsg.login('https://dmsg.net')))

export const sync = () => attempt(() => call('sync/run'))

/** A login with another identity opens its own account's workspace. */
export const loginOther = () => attempt(() => window.dmsg.login('https://dmsg.net'))

/** Another account's workspace opens locked; its fast unlock or a login opens it. */
export const switchAccount = (account: string, fast: boolean) =>
  attempt(() => call(fast ? 'session/unlock' : 'session/switch', fast ? { fast: true, account } : { account }))

/** An account ID shortened for display; the full ID stays one copy away. */
export const shortId = (id: string) => (id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id)
