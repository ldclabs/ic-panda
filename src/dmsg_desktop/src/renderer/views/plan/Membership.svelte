<script lang="ts">
  import { onMount } from 'svelte'
  import Confirm from '../../components/Confirm.svelte'
  import Icon from '../../components/Icon.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Page from '../../components/page/Page.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText, onChange, single, size, units, when } from '../../rpc'
  import Checkout from './Checkout.svelte'
  import { daysLeft, eligibilityLabels, methodLabels, offerLabels, refundOutcome, sourceLabels, waitLabels } from './plan'

  /**
   * The membership: the plan in force and its term, what it allows and what
   * is used of it, the relay's notices, the first purchase's refund window,
   * and what each plan and storage pack offers this account now. Each part
   * is read on its own, so one that fails leaves the others. Buying opens
   * the checkout dialog; paying is the approval window's.
   */
  let plan = $state<any>(null)
  let catalog = $state<any>(null)
  let cloud = $state<any>(null)
  let execution = $state<any>(null)
  let errors = $state<Record<string, string>>({})
  let busy = $state(false)
  /** The offer the checkout dialog pays: a plan card's or a storage pack's. */
  let buying = $state<any>(null)
  /** The first purchase's refund, quoted before it is confirmed. */
  let refund = $state<any>(null)
  /** When this page last opened a payment: approval changes soon after read the plan from the relay again. */
  let paidAt = 0
  let alive = true
  let refreshPlan = false

  const current = $derived(plan?.current ?? null)
  const offers = $derived(new Map<string, any>((plan?.offers ?? []).map((o: any) => [o.plan, o])))
  const paid = $derived<any[]>((catalog?.plans ?? []).filter((p: any) => p.price_cents > 0))
  const packs = $derived<any[]>(catalog?.storage_products ?? [])
  const notice = $derived(cloud?.notice ?? null)

  /** One part of the page: its failure shows in its place and leaves the others. */
  async function part(key: string, read: () => Promise<void>) {
    try {
      await read()
      errors[key] = ''
    } catch (e) {
      if (alive) errors[key] = errorText(e)
    }
  }

  // Each part coalesces its own reads. A slow usage request never queues
  // a later entitlement refresh behind it.
  const readPlan = single(() => part('plan', async () => {
    if (!alive) return
    const refresh = refreshPlan || Date.now() - paidAt < 15 * 60_000
    refreshPlan = false
    const next = await call('plan/status', { refresh })
    if (alive) plan = next
  }))
  const readCatalog = single(() => part('catalog', async () => {
    if (catalog || !alive) return
    const next = await call('plan/catalog')
    if (alive) catalog = next
  }))
  const readCloud = single(() => part('cloud', async () => {
    if (!alive) return
    const next = await call('files/cloud_usage')
    if (alive) cloud = next
  }))
  const readExecution = single(() => part('execution', async () => {
    if (!alive) return
    const next = await call('attest/usage')
    if (alive) execution = next
  }))

  function load(refresh = false) {
    refreshPlan ||= refresh
    void readCatalog()
    void readCloud()
    void readExecution()
    return readPlan()
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

  /** The refund is quoted first: what comes back, the network fee, and when the rights stop. */
  const quoteRefund = () =>
    attempt(async () => {
      refund = await call('plan/refund_quote', { id: plan.refund.order })
    })

  const confirmRefund = () =>
    attempt(async () => {
      const done: any = await call('plan/cancel', { id: refund.order })
      refund = null
      const outcome = refundOutcome(done?.refunds ?? [])
      toast(() => outcome.error ? `${t('error')}: ${outcome.error}` : t(outcome.label))
      void load(true)
    })

  /** Acknowledging the notice starts its export window now at the latest. */
  const acknowledge = () =>
    attempt(async () => {
      await call('plan/notice_ack', { id: notice.id })
      void readCloud()
    })

  const buy = (p: any, offer: any) => (buying = { kind: 'plan', plan: p.plan_id, price_cents: p.price_cents, ...offer })
  const buyPack = (pack: any) =>
    (buying = { kind: 'storage', sku: pack.product_id, bytes: pack.storage_bytes, price_cents: pack.price_cents, methods: ['Cash'], until: current?.ends_at_ms })

  onMount(() => {
    void load()
    const off = onChange(['approval/changed', 'session/changed'], () => {
      void readPlan()
      void readExecution()
      if (Date.now() - paidAt < 15 * 60_000) void readCloud()
    })
    return () => { alive = false; off() }
  })

  const price = (cents: number) => `$${(cents / 100).toFixed(2)}`
  const share = (used?: number | null, limit?: number | null) => (used != null && limit ? Math.min(100, Math.round((used / limit) * 100)) : 0)
  const label = (labels: Record<string, any>, value: string) => (labels[value] ? t(labels[value]) : value)
</script>

{#snippet ack()}
  <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={acknowledge} data-testid="plan-notice-ack">{t('acknowledge')}</button>
{/snippet}

{#snippet meter(name: string, used: string, limit: string, percent: number, testid: string)}
  <div class="grid gap-1.5" data-testid={testid}>
    <div class="flex items-baseline justify-between gap-2 text-[12.5px]">
      <span class="text-muted">{name}</span>
      <span class="tabular-nums"><b class="font-semibold">{used}</b> <span class="text-faint">/ {limit}</span></span>
    </div>
    <div class="h-1.5 overflow-hidden rounded-full bg-surface-2">
      <div class={cn('h-full rounded-full', percent >= 100 ? 'bg-danger' : percent >= 80 ? 'bg-warn' : 'bg-forest')} style:width="{percent}%"></div>
    </div>
  </div>
{/snippet}

<Page>
  <section class="px-5" data-testid="plan-hero">
    <div class="flex flex-wrap items-start gap-3">
      <div class="min-w-0 flex-1">
        <p class="text-[12px] font-medium text-faint">{t('plan')}</p>
        <p class="text-[26px] leading-tight font-semibold" data-testid="plan-name">{plan?.plan ?? '—'}</p>
        {#if plan}
          <div class="mt-1.5 flex flex-wrap items-center gap-1.5 text-[12px] text-muted">
            {#if plan.source}<Pill testid="plan-source">{label(sourceLabels, plan.source)}</Pill>{/if}
            {#if plan.eligibility}<Pill tone={plan.eligibility === 'Eligible' ? 'forest' : 'warn'}>{label(eligibilityLabels, plan.eligibility)}</Pill>{/if}
            {#if current}
              <Pill>{label(methodLabels, current.method)}</Pill>
              <span data-testid="plan-ends">{t('termEnds', { time: when(current.ends_at_ms), n: daysLeft(current.ends_at_ms, Date.now()) })}</span>
            {/if}
          </div>
        {/if}
      </div>
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => attempt(() => load(true))} data-testid="plan-refresh"><Icon name="sync-now" />{t('refresh')}</button>
    </div>
    {#if errors.plan}<div class="px-6 pb-4"><Notice failure={errors.plan} tone="danger" testid="plan-error">{errors.plan}</Notice></div>{/if}
    {#if plan}
      <dl class="mt-3">
        {#if current}<Fact label={t('term')} testid="plan-term">{when(current.term_starts_at_ms ?? current.starts_at_ms)} – {when(current.ends_at_ms)}</Fact>{/if}
        {#each plan.addons ?? [] as addon (addon.order)}
          <Fact label={t('storagePack')}>+{size(addon.storage_bytes)} · {t('until', { time: when(addon.expires_at_ms) })}</Fact>
        {/each}
        {#if plan.next}
          <Fact label={t('renewal')} testid="plan-next">{plan.next.plan} · {when(plan.next.starts_at_ms)} – {when(plan.next.expires_at_ms)}</Fact>
        {:else if plan.renew_from_ms}
          <Fact label={t('renewal')}>{t('renewalOpens', { time: when(plan.renew_from_ms) })}</Fact>
        {/if}
      </dl>
    {/if}
  </section>

  {#if plan?.refund}
    <Notice tone="info" icon="info" testid="plan-refund">
      {t('refundWindow', { time: when(plan.refund.until_ms) })}
      {#snippet action()}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={quoteRefund} data-testid="plan-refund-quote">{t('requestRefund')}</button>{/snippet}
    </Notice>
  {/if}
  {#if plan?.service_terminated_at_ms}<Notice tone="danger" icon="warning">{t('serviceEnded', { time: when(plan.service_terminated_at_ms) })}</Notice>{/if}
  {#if notice}
    <Notice tone="warn" icon="warning" testid="plan-notice" action={notice.acknowledged_at ? undefined : ack}>
      <p>{t('overLimitNotice')}</p>
      {#if notice.minimum_export_until}<p class="mt-1">{t('exportUntil', { time: when(notice.minimum_export_until) })}</p>{/if}
    </Notice>
  {:else if cloud?.status === 'over_limit'}
    <Notice tone="warn" icon="warning" testid="plan-over-limit">{t('overLimit')}</Notice>
  {/if}

  <Section title={t('usageOverview')} icon="health" testid="plan-usage">
    <div class="grid gap-4 sm:grid-cols-3">
      {#if cloud}
        {@render meter(t('storage'), size(cloud.reserved + cloud.committed), cloud.storage_limit ? size(cloud.storage_limit) : '-', share(cloud.reserved + cloud.committed, cloud.storage_limit), 'usage-storage')}
        {@render meter(t('activeChannels'), String(cloud.active_channels ?? '-'), String(cloud.channel_limit ?? '-'), share(cloud.active_channels, cloud.channel_limit), 'usage-channels')}
      {:else}
        {@render meter(t('storage'), '—', plan?.limits ? size(plan.limits.storage_bytes) : '—', 0, 'usage-storage')}
        {@render meter(t('activeChannels'), '—', String(plan?.limits?.active_channels ?? '—'), 0, 'usage-channels')}
      {/if}
      {#if execution}
        {@render meter(t('signaturesThisMonth'), String(execution.charged), String(execution.allowed), share(execution.charged, execution.allowed), 'usage-signatures')}
      {:else}
        {@render meter(t('signaturesThisMonth'), '—', '—', 0, 'usage-signatures')}
      {/if}
    </div>
    {#if plan?.limits}<p class="mt-3 text-[12px] text-muted">{t('signaturesMonthly')}: {plan.limits.monthly_execution_units}</p>{/if}
    {#if errors.cloud}<Notice failure={errors.cloud} tone="danger" class="mt-3">{errors.cloud}</Notice>{/if}
    {#if errors.execution}<Notice failure={errors.execution} tone="danger" class="mt-3">{errors.execution}</Notice>{/if}
  </Section>

  {#if errors.catalog}<Notice failure={errors.catalog} tone="danger">{errors.catalog}</Notice>{/if}
  {#if paid.length}
    <Section title={t('plans')} description={t('plansHint')} icon="coins" testid="plan-cards">
      <div class="grid gap-2 sm:grid-cols-3">
        {#each paid as p (p.plan_id)}
          {@const offer = offers.get(p.plan_id)}
          {@const value = String(p.plan_id).toLowerCase()}
          {@const mine = current?.plan === p.plan_id}
          <div class={cn('grid content-start gap-2 rounded-xl border p-4', mine ? 'border-forest bg-pale' : 'border-line bg-surface')} data-testid={`plan-${value}`}>
            <div class="flex items-center justify-between gap-2">
              <b class="text-[15px] font-semibold">{p.plan_id}</b>
              {#if mine}<Pill tone="forest">{t('current')}</Pill>{/if}
            </div>
            <p><span class="text-[22px] font-semibold tabular-nums">{price(p.price_cents)}</span> <span class="text-[12px] text-faint">/ {t('year')}</span></p>
            <ul class="grid gap-1 text-[12px] text-muted">
              <li class="flex items-center gap-1.5"><Icon name="sync" class="size-3.5 text-faint" />{size(p.limits.storage_bytes)}</li>
              <li class="flex items-center gap-1.5"><Icon name="channels" class="size-3.5 text-faint" />{t('channelSlots', { n: p.limits.active_channels })}</li>
              <li class="flex items-center gap-1.5"><Icon name="signature" class="size-3.5 text-faint" />{t('executionUnits', { n: p.limits.monthly_execution_units })}</li>
            </ul>
            {#if offer && offer.action !== 'unavailable'}
              <button
                class={buttonClass(offer.action === 'subscribe' || offer.action === 'upgrade' ? 'primary' : 'secondary', 'sm', 'mt-1')}
                disabled={busy}
                onclick={() => buy(p, offer)}
                data-testid={`plan-${value}-${offer.action}`}>{t(offerLabels[offer.action] ?? 'subscribe')}</button
              >
              {#if offer.action === 'upgrade'}
                <p class="text-[11.5px] text-faint">{t('upgradeProrated')}</p>
              {:else if offer.action !== 'subscribe'}
                <p class="text-[11.5px] text-faint">{t('startsAt', { time: when(offer.starts_at_ms) })}</p>
              {/if}
            {:else if offer}
              <p class="text-[11.5px] text-faint" data-testid={`plan-${value}-wait`}>{waitLabels[offer.reason] ? t(waitLabels[offer.reason], { time: when(offer.available_at_ms) }) : offer.reason}</p>
            {/if}
          </div>
        {/each}
      </div>
    </Section>
  {/if}

  {#if packs.length}
    <Section
      title={t('storagePacks')}
      description={plan?.storage_available ? t('storagePacksHint', { time: when(current?.ends_at_ms) }) : t('storagePacksPaidOnly')}
      icon="database"
      testid="plan-packs"
    >
      <ul class="grid gap-2 sm:grid-cols-3">
        {#each packs as pack (pack.product_id)}
          <li class="flex items-center gap-3 rounded-xl border border-line px-4 py-3">
            <div class="min-w-0 flex-1">
              <b class="text-[14px] font-semibold">+{size(pack.storage_bytes)}</b>
              <p class="text-[12px] text-faint tabular-nums">{price(pack.price_cents)} / {t('year')}</p>
            </div>
            <button class={buttonClass('secondary', 'xs')} disabled={busy || !plan?.storage_available} onclick={() => buyPack(pack)} data-testid="plan-pack-buy">{t('addStorage')}</button>
          </li>
        {/each}
      </ul>
    </Section>
  {/if}
</Page>

<Checkout bind:offer={buying} onPaid={() => (paidAt = Date.now())} />

<Confirm
  bind:open={() => refund !== null, (open) => (refund = open ? refund : null)}
  title={t('requestRefund')}
  description={t('refundHint', { time: when(refund?.stops_at_ms) })}
  confirm={t('requestRefund')}
  tone="warn"
  {busy}
  testid="refund"
  onConfirm={confirmRefund}
>
  {#snippet body()}
    {#if refund}
      <dl>
        <Fact label={t('refundAmount')} testid="refund-amount">{units(refund.refund, refund.decimals)} {refund.asset}</Fact>
        <Fact label={t('networkFee')}>{units(refund.fee, refund.decimals)} {refund.asset}</Fact>
        <Fact label={t('refundReceived')}>{units(refund.received, refund.decimals)} {refund.asset}</Fact>
      </dl>
    {/if}
  {/snippet}
</Confirm>
