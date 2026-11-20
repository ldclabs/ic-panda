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
  import { ago, bridge, call, code, errorText, onChange, single, size, units, when } from '../../rpc'
  import Operations from '../Operations.svelte'
  import { reviewApproval } from './approval'
  import { deadlineOf, quoteDeadline, quoteExpired, moving, orderActions, orderState, refundOutcome, skuOf, type OrderAction } from './plan'

  /**
   * Payments: the orders this device opened, newest first, each with what
   * it buys, its state for people, its amount, service period and
   * deadlines, and only the actions its state allows; and the wallet that
   * pays, with its balances. Orders on their way are read again until they
   * settle, and one that applies has the relay read the plan anew.
   */
  let { walletFirst = false, fundingOrderId, onMembership }: { walletFirst?: boolean; fundingOrderId?: string | null; onMembership: () => void } = $props()
  let remoteErrors = $state<Record<string, string>>({})
  let actionErrors = $state<Record<string, string>>({})
  let unknown = $state<Record<string, boolean>>({})
  let walletRegion = $state<HTMLElement>()
  $effect(() => { if (walletFirst && walletRegion) walletRegion.focus({ preventScroll: true }) })
  let checkouts = $state<any[] | null>(null)
  const fundingOrder = $derived(walletFirst && fundingOrderId ? checkouts?.find(job => job.id === fundingOrderId) : null)
  let orders = $state<Record<string, any>>({})
  let catalog = $state<any>(null)
  /** The first purchase that may still be refunded. */
  let now = $state(Date.now())
  let refundWindow = $state<{ order: string; until_ms?: number } | null>(null)
  const refundable = $derived(refundWindow && (refundWindow.until_ms == null || refundWindow.until_ms > now) ? refundWindow.order : null)
  let payer = $state<any>(null)
  let payerReading = $state(false)
  let errors = $state<Record<string, string>>({})
  let busy = $state(false)
  /** The order to cancel, with the refund's quote for the first purchase. */
  let cancelling = $state<{ job: any; quote?: any } | null>(null)
  /** Each order's last status seen: one turning applied has the relay read the plan anew. */
  const seen = new Map<string, string>()
  let alive = true
  let allOrders = false
  let refreshPlan = false

  const actionLabels: Record<OrderAction, Label> = {
    pay: 'continueToApprove',
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
    if (alive) refundWindow = status?.refund ?? null
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
          remoteErrors[job.id] = ''
          if (!next) continue
          seen.set(job.id, next.status)
          if (before && before !== next.status && ['Applied', 'Active'].includes(next.status)) {
            refreshPlan = true
            void readStatus()
          }
        } catch (cause) {
          if (alive && checkouts?.some(j => j.id === job.id)) {
            // A locally authorized order may not have reached the service yet.
            remoteErrors[job.id] = !orders[job.id] && code(cause) === 'NOT_FOUND' ? '' : errorText(cause)
          }
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
    await readRemoteOrders()
  }))

  const readPayer = single(async () => {
    if (!alive) return
    payerReading = true
    await part('payer', async () => {
      const next = await call('payer/status')
      if (alive) payer = next
    })
    if (alive) payerReading = false
  })
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

  async function attempt(work: () => Promise<unknown>, key = 'refresh') {
    if (busy) return
    busy = true
    actionErrors[key] = ''
    try {
      await work()
    } catch (e) {
      if (alive) {
        unknown[key] = ['EXECUTION_UNKNOWN', 'PENDING', 'RESULT_EXPIRED'].includes(code(e))
        actionErrors[key] = unknown[key] ? t('operationUnknown') : errorText(e)
      }
    } finally { if (alive) busy = false }
  }

  async function checkOrder(id: string) {
    await attempt(async () => {
      allOrders = true
      await refreshOrders()
      if (alive && !remoteErrors[id] && !errors.orders) { actionErrors[id] = ''; unknown[id] = false }
    }, 'refresh')
  }

  function act(action: OrderAction, job: any) {
    if (busy || unknown[job.id] || !orderActions(job, orders[job.id], Date.now(), refundable).includes(action)) return
    switch (action) {
      case 'pay':
        return attempt(() => reviewApproval('checkout:' + job.id, () => call('plan/checkout', { id: job.id }), () => alive), job.id)
      case 'advance':
        return attempt(() => reviewApproval('advance:' + job.id, () => call('plan/advance', { id: job.id }), () => alive), job.id)
      case 'discard':
        return attempt(async () => {
          await call('plan/discard', { id: job.id })
          await refreshOrders()
        }, job.id)
      case 'reconcile':
        return attempt(async () => {
          await call('plan/reconcile', { id: job.id })
          if (alive) { allOrders = true; await refreshOrders() }
        }, job.id)
      case 'cancel':
        cancelling = { job }
        return
      case 'refund':
        // The refund is quoted first: what comes back, the network fee, and when the rights stop.
        return attempt(async () => {
          const quote = await call('plan/refund_quote', { id: job.id })
          if (alive) cancelling = { job, quote }
        }, job.id)
    }
  }

  async function confirmCancel() {
    const refund = cancelling?.quote != null
    const done: any = await call('plan/cancel', { id: cancelling!.job.id })
    if (!alive) return
    const outcome = refundOutcome(done?.refunds ?? [])
    toast(() => outcome.error ? `${t('error')}: ${outcome.error}` : t(refund || done?.refunds?.length ? outcome.label : 'orderCancelledNote'))
    void reload()
  }

  /** The payer: a PEM file the main process picks, or the login again. */
  const usePayer = (login: boolean) =>
    attempt(async () => {
      const done = await bridge(() => window.dmsg.payerPem(login))
      if (alive && done) await readPayer()
    }, 'payer')

  onMount(() => {
    const stop = watchDisplays(['catalog'], (mode) => readCatalog(mode))
    void reload()
    const off = onChange(['approval/changed', 'session/changed'], reload)
    const clock = setInterval(() => { now = Date.now() }, 1000)
    const timer = setInterval(() => {
      if ((checkouts ?? []).some((job) => moving(job, orders[job.id], now))) void refreshOrders()
    }, 15_000)
    return () => {
      alive = false
      off()
      stop()
      clearInterval(timer)
      clearInterval(clock)
    }
  })

  /** What an order buys, for people. */
  function title(job: any) {
    const sku = skuOf(job.sku, catalog)
    if (sku.kind === 'upgrade') return t('upgradeTo', { plan: sku.plan ?? '' })
    if (sku.kind === 'storage') return sku.bytes != null ? t('storagePackOf', { size: size(sku.bytes) }) : t('storagePack')
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

{#snippet walletPanel()}
  <div bind:this={walletRegion} role="region" aria-label={t('wallet')} tabindex="-1" class="min-w-0 outline-none">
  <Section title={t('wallet')} description={t('walletHint')} icon="wallet" testid="plan-wallet">
    {#snippet actions()}
      <button class={buttonClass('ghost', 'sm')} disabled={busy || payerReading} aria-busy={payerReading} aria-label={`${t('refresh')} · ${t('balance')}`} onclick={readPayer} data-testid="payer-refresh"><Icon name="sync-now" />{t('refresh')}</button>
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => usePayer(false)} data-testid="payer-pem"><Icon name="key" />{t('usePemFile')}</button>
      {#if payer?.source === 'pem'}
        <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => usePayer(true)} data-testid="payer-login">{t('useLoginPayer')}</button>
      {/if}
    {/snippet}
    {#if errors.payer}
      <Notice tone="warn" testid="payer-read-error">{errors.payer}</Notice>
    {/if}
    {#if actionErrors.payer}<Notice tone="danger">{actionErrors.payer}</Notice>{/if}
    {#if payer}
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
            {:else if row.balance == null}
              <span class="text-muted">{t('wallet.unknown')}</span>
            {:else}
              <span class="font-semibold tabular-nums">{units(String(row.balance), row.decimals)} {row.asset}</span>
            {/if}
          </Fact>
        {/each}
      </dl>
      <p class="mt-3 text-[12px] text-faint">{t('topUpHint')}</p>
    {:else if !errors.payer}<p class="text-xs text-muted" role="status">{t('loading')}</p>{/if}
    {#if fundingOrder}
      <p class="mt-3 flex flex-wrap items-center gap-2 text-xs text-muted">
        <span>{t('order')} · {title(fundingOrder)} · {amount(fundingOrder)}</span>
        <button class={buttonClass('secondary', 'xs')} aria-controls={'plan-order-' + fundingOrder.id} data-testid="payer-open-order"
          onclick={() => document.getElementById('plan-order-' + fundingOrder.id)?.focus()}>{t('openOrders')}</button>
      </p>
    {/if}
  </Section>
  </div>
{/snippet}

<Page>
  {#if walletFirst}{@render walletPanel()}{/if}
  <Section title={t('orders')} description={t('ordersHint')} icon="audit" flush testid="plan-orders">
    {#snippet actions()}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => attempt(reload)} data-testid="orders-refresh"><Icon name="sync-now" />{t('refresh')}</button>
    {/snippet}
    {#if errors.orders}
      <div class="px-5 pb-4"><Notice tone="warn" testid="orders-read-error">{errors.orders}</Notice></div>
    {/if}
    {#if errors.status || errors.catalog}<div class="px-5 pb-3"><Notice tone="warn">{errors.status || errors.catalog}</Notice></div>{/if}
    {#if checkouts && !checkouts.length && !errors.orders}
      <p class="px-5 pb-4 text-[12.5px] text-faint" data-testid="no-orders">{t('noOrders')}</p>
    {:else if checkouts}
      <ul class="divide-y divide-line" data-testid="checkouts">
        {#each checkouts as job (job.id)}
          {@const order = orders[job.id]}
          {@const state = orderState(job, order, now)}
          {@const draft = job.stage === 'review'}
          {@const expired = draft && quoteExpired(job, now)}
          {@const deadline = draft ? quoteDeadline(job) : deadlineOf(job)}
          <li id={'plan-order-' + job.id} tabindex="-1" class="grid gap-1 px-5 py-3 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-forest" data-testid="order">
            <div class="flex flex-wrap items-center gap-2">
              <b class="text-[13px] font-medium" data-testid="order-title">{title(job)}</b>
              {#if renewal(job)}<Pill>{t('renewal')}</Pill>{/if}
              <Pill tone={state.tone} testid="order-state">{t(state.label)}</Pill>
              <span class="flex-1"></span>
              {#if expired}<button class={buttonClass('secondary', 'xs')} onclick={onMembership} data-testid="order-view-plans">{t('plans')}</button>{/if}
              {#each orderActions(job, order, now, refundable) as action (action)}
                <button
                  class={buttonClass(action === 'pay' || action === 'advance' ? 'primary' : action === 'cancel' || action === 'refund' ? 'danger' : 'secondary', 'xs')}
                  disabled={busy || !!errors.orders || !!unknown[job.id] || (!!remoteErrors[job.id] && ['advance','cancel','refund'].includes(action))}
                  aria-label={`${t(actionLabels[action])} · ${title(job)} · ${job.id.slice(0, 8)}`}
                  onclick={() => act(action, job)}
                  data-testid={`order-${action}`}>{t(actionLabels[action])}</button
                >
              {/each}
            </div>
            {#if remoteErrors[job.id] || actionErrors[job.id]}<Notice tone={remoteErrors[job.id] || unknown[job.id] ? 'warn' : 'danger'} testid="order-error">
              {actionErrors[job.id] || `${t('planOrderReadFailed')} · ${remoteErrors[job.id]}`}
              {#if remoteErrors[job.id] || unknown[job.id]}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => checkOrder(job.id)}>{t('confirmCheckStatus')}</button>{/if}
            </Notice>{/if}
            <div class="flex flex-wrap gap-x-3 gap-y-0.5 text-[12px] text-muted tabular-nums">
              <span data-testid="order-amount">{amount(job)}</span>
              <span data-testid="order-period">{t('servicePeriod', { from: when(order?.starts_at_ms ?? job.starts_at_ms), to: when(order?.expires_at_ms ?? job.expires_at_ms) })}</span>
              {#if deadline && awaiting(job, order)}
                <span class="text-warn" data-testid="order-deadline">{draft ? `${t('expires')} · ${when(deadline)}` : t(job.terms?.method === 'Cash' ? 'payBy' : 'applyBy', { time: when(deadline) })}</span>
              {/if}
              <span class="text-faint">{ago(job.created_at_ms, now)}</span>
            </div>
          </li>
        {/each}
      </ul>
    {:else if !checkouts && !errors.orders}<p class="px-5 text-xs text-muted" role="status">{t('loading')}</p>{/if}
  </Section>

  {#if !walletFirst}{@render walletPanel()}{/if}

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
  busy={busy || (!!cancelling && (!orderActions(cancelling.job, orders[cancelling.job.id], now, refundable).includes(cancelling.quote ? 'refund' : 'cancel') || !!remoteErrors[cancelling.job.id] || !!errors.orders))}
  focusCancel
  testid="order-cancel-dialog"
  onConfirm={confirmCancel}
  onReview={reload}
>
  {#snippet body()}
    {#if cancelling}<dl>
      <Fact label={t('order')}>{title(cancelling.job)} · {amount(cancelling.job)}</Fact>
      <Fact label={t('serviceTerm')}>{when(cancelling.job.starts_at_ms)} – {when(cancelling.job.expires_at_ms)}</Fact>
      <Fact label={t('planOrderId')}><Id id={cancelling.job.id} full wrap /></Fact>
    </dl>{/if}
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
