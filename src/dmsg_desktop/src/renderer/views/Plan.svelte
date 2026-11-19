<script lang="ts">
  import Locked from '../components/page/Locked.svelte'
  import TabBar from '../components/page/TabBar.svelte'
  import { t } from '../i18n/index.svelte'
  import { visitRoute } from '../view-memory'
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

  const params = $derived(new URLSearchParams(route.split('?')[1]))
  const tab = $derived.by((): Tab => {
    if (params.get('accept') || params.get('names')) return 'names'
    const part = params.get('tab')
    return isTab(part) ? part : 'membership'
  })
  /** A link's request for the names part: a transfer token to accept, or a name to look up. */
  let ask = $state<{ accept?: string; name?: string } | null>(null)

  const unlocked = $derived(status?.state === 'unlocked')

  function showTab(next: string) {
    go(next === 'membership' ? '/plan' : `/plan?tab=${next}`)
  }

  // Hand off a one-time lookup or invitation. The lasting route only names the tab.
  $effect(() => {
    if (!unlocked) return
    const accept = params.get('accept') ?? undefined
    const name = params.get('names') ?? undefined
    if (accept || name) {
      ask = { accept, name }
      const next = visitRoute(route)
      if (next !== route) go(next)
    }
  })

  const tabs = $derived([
    { value: 'membership', label: t('membership'), icon: 'plan' as const },
    { value: 'names', label: t('names'), icon: 'user' as const },
    { value: 'payments', label: t('payments'), icon: 'coins' as const }
  ])
</script>

<div class="flex h-full min-h-0 flex-col">
  <TabBar bind:value={() => tab, showTab} items={tabs} label={t('planAndNames')} testid="plan-tab" />
  <div class="flex min-h-0 flex-1 flex-col">
    {#if !unlocked}
      <Locked {status} testid="plan-locked" />
    {:else}
      {#key status?.account}
        {#if tab === 'membership'}
          <Membership {status} onReviewPayments={() => showTab('payments')} />
        {:else if tab === 'names'}
          <Names {status} {go} bind:ask />
        {:else}
          <Payments />
        {/if}
      {/key}
    {/if}
  </div>
</div>
