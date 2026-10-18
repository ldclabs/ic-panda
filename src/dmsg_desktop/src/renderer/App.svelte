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
  import Account from './views/Account.svelte'
  import Agents from './views/Agents.svelte'
  import Wallet from './views/Wallet.svelte'
  import Toaster from './components/Toaster.svelte'
  import CommandPalette from './shell/CommandPalette.svelte'
  import Header from './shell/Header.svelte'
  import LoginDialog from './shell/LoginDialog.svelte'
  import Sidebar from './shell/Sidebar.svelte'
  import WindowNav from './shell/WindowNav.svelte'
  import { shortcuts, viewOf, views } from './shell/nav'
  import { lock, offerUnlock, settleOffer } from './shell/session'
  import { call, onChange, single } from './rpc'

  const opened = location.hash.slice(1) || '/vault'
  let route = $state(opened)
  let status = $state<any>(null)
  let connected = $state(true)
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

  const refresh = single(async () => {
    ;({ connected, error: connectionError } = await window.dmsg.connection())
    try {
      status = await call('daemon/status')
    } catch {
      status = null
    }
    settleOffer(status)
    if (first && status && document.hasFocus()) offerUnlock(status)
    first = !status
  })

  // The offers change on unlock and with legacy state, never on a status refresh.
  async function loadOffers() {
    const pairs = unlocked ? await call<any[]>('legacy/pairs').catch(() => []) : []
    offers = pairs.map((p) => p.nonce)
  }

  $effect(() => {
    void unlocked
    if (isMain) void loadOffers()
  })

  // The idle lock follows a person's input in any window, which the main
  // process reports. While unlocked the status is read again as often as the
  // daemon checks the lock, so the gate rail's countdown follows `lock_at`.
  $effect(() => {
    if (!isMain || !unlocked) return
    const timer = setInterval(() => void refresh(), 10_000)
    return () => clearInterval(timer)
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
    const offNavigate = window.dmsg.onNavigate(go)
    const onHash = () => visited(location.hash.slice(1) || '/vault')
    window.addEventListener('hashchange', onHash)
    const offs = [offNavigate, () => window.removeEventListener('hashchange', onHash)]
    if (isMain) {
      void refresh()
      offs.push(onChange(['session/changed', 'connection/changed', 'approval/changed', 'sync/changed'], refresh), onChange(['legacy/changed'], loadOffers))
    }
    return () => offs.forEach((off) => off())
  })
</script>

<svelte:window onkeydown={onKey} onfocus={() => isMain && offerUnlock(status)} />

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

{#if !isMain}
  <main class="h-screen">
    {#if route.startsWith('/approval/')}
      {#key route}
        <Approval id={route.slice('/approval/'.length)} />
      {/key}
    {/if}
  </main>
{:else}
  <Tooltip.Provider delayDuration={350}>
    <div class="flex h-screen min-h-0">
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
        <Header {view} {mod} {collapsed} {windowNav} {go} onPalette={() => (palette = true)} />
        <main class="min-h-0 flex-1 overflow-hidden">
          {#if view === views.files}
            <Files {status} {mod} {route} {go} />
          {:else if view === views.channels}
            <Channels {status} />
          {:else if view === views.inbox}
            <Inbox {status} {mod} />
          {:else if view === views.plan}
            <Account {status} />
          {:else if view === views.wallet}
            <Wallet {status} />
          {:else if view === views.agents}
            <Agents {status} />
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
    </div>
  </Tooltip.Provider>
{/if}
