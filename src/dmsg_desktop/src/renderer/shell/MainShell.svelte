<script lang="ts">
  import { onMount, untrack, type Component } from 'svelte'
  import LinkConfirm from '../components/LinkConfirm.svelte'
  import Toaster from '../components/Toaster.svelte'
  import { t } from '../i18n/index.svelte'
  import { MessageDeliveries, type Delivery } from '../message-delivery'
  import { call, onChange, single } from '../rpc'
  import { lastRoute, noteRoute } from '../view-memory'
  import { namesChanged } from '../views/channels/names.svelte'
  import type { NoticeStatus } from '../../shared/notifications'
  import CommandPalette from './CommandPalette.svelte'
  import Header from './Header.svelte'
  import LoginDialog from './LoginDialog.svelte'
  import { setupStates } from './login.svelte'
  import { entryOf, shortcuts, viewOf, views } from './nav'
  import RecoveryBanner from './RecoveryBanner.svelte'
  import { lock, offerUnlock, settleOffer } from './session'
  import Shortcuts from './Shortcuts.svelte'
  import Sidebar from './Sidebar.svelte'
  import Welcome from './Welcome.svelte'
  import WindowNav from './WindowNav.svelte'

  /**
   * Everything only the main window has: the sidebar, the title bar, the views
   * and the shortcuts. The views load when first asked for and the rest soon
   * after the window is up, so the approval window carries none of this.
   */
  let {
    status,
    lastStatus,
    route,
    viewSession,
    verified,
    blocked,
    accounts,
    refresh,
    go
  }: {
    status: any
    lastStatus: any
    route: string
    viewSession: string | null
    verified: boolean
    blocked: boolean
    accounts: number | null
    refresh: () => Promise<void>
    go: (path: string) => void
  } = $props()

  const pages: Record<string, () => Promise<{ default: Component<any> }>> = {
    [views.files.path]: () => import('../views/Files.svelte'),
    [views.channels.path]: () => import('../views/Channels.svelte'),
    [views.inbox.path]: () => import('../views/Inbox.svelte'),
    [views.reminders.path]: () => import('../views/Reminders.svelte'),
    [views.plan.path]: () => import('../views/Plan.svelte'),
    [views.wallet.path]: () => import('../views/Wallet.svelte'),
    [views.agents.path]: () => import('../views/Agents.svelte'),
    [views.approvals.path]: () => import('../views/Approvals.svelte'),
    [views.settings.path]: () => import('../views/Settings.svelte'),
    [views.vault.path]: () => import('../views/Vault.svelte')
  }
  let loaded = $state.raw<Record<string, Component<any>>>({})

  function load(path: string) {
    if (!loaded[path]) void pages[path]().then((page) => (loaded = { ...loaded, [path]: page.default }))
  }

  const SIDEBAR_KEY = 'dmsg.sidebar'
  /** Below this the sidebar folds on its own; the person's own choice is kept for a wider window. */
  const narrowWindow = matchMedia('(max-width: 899px)')

  /** On macOS the main window draws its own title bar over the sidebar's material. */
  const inset = window.dmsg.platform === 'darwin'
  const mod = inset ? '⌘' : 'Ctrl+'
  /** Back and forward: ⌘[ and ⌘] on macOS, Alt+← and Alt+→ elsewhere. */
  const travelKeys = inset ? { back: '⌘[', forward: '⌘]' } : { back: 'Alt+←', forward: 'Alt+→' }
  const travelCodes: Record<string, -1 | 1> = inset ? { BracketLeft: -1, BracketRight: 1 } : { ArrowLeft: -1, ArrowRight: 1 }

  let reminders = $state(0)
  let deliveries = $state<Delivery[]>([])
  const messageDeliveries = new MessageDeliveries((rows) => { deliveries = rows }, () => verified && status?.state === 'unlocked')
  let palette = $state(false)
  let helping = $state(false)
  let foldedByChoice = $state(stored(SIDEBAR_KEY) === 'collapsed')
  let compact = $state(narrowWindow.matches)
  /** A narrow window's own unfolding, until the width changes again. */
  let unfolded = $state<boolean | null>(null)
  const collapsed = $derived(unfolded === null ? foldedByChoice || compact : !unfolded)
  /** Legacy pairing offers started and not imported yet, and those whose notice was dismissed. */
  let offers = $state<string[]>([])
  let dismissed = $state<string[]>((stored('dmsg.legacy-notice') ?? '').split(',').filter(Boolean))
  /** Setup is finished; the choices at the end of the welcome are still on screen. */
  let finishing = $state(false)
  let welcomed = false

  const view = $derived(viewOf(route))
  const page = $derived(view === views.legacy ? views.settings.path : view.path)
  const unlocked = $derived(status?.state === 'unlocked')
  const legacyNotice = $derived(offers.some((nonce) => !dismissed.includes(nonce)))
  const setup = $derived(!!status && setupStates.includes(status.state))
  const fresh = $derived(status?.state === 'logged_out' && accounts === 0)
  const welcoming = $derived(setup || fresh || finishing)
  const Page = $derived(loaded[page])

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

  $effect(() => load(page))

  // A device that was being set up ends the welcome once it opens.
  $effect(() => {
    if (setup || fresh) welcomed = true
    else if (welcomed && unlocked) {
      welcomed = false
      finishing = true
    }
  })

  // The first status: a window opened in front of a locked device offers its fast unlock.
  let first = true
  $effect(() => {
    if (!status) return
    settleOffer(status)
    untrack(() => {
      if (first && document.hasFocus() && !route.startsWith('/reminders')) offerUnlock(status)
    })
    first = false
  })

  // A session change ended every view: what they were sending goes with them.
  $effect.pre(() => {
    void viewSession
    untrack(() => {
      messageDeliveries.reset()
      palette = false
    })
  })

  const loadReminders = single(async () => {
    const session = viewSession
    try {
      const next = await call<NoticeStatus>('notifications/status')
      if (session === viewSession && next.counts) reminders = next.counts.actionable + next.counts.unread
    } catch { /* Keep the last verified count while disconnected/locked. */ }
  })

  // The offers change on unlock and with legacy state, never on a status refresh.
  async function loadOffers() {
    const session = viewSession
    const pairs = unlocked ? await call<any[]>('legacy/pairs').catch(() => []) : []
    if (session === viewSession) offers = pairs.map((p) => p.nonce)
  }

  $effect(() => {
    void unlocked
    void loadOffers()
  })

  function dismissLegacy() {
    dismissed = [...offers]
    store('dmsg.legacy-notice', dismissed.join(','))
  }

  function toggleSidebar() {
    if (compact) unfolded = collapsed
    else {
      foldedByChoice = !foldedByChoice
      store(SIDEBAR_KEY, foldedByChoice ? 'collapsed' : 'expanded')
    }
  }

  /** Back and forward walk the views this window showed, as in anda-bot. */
  let visits = $state(untrack(() => [route]))
  let visit = $state(0)
  let traveling = false

  $effect(() => {
    const next = route
    noteRoute(entryOf(next).path, next)
    untrack(() => {
      if (traveling) traveling = false
      else if (next !== visits[visit]) {
        visits = [...visits.slice(0, visit + 1), next].slice(-50)
        visit = visits.length - 1
      }
    })
  })

  /** Neighbouring visits differ, so the hash changes and the route follows. */
  function travel(step: -1 | 1) {
    const target = visits[visit + step]
    if (target === undefined) return
    traveling = true
    visit += step
    go(target)
  }

  /** A view's entry returns to where it was left; pressed again, it goes to the view's start. */
  const enter = (path: string) => go(entryOf(route).path === path ? path : lastRoute(path))

  /** ⌘K opens the palette, ⌘1–⌘8 and ⌘, go to a view, ⌘L locks, ⌘/ lists the shortcuts (Ctrl elsewhere); ⌘[ and ⌘] go back and forward (Alt+arrows elsewhere). */
  function onKey(event: KeyboardEvent) {
    if (welcoming) return
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
    else if (key === '/') helping = !helping
    else if (key === 'l' && unlocked) void lock()
    else {
      const target = shortcuts.find((v) => v.key === key)
      if (!target) return
      palette = false
      enter(target.path)
    }
    event.preventDefault()
  }

  onMount(() => {
    void loadReminders()
    const online = () => messageDeliveries.resume()
    const onNarrow = () => {
      compact = narrowWindow.matches
      unfolded = null
    }
    window.addEventListener('online', online)
    narrowWindow.addEventListener('change', onNarrow)
    // A long sync reports often; the rail needs it about once a second.
    let progressing: ReturnType<typeof setTimeout> | undefined
    const onProgress = () => {
      progressing ??= setTimeout(() => {
        progressing = undefined
        void refresh()
      }, 1000)
    }
    const offs = [
      () => window.removeEventListener('online', online),
      () => narrowWindow.removeEventListener('change', onNarrow),
      onChange(['notifications/changed', 'session/changed', 'resync'], loadReminders),
      onChange(['approval/changed', 'sync/changed', 'sync/finished', 'channel/changed', 'channel/read'], refresh),
      onChange(['sync/progress'], onProgress),
      onChange(['sync/finished'], () => messageDeliveries.resume()),
      onChange(['legacy/changed'], loadOffers),
      onChange(['names/changed'], namesChanged)
    ]
    // The other views, ready before they are asked for.
    const idle = requestIdleCallback(() => Object.keys(pages).forEach(load), { timeout: 3000 })
    return () => {
      cancelIdleCallback(idle)
      clearTimeout(progressing)
      offs.forEach((off) => off())
    }
  })
</script>

<svelte:window onkeydown={onKey} onfocus={() => !route.startsWith('/reminders') && offerUnlock(status)} />

{#snippet windowNav()}
  <WindowNav
    {collapsed}
    canBack={visit > 0}
    canForward={visit < visits.length - 1}
    keys={travelKeys}
    onToggle={toggleSidebar}
    onTravel={travel}
  />
{/snippet}

<div class="flex h-screen min-h-0" inert={blocked} aria-hidden={blocked}>
  {#if welcoming}
    <Welcome {status} {refresh} {finishing} onDone={() => (finishing = false)} />
  {:else}
    <Sidebar
      {route}
      status={status ?? lastStatus}
      {collapsed}
      {inset}
      {mod}
      {legacyNotice}
      onDismissLegacy={dismissLegacy}
      {go}
      {enter}
      {windowNav}
    />
    <div class="flex min-w-0 flex-1 flex-col bg-paper">
      <Header {view} {mod} {collapsed} {windowNav} {go} {reminders} {status} onPalette={() => (palette = true)} />
      {#key viewSession}<RecoveryBanner />{/key}
      <main class="min-h-0 flex-1 overflow-hidden" aria-busy={!Page}>
        {#key viewSession}
          <!-- A session change ended every view; the next status opens one. -->
          {#if viewSession && Page}
            <Page {status} {mod} {route} {go} {refresh} {deliveries} {messageDeliveries} />
          {/if}
        {/key}
      </main>
    </div>
    {#key viewSession}
      <CommandPalette bind:open={palette} {status} {mod} {collapsed} {go} onToggleSidebar={toggleSidebar} onShortcuts={() => (helping = true)} />
    {/key}
    <Shortcuts bind:open={helping} {mod} keys={travelKeys} />
  {/if}
  {#key viewSession}
    <LoginDialog />
    <LinkConfirm />
    <Toaster />
  {/key}
</div>
