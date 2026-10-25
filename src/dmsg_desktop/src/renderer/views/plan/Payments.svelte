<script lang="ts">
  import { onMount } from 'svelte'
  import { display, watchDisplays, type Mode } from '../../display.svelte'
  import Collapsible from '../../components/Collapsible.svelte'
  import Confirm from '../../components/Confirm.svelte'
  import Icon from '../../components/Icon.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Page from '../../components/page/Page.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { t, type Label } from '../../i18n/index.svelte'
  import { ago, bridge, call, errorText, onChange, single, size, units, when } from '../../rpc'
  import Operations from '../Operations.svelte'
  import { deadlineOf, moving, orderActions, orderState, refundOutcome, skuOf, type OrderAction } from './plan'

  /**
   * Payments: the orders this device opened, newest first, each with what
   * it buys, its state for people, its amount, service period and
   * deadlines, and only the actions its state allows; and the wallet that
   * pays, with its balances. Orders on their way are read again until they
   * settle, and one that applies has the relay read the plan anew.
   */
  let checkouts = $state<any[] | null>(null)
  let orders = $state<Record<string, any>>({})
  let catalog = $state<any>(null)
  /** The first purchase that may still be refunded. */
  let refundable = $state<string | null>(null)
  let payer = $state<any>(null)
  let errors = $state<Record<string, string>>({})
  let busy = $state(false)
  let now = $state(Date.now())
  /** The order to cancel, with the refund's quote for the first purchase. */
  let cancelling = $state<{ job: any; quote?: any } | null>(null)
  /** Each order's last status seen: one turning applied has the relay read the plan anew. */
  const seen = new Map<string, string>()
  let alive = true
  let allOrders = false
  let refreshPlan = false

  const actionLabels: Record<OrderAction, Label> = {
    pay: 'pay',
    discard: 'discard',
    cancel: 'cancelOrder',
    refund: 'requestRefund',
    advance: 'continueOrder',
    reconcile: 'reconcile'
  }

  /** One part of the page: its failure shows in its place and leaves the others. */
  async function part(key: string, read: () => Promise<void>) {
    try {
      await read()
      errors[key] = ''
    } catch (e) {
      if (alive) errors[key] = errorText(e)
    }
  }

  const readStatus = single(() => part('status', async () => {
    if (!alive) return
    const refresh = refreshPlan
    refreshPlan = false
    const status: any = await call('plan/status', { refresh })
    if (alive) refundable = status?.refund?.order ?? null
  }))

  const readRemoteOrders = single(async () => {
    if (!alive) return
    const all = allOrders
    allOrders = false
    const jobs = (checkouts ?? []).filter((job) => job.stage !== 'review' && (all || !orders[job.id] || moving(job, orders[job.id], Date.now())))
    let cursor = 0
    // Bound the remote fan-out and publish each reply as it arrives.
    await Promise.all(Array.from({ length: Math.min(4, jobs.length) }, async () => {
      while (alive && cursor < jobs.length) {
        const job = jobs[cursor++]
        try {
          const next = await call('plan/order', { id: job.id })
          if (!alive || !checkouts?.some((j) => j.id === job.id)) continue
          const before = seen.get(job.id)
          orders[job.id] = next
          seen.set(job.id, next.status)
          if (before && before !== next.status && ['Applied', 'Active'].includes(next.status)) {
            refreshPlan = true
            void readStatus()
          }
        } catch {
          // Keep the last known result; local journals remain usable.
        }
      }
    }))
  })

  const refreshOrders = single(() => part('orders', async () => {
    if (!alive) return
    const jobs: any[] = await call('plan/checkouts')
    if (!alive) return
    jobs.sort((a, b) => (b.created_at_ms ?? 0) - (a.created_at_ms ?? 0))
    checkouts = jobs
    orders = Object.fromEntries(Object.entries(orders).filter(([id]) => jobs.some((j) => j.id === id)))
    now = Date.now()
    void readRemoteOrders()
  }))

  const readPayer = single(() => part('payer', async () => {
    if (!alive) return
    const next = await call('payer/status')
    if (alive) payer = next
  }))
  const readCatalog = single((mode: Mode = 'swr') => part('catalog', async () => {
    if (!alive) return
    const next = await display('plan/catalog', {}, mode)
    if (alive) catalog = next
  }))

  function reload() {
    allOrders = true
    void readCatalog()
    void readStatus()
    void readPayer()
    return refreshOrders()
  }

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    try {
      await work()
    } catch (e) {
      toast(() => errorText(e))
    } finally {
      busy = false
    }
  }

  /** Paying and continuing are decided in the approval window. */
  const approve = (request: any) => alive ? bridge(() => window.dmsg.openApproval(request.id)) : Promise.resolve()

  function act(action: OrderAction, job: any) {
    switch (action) {
      case 'pay':
        return attempt(async () => approve(await call('plan/checkout', { id: job.id })))
      case 'advance':
        return attempt(async () => approve(await call('plan/advance', { id: job.id })))
      case 'discard':
        return attempt(async () => {
          await call('plan/discard', { id: job.id })
          await refreshOrders()
        })
      case 'reconcile':
        return attempt(async () => {
          orders[job.id] = await call('plan/reconcile', { id: job.id })
        })
      case 'cancel':
        cancelling = { job }
        return
      case 'refund':
        // The refund is quoted first: what comes back, the network fee, and when the rights stop.
        return attempt(async () => {
          cancelling = { job, quote: await call('plan/refund_quote', { id: job.id }) }
        })
    }
  }

  const confirmCancel = () =>
    attempt(async () => {
      const refund = cancelling?.quote != null
      const done: any = await call('plan/cancel', { id: cancelling!.job.id })
      cancelling = null
      const outcome = refundOutcome(done?.refunds ?? [])
      toast(() => outcome.error ? `${t('error')}: ${outcome.error}` : t(refund || done?.refunds?.length ? outcome.label : 'orderCancelledNote'))
      await reload()
    })

  /** The payer: a PEM file the main process picks, or the login again. */
  const usePayer = (login: boolean) =>
    attempt(async () => {
      const done = await bridge(() => window.dmsg.payerPem(login))
      if (done) await readPayer()
    })

  onMount(() => {
    const stop = watchDisplays(['catalog'], (mode) => readCatalog(mode))
    void reload()
    const off = onChange(['approval/changed', 'session/changed'], reload)
    const timer = setInterval(() => {
      now = Date.now()
      if ((checkouts ?? []).some((job) => moving(job, orders[job.id], now))) void refreshOrders()
    }, 15_000)
    return () => {
      alive = false
      off()
      stop()
      clearInterval(timer)
    }
  })

  /** What an order buys, for people. */
  function title(job: any) {
    const sku = skuOf(job.sku, catalog)
    if (sku.kind === 'upgrade') return t('upgradeTo', { plan: sku.plan ?? '' })
    if (sku.kind === 'storage') return t('storagePackOf', { size: sku.bytes != null ? size(sku.bytes) : '-' })
    return sku.plan ?? job.sku
  }

  /** A plan bought to start later: a renewal, or a switch at the end of the term. */
  const renewal = (job: any) => skuOf(job.sku, catalog).kind === 'plan' && job.starts_at_ms > (job.created_at_ms ?? 0) + 60_000

  function amount(job: any) {
    const terms = job.terms ?? {}
    if (terms.method === 'Cash') return `${units(String(terms.amount), terms.decimals)} ${terms.asset}`
    if (terms.required_stake_e8s != null) return `${t('stake')} ${units(String(terms.required_stake_e8s), 8)} PANDA`
    return ''
  }

  /** Whether the order still has to reach the service: funded, or applied with the neuron. */
  const awaiting = (job: any, order: any) =>
    order ? ['Reserving', 'AwaitingFunding', 'Checking'].includes(order.status) : job.stage === 'review' || job.stage === 'authorized'
</script>

<Page>
  <Section title={t('orders')} description={t('ordersHint')} icon="audit" flush testid="plan-orders">
    {#snippet actions()}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => attempt(reload)} data-testid="orders-refresh"><Icon name="sync-now" />{t('refresh')}</button>
    {/snippet}
    {#if errors.orders}
      <div class="px-5 pb-4"><Notice failure={errors.orders} tone="danger">{errors.orders}</Notice></div>
    {:else if checkouts && !checkouts.length}
      <p class="px-5 pb-4 text-[12.5px] text-faint" data-testid="no-orders">{t('noOrders')}</p>
    {:else if checkouts}
      <ul class="divide-y divide-line" data-testid="checkouts">
        {#each checkouts as job (job.id)}
          {@const order = orders[job.id]}
          {@const state = orderState(job, order)}
          {@const deadline = deadlineOf(job)}
          <li class="grid gap-1 px-5 py-3" data-testid="order">
            <div class="flex flex-wrap items-center gap-2">
              <b class="text-[13px] font-medium" data-testid="order-title">{title(job)}</b>
              {#if renewal(job)}<Pill>{t('renewal')}</Pill>{/if}
              <Pill tone={state.tone} testid="order-state">{t(state.label)}</Pill>
              <span class="flex-1"></span>
              {#each orderActions(job, order, now, refundable) as action (action)}
                <button
                  class={buttonClass(action === 'pay' || action === 'advance' ? 'primary' : action === 'cancel' || action === 'refund' ? 'danger' : 'secondary', 'xs')}
                  disabled={busy}
                  onclick={() => act(action, job)}
                  data-testid={`order-${action}`}>{t(actionLabels[action])}</button
                >
              {/each}
            </div>
            <div class="flex flex-wrap gap-x-3 gap-y-0.5 text-[12px] text-muted tabular-nums">
              <span data-testid="order-amount">{amount(job)}</span>
              <span data-testid="order-period">{t('servicePeriod', { from: when(order?.starts_at_ms ?? job.starts_at_ms), to: when(order?.expires_at_ms ?? job.expires_at_ms) })}</span>
              {#if deadline && awaiting(job, order)}
                <span class="text-warn" data-testid="order-deadline">{t(job.terms?.method === 'Cash' ? 'payBy' : 'applyBy', { time: when(deadline) })}</span>
              {/if}
              <span class="text-faint">{ago(job.created_at_ms, now)}</span>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </Section>

  <Section title={t('wallet')} description={t('walletHint')} icon="wallet" testid="plan-wallet">
    {#snippet actions()}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => usePayer(false)} data-testid="payer-pem"><Icon name="key" />{t('usePemFile')}</button>
      {#if payer?.source === 'pem'}
        <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => usePayer(true)} data-testid="payer-login">{t('useLoginPayer')}</button>
      {/if}
    {/snippet}
    {#if errors.payer}
      <Notice failure={errors.payer} tone="danger">{errors.payer}</Notice>
    {:else if payer}
      <dl>
        <Fact label={t('payer')}>
          <span class="flex flex-wrap items-center gap-1.5">
            <Id id={payer.principal} full testid="payer-principal" />
            <Pill testid="payer-source">{payer.source === 'pem' ? t('payerPem') : t('payerLogin')}</Pill>
          </span>
        </Fact>
        {#each payer.balances ?? [] as row (row.ledger)}
          <Fact label={row.asset} testid="wallet-balance">
            {#if row.error}
              <span class="text-danger">{t('error')}: {row.error}</span>
            {:else}
              <span class="font-semibold tabular-nums">{units(String(row.balance), row.decimals)} {row.asset}</span>
            {/if}
          </Fact>
        {/each}
      </dl>
      <p class="mt-3 text-[12px] text-faint">{t('topUpHint')}</p>
    {/if}
  </Section>

  <Collapsible testid="plan-advanced" class="px-1">
    {#snippet title()}<span class="text-[12.5px] font-medium text-muted">{t('advanced')}</span>{/snippet}
    <div class="pt-2"><Operations /></div>
  </Collapsible>
</Page>

<Confirm
  bind:open={() => cancelling !== null, (open) => (cancelling = open ? cancelling : null)}
  title={cancelling?.quote ? t('requestRefund') : t('cancelOrder')}
  description={cancelling?.quote
    ? t('refundHint', { time: when(cancelling.quote.stops_at_ms) })
    : cancelling?.job.terms?.method === 'Panda'
      ? t('cancelPandaHint')
      : t('cancelCashHint')}
  confirm={cancelling?.quote ? t('requestRefund') : t('cancelOrder')}
  tone="danger"
  {busy}
  testid="order-cancel"
  onConfirm={confirmCancel}
>
  {#snippet body()}
    {#if cancelling?.quote}
      {@const quote = cancelling.quote}
      <dl>
        <Fact label={t('refundAmount')} testid="refund-amount">{units(quote.refund, quote.decimals)} {quote.asset}</Fact>
        <Fact label={t('networkFee')}>{units(quote.fee, quote.decimals)} {quote.asset}</Fact>
        <Fact label={t('refundReceived')}>{units(quote.received, quote.decimals)} {quote.asset}</Fact>
      </dl>
    {/if}
  {/snippet}
</Confirm>
