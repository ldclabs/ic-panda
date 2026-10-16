<script lang="ts">
  import { Tooltip } from 'bits-ui'
  import { onMount } from 'svelte'
  import Vault from './views/Vault.svelte'
  import Approvals from './views/Approvals.svelte'
  import Approval from './views/Approval.svelte'
  import Settings from './views/Settings.svelte'
  import Channels from './views/Channels.svelte'
  import Inbox from './views/Inbox.svelte'
  import Account from './views/Account.svelte'
  import Agents from './views/Agents.svelte'
  import Toaster from './components/Toaster.svelte'
  import CommandPalette from './shell/CommandPalette.svelte'
  import Header from './shell/Header.svelte'
  import Sidebar from './shell/Sidebar.svelte'
  import { shortcuts, viewOf, views } from './shell/nav'
  import { lock } from './shell/session'
  import { call } from './rpc'

  let route = $state(location.hash.slice(1) || '/vault')
  let status = $state<any>(null)
  let connected = $state(true)
  let connectionError = $state<string | undefined>()
  let palette = $state(false)
  let collapsed = $state(stored('dmsg.sidebar') === 'collapsed')
  /** Legacy pairing offers started and not imported yet, and those whose notice was dismissed. */
  let offers = $state<string[]>([])
  let dismissed = $state<string[]>((stored('dmsg.legacy-notice') ?? '').split(',').filter(Boolean))

  /** On macOS the main window draws its own title bar over the sidebar's material. */
  const inset = window.dmsg.platform === 'darwin'
  const mod = inset ? '⌘' : 'Ctrl+'
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

  async function refresh() {
    ;({ connected, error: connectionError } = await window.dmsg.connection())
    try {
      status = await call('daemon/status')
    } catch {
      status = null
    }
  }

  // Reading the offers counts as activity for the idle lock, so it happens on
  // unlock and when legacy state changes, never on a status refresh.
  async function loadOffers() {
    const pairs = unlocked ? await call<any[]>('legacy/pairs').catch(() => []) : []
    offers = pairs.map((p) => p.nonce)
  }

  $effect(() => {
    void unlocked
    void loadOffers()
  })

  $effect(() => store('dmsg.sidebar', collapsed ? 'collapsed' : 'expanded'))

  function dismissLegacy() {
    dismissed = [...offers]
    store('dmsg.legacy-notice', dismissed.join(','))
  }

  const go = (next: string) => (location.hash = next)

  /** ⌘K opens the palette, ⌘1–⌘6 and ⌘, go to a view, ⌘L locks (Ctrl elsewhere). */
  function onKey(event: KeyboardEvent) {
    if (window.dmsg.role !== 'main') return
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
    void refresh()
    const offEvent = window.dmsg.onEvent((event) => {
      if (['session/changed', 'connection/changed', 'approval/changed', 'sync/changed'].includes(event.type)) void refresh()
      if (event.type === 'legacy/changed') void loadOffers()
    })
    const offNavigate = window.dmsg.onNavigate((next) => (route = next))
    const onHash = () => (route = location.hash.slice(1) || '/vault')
    window.addEventListener('hashchange', onHash)
    return () => {
      offEvent()
      offNavigate()
      window.removeEventListener('hashchange', onHash)
    }
  })
</script>

<svelte:window onkeydown={onKey} />

{#if window.dmsg.role === 'approval'}
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
        bind:collapsed
        {inset}
        {mod}
        {legacyNotice}
        onDismissLegacy={dismissLegacy}
        {go}
      />
      <div class="flex min-w-0 flex-1 flex-col bg-paper">
        <Header {view} {mod} {go} onPalette={() => (palette = true)} />
        <main class="min-h-0 flex-1 overflow-hidden">
          {#if view === views.channels}
            <Channels {status} />
          {:else if view === views.inbox}
            <Inbox {status} {mod} />
          {:else if view === views.plan}
            <Account {status} />
          {:else if view === views.agents}
            <Agents {status} />
          {:else if view === views.approvals}
            <Approvals {status} />
          {:else if view === views.settings || view === views.legacy}
            <Settings {status} {refresh} {route} {go} />
          {:else}
            <Vault {status} {mod} />
          {/if}
        </main>
      </div>
      <CommandPalette bind:open={palette} {status} {mod} {collapsed} {go} onToggleSidebar={() => (collapsed = !collapsed)} />
      <Toaster />
    </div>
  </Tooltip.Provider>
{/if}
