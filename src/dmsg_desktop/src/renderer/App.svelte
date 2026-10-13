<script lang="ts">
  import { onMount } from 'svelte'
  import Vault from './views/Vault.svelte'
  import Approvals from './views/Approvals.svelte'
  import Approval from './views/Approval.svelte'
  import Settings from './views/Settings.svelte'
  import Channels from './views/Channels.svelte'
  import Inbox from './views/Inbox.svelte'
  import Account from './views/Account.svelte'
  import Agents from './views/Agents.svelte'
  import Legacy from './views/Legacy.svelte'
  import { t, sessionLabel } from './i18n/index.svelte'
  import LocaleSwitcher from './i18n/LocaleSwitcher.svelte'
  import { call } from './rpc'

  let route = $state(location.hash.slice(1) || '/vault')
  let status = $state<any>(null)
  let connected = $state(true)

  async function refresh() {
    connected = (await window.dmsg.connection()).connected
    try {
      status = await call('daemon/status')
    } catch {
      status = null
    }
  }

  onMount(() => {
    void refresh()
    const offEvent = window.dmsg.onEvent((event) => {
      if (['session/changed', 'connection/changed', 'approval/changed', 'sync/changed'].includes(event.type)) void refresh()
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

  const go = (next: string) => (location.hash = next)
</script>

{#if window.dmsg.role === 'approval'}
  <main class="approval">
    {#if route.startsWith('/approval/')}
      {#key route}
        <Approval id={route.slice('/approval/'.length)} />
      {/key}
    {/if}
  </main>
{:else}
  <div class="shell">
    <nav>
      <div class="brand">dMsg</div>
      <button class:active={route.startsWith('/vault')} onclick={() => go('/vault')}>{t('vault')}</button>
      <button class:active={route.startsWith('/channels')} onclick={() => go('/channels')}>{t('channels')}</button>
      <button class:active={route.startsWith('/inbox')} onclick={() => go('/inbox')}>{t('inbox')}</button>
      <button class:active={route.startsWith('/account')} onclick={() => go('/account')}>{t('planAndNames')}</button>
      <button class:active={route.startsWith('/agents')} onclick={() => go('/agents')}>{t('agents')}</button>
      <button class:active={route.startsWith('/legacy')} onclick={() => go('/legacy')}>{t('legacy')}</button>
      <button class:active={route.startsWith('/approvals')} onclick={() => go('/approvals')}>
        {t('approvals')}
        {#if status?.approvals_waiting}<span class="badge" data-testid="waiting-count">{status.approvals_waiting}</span>{/if}
      </button>
      <button class:active={route.startsWith('/settings')} onclick={() => go('/settings')}>{t('settings')}</button>
      <div class="foot muted">
        <LocaleSwitcher />
        <div class="state">{#if !connected}<span class="error">{t('disconnected')}</span>{:else}{status?.state ? sessionLabel(status.state) : '-'}{/if}</div>
      </div>
    </nav>
    <main>
      {#if route.startsWith('/channels')}
        <Channels {status} />
      {:else if route.startsWith('/inbox')}
        <Inbox {status} />
      {:else if route.startsWith('/account')}
        <Account {status} />
      {:else if route.startsWith('/agents')}
        <Agents {status} />
      {:else if route.startsWith('/legacy')}
        <Legacy {status} />
      {:else if route.startsWith('/approvals')}
        <Approvals {status} />
      {:else if route.startsWith('/settings')}
        <Settings {status} {refresh} browsers={route === '/settings/browsers'} />
      {:else}
        <Vault {status} />
      {/if}
    </main>
  </div>
{/if}

<style>
  .shell {
    display: grid;
    grid-template-columns: 200px 1fr;
    min-height: 100vh;
  }
  nav {
    border-inline-end: 1px solid var(--line);
    padding: 16px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  nav button {
    text-align: start;
    border: 0;
    background: transparent;
    display: flex;
    justify-content: space-between;
  }
  nav button.active {
    background: var(--pale);
    color: var(--green);
  }
  .brand {
    font-weight: 700;
    font-size: 18px;
    padding: 0 12px 12px;
  }
  .foot {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .foot :global(.locale-trigger) {
    padding: 4px 12px;
    font-size: 13px;
  }
  .state {
    padding: 0 12px;
    font-size: 12px;
  }
  main {
    padding: 20px 24px;
    overflow: auto;
  }
  main.approval {
    padding: 16px;
  }
</style>
