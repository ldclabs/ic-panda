<script lang="ts">
  import Locked from '../components/page/Locked.svelte'
  import TabBar from '../components/page/TabBar.svelte'
  import { t } from '../i18n/index.svelte'
  import Membership from './plan/Membership.svelte'
  import Names from './plan/Names.svelte'
  import Payments from './plan/Payments.svelte'
  import { isTab, type Tab } from './plan/plan'

  /**
   * The account's plan in three parts: the membership, its term and what
   * may be bought now; names, looked up, registered, claimed and handed
   * over; and payments, the orders and the wallet that pays. Each part
   * reads its own data, so one that fails leaves the others. A quote only
   * fixes terms: paying, claiming, registering, accepting and moving a name
   * are decided in the approval window.
   */
  let { status, route, go }: { status: any; route: string; go: (path: string) => void } = $props()

  let tab = $state<Tab>('membership')
  /** A link's request for the names part: a transfer token to accept, or a name to look up. */
  let ask = $state<{ accept?: string; name?: string } | null>(null)

  const unlocked = $derived(status?.state === 'unlocked')

  // Links and other views arrive as the route's query: a transfer to accept, a name, a part.
  $effect(() => {
    const at = route.indexOf('?')
    if (at < 0) return
    const params = new URLSearchParams(route.slice(at + 1))
    const accept = params.get('accept') ?? undefined
    const name = params.get('names') ?? undefined
    const part = params.get('tab')
    if (accept || name) {
      tab = 'names'
      ask = { accept, name }
    } else if (isTab(part)) tab = part
    go('/plan')
  })

  const tabs = $derived([
    { value: 'membership', label: t('membership'), icon: 'plan' as const },
    { value: 'names', label: t('names'), icon: 'user' as const },
    { value: 'payments', label: t('payments'), icon: 'coins' as const }
  ])
</script>

<div class="flex h-full min-h-0 flex-col">
  <TabBar bind:value={() => tab, (next) => (tab = next as Tab)} items={tabs} label={t('planAndNames')} testid="plan-tab" />
  <div class="flex min-h-0 flex-1 flex-col">
    {#if !unlocked}
      <Locked {status} testid="plan-locked" />
    {:else}
      {#key status?.account}
        {#if tab === 'membership'}
          <Membership />
        {:else if tab === 'names'}
          <Names {status} {go} bind:ask />
        {:else}
          <Payments />
        {/if}
      {/key}
    {/if}
  </div>
</div>
