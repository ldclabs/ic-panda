<script lang="ts">
  import { Tooltip } from 'bits-ui'
  import { onMount } from 'svelte'
  import Vault from './views/Vault.svelte'
  import Files from './views/Files.svelte'
  import Approvals from './views/Approvals.svelte'
  import Approval from './views/Approval.svelte'
  import Settings from './views/Settings.svelte'
  import Channels from './views/Channels.svelte'
  import Inbox from './views/Inbox.svelte'
  import Reminders from './views/Reminders.svelte'
  import { toast } from './components/toast.svelte'
  import { notificationBody } from '../shared/notification-text'
  import { isNoticePlan, type NoticeStatus } from '../shared/notifications'
  import Plan from './views/Plan.svelte'
  import Agents from './views/Agents.svelte'
  import Wallet from './views/Wallet.svelte'
  import Toaster from './components/Toaster.svelte'
  import CommandPalette from './shell/CommandPalette.svelte'
  import Header from './shell/Header.svelte'
  import LoginDialog from './shell/LoginDialog.svelte'
  import Sidebar from './shell/Sidebar.svelte'
  import WindowNav from './shell/WindowNav.svelte'
  import RecoveryBanner from './shell/RecoveryBanner.svelte'
  import { shortcuts, viewOf, views } from './shell/nav'
  import { lock, offerUnlock, settleOffer } from './shell/session'
  import { startLogin } from './shell/login.svelte'
  import { call, onChange, single } from './rpc'
  import { clearDisplays } from './display.svelte'
  import { locale, t } from './i18n/index.svelte'
  import { namesSession, namesChanged } from './views/channels/names.svelte'

  const opened = location.hash.slice(1) || views.channels.path
  let route = $state(opened)
  let status = $state<any>(null)
  let reminders = $state(0)
  let connected = $state(true)
  let verifiedConnection = $state(false)
  let viewSession = $state<string | null>(null)
  let connectionError = $state<string | undefined>()
  let palette = $state(false)
  let collapsed = $state(stored('dmsg.sidebar') === 'collapsed')
  /** Legacy pairing offers started and not imported yet, and those whose notice was dismissed. */
  let offers = $state<string[]>([])
  let dismissed = $state<string[]>((stored('dmsg.legacy-notice') ?? '').split(',').filter(Boolean))

  /** The approval window shows one request; the status, offers and shortcuts are the main window's. */
  const isMain = window.dmsg.role === 'main'
  /** On macOS the main window draws its own title bar over the sidebar's material. */
  const inset = window.dmsg.platform === 'darwin'
  const mod = inset ? '⌘' : 'Ctrl+'
  /** Back and forward: ⌘[ and ⌘] on macOS, Alt+← and Alt+→ elsewhere. */
  const travelKeys = inset ? { back: '⌘[', forward: '⌘]' } : { back: 'Alt+←', forward: 'Alt+→' }
  const travelCodes: Record<string, -1 | 1> = inset ? { BracketLeft: -1, BracketRight: 1 } : { ArrowLeft: -1, ArrowRight: 1 }
  const view = $derived(viewOf(route))
  const unlocked = $derived(status?.state === 'unlocked')
  const legacyNotice = $derived(offers.some((nonce) => !dismissed.includes(nonce)))

  function stored(key: string): string | null {
    try {
      return localStorage.getItem(key)
    } catch {
      return null
    }
  }

  function store(key: string, value: string) {
    try {
      localStorage.setItem(key, value)
    } catch {
      /* the choice lasts until the window closes */
    }
  }

  /** The first status: a window opened in front of a locked device offers its fast unlock. */
  let first = true
  let stateRevision = 0

  const refresh = single(async () => {
    const revision = stateRevision
    const connection = await window.dmsg.connection()
    if (revision !== stateRevision) return
    ;({ connected, error: connectionError } = connection)
    if (!connected) { verifiedConnection = false; return }
    try {
      const next: any = await call('daemon/status')
      if (revision !== stateRevision) return
      // Without an opaque token, a reconnect cannot prove it is the same
      // unlocked session (for example while the installed daemon is older).
      const key = `${next.session || `unknown:${revision}`}:${next.account ?? ''}:${next.state}`
      if (viewSession !== key) clearDisplays()
      namesSession(next)
      viewSession = key
      status = next
      verifiedConnection = true
    } catch {
      if (revision !== stateRevision) return
      verifiedConnection = false
    }
    settleOffer(status)
    if (isMain && first && status && document.hasFocus() && !route.startsWith('/reminders')) offerUnlock(status)
    first = !status
  })

  const loadReminders = single(async () => {
    const revision = stateRevision
    try {
      const next = await call<NoticeStatus>('notifications/status')
      if (revision === stateRevision && next.counts) reminders = next.counts.actionable + next.counts.unread
    } catch { /* Keep the last verified count while disconnected/locked. */ }
  })

  // The offers change on unlock and with legacy state, never on a status refresh.
  async function loadOffers() {
    const revision = stateRevision
    const pairs = unlocked ? await call<any[]>('legacy/pairs').catch(() => []) : []
    if (revision === stateRevision) offers = pairs.map((p) => p.nonce)
  }

  // Portalled dialogs live outside the app's inert wrapper. Mask those too,
  // while retaining same-session forms only for a connection interruption.
  $effect(() => {
    const blocked = !connected || !verifiedConnection
    document.body.inert = blocked
    document.body.toggleAttribute('data-daemon-unverified', blocked)
    return () => { document.body.inert = false; document.body.removeAttribute('data-daemon-unverified') }
  })

  $effect(() => {
    void unlocked
    if (isMain) void loadOffers()
  })

  $effect(() => store('dmsg.sidebar', collapsed ? 'collapsed' : 'expanded'))

  function dismissLegacy() {
    dismissed = [...offers]
    store('dmsg.legacy-notice', dismissed.join(','))
  }

  const go = (next: string) => (location.hash = next)

  /** Back and forward walk the views this window showed, as in anda-bot. */
  let visits = $state([opened])
  let visit = $state(0)
  let traveling = false

  function visited(next: string) {
    route = next
    if (traveling) traveling = false
    else if (next !== visits[visit]) {
      visits = [...visits.slice(0, visit + 1), next].slice(-50)
      visit = visits.length - 1
    }
  }

  /** Neighbouring visits differ, so the hash changes and `visited` hears it. */
  function travel(step: -1 | 1) {
    const target = visits[visit + step]
    if (target === undefined) return
    traveling = true
    visit += step
    go(target)
  }

  /** ⌘K opens the palette, ⌘1–⌘8 and ⌘, go to a view, ⌘L locks (Ctrl elsewhere); ⌘[ and ⌘] go back and forward (Alt+arrows elsewhere). */
  function onKey(event: KeyboardEvent) {
    if (!isMain) return
    const step = travelCodes[event.code]
    const travelHeld = inset ? event.metaKey && !event.ctrlKey && !event.altKey : event.altKey && !event.ctrlKey && !event.metaKey
    if (step && travelHeld && !event.shiftKey) {
      palette = false
      travel(step)
      event.preventDefault()
      return
    }
    const held = inset ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
    if (!held || event.altKey || event.shiftKey) return
    const key = event.key.toLowerCase()
    if (key === 'k') palette = !palette
    else if (key === 'l' && unlocked) void lock()
    else {
      const target = shortcuts.find((v) => v.key === key)
      if (!target) return
      palette = false
      go(target.path)
    }
    event.preventDefault()
  }

  onMount(() => {
    // The route follows the hash only, so a view the hash still names opens again on a click.
    // `/login` is not a view: the approval window asks the main window to log in.
    const offNavigate = window.dmsg.onNavigate((next) => (next === '/login' ? void startLogin() : go(next)))
    const onHash = () => visited(location.hash.slice(1) || views.channels.path)
    window.addEventListener('hashchange', onHash)
    const offs = [offNavigate, () => window.removeEventListener('hashchange', onHash)]
    void refresh()
    offs.push(window.dmsg.onEvent((event) => {
      if (event.type === 'session/changed' || event.type === 'connection/changed') {
        stateRevision++
        verifiedConnection = false
        document.body.inert = true
        document.body.setAttribute('data-daemon-unverified', '')
        if (event.type === 'session/changed') {
          // Destroy plaintext views immediately, even if the subsequent status
          // query is blocked or the daemon disconnects before it can answer.
          status = null
          viewSession = null
          offers = []
          palette = false
          namesSession(null)
          clearDisplays()
        }
        void refresh()
      } else if (event.type === 'resync') void refresh()
    }))
    if (isMain) {
      void loadReminders()
      offs.push(
        onChange(['notifications/changed', 'session/changed', 'resync'], loadReminders),
        onChange(['approval/changed', 'sync/changed', 'sync/progress', 'channel/changed', 'channel/read'], refresh),
        onChange(['legacy/changed'], loadOffers),
        onChange(['names/changed'], namesChanged),
        window.dmsg.onEvent((event) => { if (event.type === 'cache/changed' && (event.data as any)?.kind === 'all') { namesSession(null); namesSession(status) } })
      )
    }
    offs.push(window.dmsg.onEvent((event) => {
      if (event.type !== 'notifications/present' || !isNoticePlan(event.data) || !document.hasFocus()) return
      const plan = event.data
      toast(() => notificationBody(locale(), plan.category, plan.count), { testid: 'notification-toast', ms: 6000,
        action: { label: t('open'), run: () => plan.covered.length === 1 ? window.dmsg.notificationOpen(plan.account, plan.covered[0].id) : window.dmsg.showMain('/reminders') } })
    }))
    return () => offs.forEach((off) => off())
  })
</script>

<svelte:window onkeydown={onKey} onfocus={() => isMain && !route.startsWith('/reminders') && offerUnlock(status)} />

{#snippet windowNav()}
  <WindowNav
    {collapsed}
    canBack={visit > 0}
    canForward={visit < visits.length - 1}
    keys={travelKeys}
    onToggle={() => (collapsed = !collapsed)}
    onTravel={travel}
  />
{/snippet}

{#if !connected || !verifiedConnection}
  <div class="fixed inset-0 z-[100] grid place-items-center bg-paper" role="status" data-testid="daemon-disconnected">{t('daemonDisconnected')}</div>
{/if}
{#if !isMain}
  <main class="h-screen" inert={!connected || !verifiedConnection} aria-hidden={!connected || !verifiedConnection}>
    <RecoveryBanner />
    {#if route.startsWith('/approval/')}
      {#key `${viewSession}:${route}`}
        <Approval id={route.slice('/approval/'.length)} />
      {/key}
    {/if}
  </main>
  <Toaster />
{:else}
  <Tooltip.Provider delayDuration={350}>
    <div class="flex h-screen min-h-0" inert={!connected || !verifiedConnection} aria-hidden={!connected || !verifiedConnection}>
      {#key viewSession}
      <Sidebar
        {route}
        {status}
        {connected}
        {connectionError}
        {collapsed}
        {inset}
        {mod}
        {legacyNotice}
        onDismissLegacy={dismissLegacy}
        {go}
        {windowNav}
      />
      <div class="flex min-w-0 flex-1 flex-col bg-paper">
        <Header {view} {mod} {collapsed} {windowNav} {go} {reminders} onPalette={() => (palette = true)} />
        <RecoveryBanner />
        <main class="min-h-0 flex-1 overflow-hidden">
          {#if view === views.files}
            <Files {status} {mod} {route} {go} />
          {:else if view === views.channels}
            <Channels {status} {route} {go} />
          {:else if view === views.inbox}
            <Inbox {status} {mod} {route} />
          {:else if view === views.reminders}
            <Reminders {status} {route} {go} />
          {:else if view === views.plan}
            <Plan {status} {route} {go} />
          {:else if view === views.wallet}
            <Wallet {status} {route} {go} />
          {:else if view === views.agents}
            <Agents {status} {route} />
          {:else if view === views.approvals}
            <Approvals {status} />
          {:else if view === views.settings || view === views.legacy}
            <Settings {status} {refresh} {route} {go} />
          {:else}
            <Vault {status} {mod} {go} />
          {/if}
        </main>
      </div>
      <CommandPalette bind:open={palette} {status} {mod} {collapsed} {go} onToggleSidebar={() => (collapsed = !collapsed)} />
      <LoginDialog />
      <Toaster />
      {/key}
    </div>
  </Tooltip.Provider>
{/if}

<style>
  :global(body[data-daemon-unverified] > :not(#app)) { visibility: hidden !important; }
</style>
