<script lang="ts">
  import { Tooltip } from 'bits-ui'
  import { onMount } from 'svelte'
  import { toast } from './components/toast.svelte'
  import { notificationBody } from '../shared/notification-text'
  import { isNoticePlan } from '../shared/notifications'
  import Toaster from './components/Toaster.svelte'
  import Collapsible from './components/Collapsible.svelte'
  import Icon from './components/Icon.svelte'
  import { buttonClass } from './components/ui'
  import RecoveryBanner from './shell/RecoveryBanner.svelte'
  import { startLogin } from './shell/login.svelte'
  import { call, single } from './rpc'
  import { clearDisplays } from './display.svelte'
  import { locale, t } from './i18n/index.svelte'
  import { namesSession } from './views/channels/names.svelte'
  import { forgetPlain, settleMemory } from './view-memory'

  /**
   * What both windows share: the route, the daemon's status and the page that
   * covers a lost daemon. The approval window shows one request, and the main
   * window's shell loads only there.
   */
  const isMain = window.dmsg.role === 'main'
  const Shell = isMain ? import('./shell/MainShell.svelte') : null
  const Single = isMain ? null : import('./views/Approval.svelte')

  let route = $state(location.hash.slice(1) || '/channels')
  type ViewSession = { key: string; status: any }
  type Connection =
    | { kind: 'verifying'; view: ViewSession | null }
    | { kind: 'disconnected'; view: ViewSession | null; error?: string }
    | { kind: 'verified'; view: ViewSession }
  let connection = $state<Connection>({ kind: 'verifying', view: null })
  const status = $derived(connection.view?.status ?? null)
  const viewSession = $derived(connection.view?.key ?? null)
  const verifiedConnection = $derived(connection.kind === 'verified')
  const connectionError = $derived(connection.kind === 'disconnected' ? connection.error : undefined)
  let reconnecting = $state(false)
  /** The status last read: the shell keeps showing it while the next is read, where the views are gone at once. */
  let lastStatus = $state<any>(null)
  /** The window has waited long enough for its daemon that saying so is no flash. */
  let waited = $state(false)
  /** How many accounts this device holds, read while logged out: none means a first run. */
  let accounts = $state<number | null>(null)

  const blocked = $derived(!verifiedConnection)
  // A session change destroyed the views at once, so nothing needs covering
  // unless the wait lasts. A lost connection keeps its forms: those cover at once.
  const covered = $derived(blocked && (viewSession !== null || waited))
  const failed = $derived(waited && connection.kind === 'disconnected' && !!connectionError)

  let stateRevision = 0

  const refresh = single(async () => {
    const revision = stateRevision
    const remote = await window.dmsg.connection()
    if (revision !== stateRevision) return
    if (!remote.connected) { connection = { kind: 'disconnected', view: connection.view, error: remote.error }; return }
    try {
      const next: any = await call('daemon/status')
      if (revision !== stateRevision) return
      if (isMain && next.state === 'logged_out') {
        const known = await call<any[]>('session/accounts').then((list) => list.length, () => 1)
        if (revision !== stateRevision) return
        accounts = known
      }
      // Without an opaque token, a reconnect cannot prove it is the same
      // unlocked session (for example while the installed daemon is older).
      const key = `${next.session || `unknown:${revision}`}:${next.account ?? ''}:${next.state}`
      if (viewSession !== key) clearDisplays()
      namesSession(next)
      settleMemory(next)
      lastStatus = next
      connection = { kind: 'verified', view: { key, status: next } }
    } catch {
      if (revision !== stateRevision) return
      connection = { kind: 'verifying', view: connection.view }
    }
  })

  // Portalled dialogs live outside the app's inert wrapper. Mask those too,
  // while retaining same-session forms only for a connection interruption.
  $effect(() => {
    document.body.toggleAttribute('data-daemon-unverified', blocked)
    return () => document.body.removeAttribute('data-daemon-unverified')
  })

  $effect(() => {
    waited = false
    if (!blocked) return
    const timer = setTimeout(() => (waited = true), 400)
    return () => clearTimeout(timer)
  })

  async function reconnect() {
    reconnecting = true
    try {
      await window.dmsg.reconnect()
      await refresh()
    } catch {
      /* the page keeps saying why */
    } finally {
      reconnecting = false
    }
  }

  const go = (next: string) => (location.hash = next)

  onMount(() => {
    // The route follows the hash only, so a view the hash still names opens again on a click.
    // `/login` is not a view: the approval window asks the main window to log in.
    const offNavigate = window.dmsg.onNavigate((next) => (next === '/login' ? void startLogin() : go(next)))
    const onHash = () => (route = location.hash.slice(1) || '/channels')
    window.addEventListener('hashchange', onHash)
    const offs = [offNavigate, () => window.removeEventListener('hashchange', onHash)]
    void refresh()
    offs.push(window.dmsg.onEvent((event) => {
      if (event.type === 'session/changed' || event.type === 'connection/changed') {
        stateRevision++
        connection = { kind: 'verifying', view: event.type === 'session/changed' ? null : connection.view }
        document.body.setAttribute('data-daemon-unverified', '')
        if (event.type === 'session/changed') {
          // Destroy plaintext views immediately, even if the subsequent status
          // query is blocked or the daemon disconnects before it can answer.
          namesSession(null)
          forgetPlain()
          clearDisplays()
        }
        void refresh()
      } else if (event.type === 'resync') void refresh()
      else if (event.type === 'cache/changed' && (event.data as any)?.kind === 'all') {
        namesSession(null)
        namesSession(status)
      }
    }))
    offs.push(window.dmsg.onEvent((event) => {
      if (event.type !== 'notifications/present' || !isNoticePlan(event.data) || !document.hasFocus()) return
      const plan = event.data
      // A relay hint also echoes our own sends. It wakes reconciliation, but
      // cannot yet establish an incoming message worth interrupting the user.
      if (plan.category === 'message' && plan.count === null) return
      toast(() => notificationBody(locale(), plan.category, plan.count), { testid: 'notification-toast', ms: 6000,
        action: { label: t('open'), run: () => window.dmsg.notificationOpen(plan.account, plan.covered.length === 1 ? plan.covered[0].id : plan.category) } })
    }))
    return () => offs.forEach((off) => off())
  })
</script>

{#if covered}
  <div class="fixed inset-0 z-[100] grid place-items-center bg-paper p-6 [-webkit-app-region:drag]" role="status" data-testid="daemon-disconnected">
    {#if failed}
      <div class="grid w-[min(460px,100%)] gap-3 [-webkit-app-region:no-drag]" data-testid="daemon-failed">
        <span class="text-danger [&_svg]:size-6"><Icon name="warning" /></span>
        <h1 class="text-[15px] font-semibold">{t('daemonFailed')}</h1>
        <p class="text-[13px] text-muted">{t('daemonFailedHint')}</p>
        <div class="flex flex-wrap gap-2">
          <button class={buttonClass('primary')} disabled={reconnecting} aria-busy={reconnecting} onclick={reconnect} data-testid="daemon-retry">{t('retry')}</button>
          <button class={buttonClass('ghost')} onclick={() => void window.dmsg.quit()} data-testid="daemon-quit">{t('quitApp')}</button>
        </div>
        <Collapsible testid="daemon-details">
          {#snippet title()}<span class="text-[12.5px] text-muted">{t('technicalDetails')}</span>{/snippet}
          <pre class="max-h-48 overflow-auto rounded-lg bg-surface-2 p-3 font-mono text-[11.5px] break-words whitespace-pre-wrap select-text" dir="ltr">{connectionError}</pre>
        </Collapsible>
      </div>
    {:else if waited}
      {t('daemonDisconnected')}
    {/if}
  </div>
{/if}
<!-- Shared controls, including approval identifiers, need this in both windows. -->
<Tooltip.Provider delayDuration={350}>
{#if Single}
  <main class="h-screen" inert={blocked} aria-hidden={blocked}>
    <RecoveryBanner />
    {#if route.startsWith('/approval/')}
      {#await Single then { default: Approval }}
        {#key `${viewSession}:${route}`}
          <Approval id={route.slice('/approval/'.length)} />
        {/key}
      {/await}
    {/if}
  </main>
  <Toaster />
{:else if Shell}
  {#await Shell then { default: MainShell }}
    <MainShell {status} {lastStatus} {route} {viewSession} verified={verifiedConnection} {blocked} {accounts} {refresh} {go} />
  {/await}
{/if}
</Tooltip.Provider>

<style>
  :global(body[data-daemon-unverified] > :not(#app)) { visibility: hidden !important; }
</style>
