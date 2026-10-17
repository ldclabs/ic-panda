<script lang="ts">
  import { RadioGroup } from 'bits-ui'
  import { onMount } from 'svelte'
  import { copyText } from '../components/clipboard'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Field from '../components/page/Field.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Page from '../components/page/Page.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import Select from '../components/Select.svelte'
  import { toast } from '../components/toast.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, errorText, onChange, single, size, units, when } from '../rpc'
  import Operations from './Operations.svelte'

  /**
   * The plan and its limits, the plans on offer, checkouts, names, and the
   * wallet. A quote only fixes the terms; paying, claiming, registering and
   * accepting a name are approval requests.
   */
  let { status }: { status: any } = $props()

  let plan = $state<any>(null)
  let catalog = $state<any>(null)
  let assets = $state<any[]>([])
  let checkouts = $state<any[]>([])
  let orders = $state<Record<string, any>>({})
  let sku = $state('')
  let ledger = $state('')
  let wallet = $state<any>(null)
  let name = $state('')
  let handle = $state<any>(null)
  let target = $state('')
  let offer = $state<any>(null)
  let incoming = $state('')
  let operations = $state<any[]>([])
  let blocks = $state<Record<string, string>>({})
  let error = $state('')
  let busy = $state(false)

  const unlocked = $derived(status?.state === 'unlocked')
  const asset = $derived(assets.find((a) => a.ledger === ledger))
  const offered = $derived((catalog?.plans ?? []).filter((p: any) => p.price_cents > 0))

  $effect(() => {
    if (!sku && offered.length) sku = String(offered[0].plan_id).toLowerCase()
  })

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

  /** The plan, read from the relay again with `refresh`; the catalog once. */
  async function load(refresh = false) {
    try {
      ;[plan, catalog, assets, checkouts, operations] = await Promise.all([
        call('plan/status', { refresh }),
        catalog ?? call('plan/catalog'),
        call('plan/assets'),
        call('plan/checkouts'),
        call('handle/operations')
      ])
      if (!ledger && assets.length) ledger = assets[0].ledger
      error = ''
    } catch (e) {
      error = errorText(e)
    }
  }

  const reload = single(() => load())

  const approve = async (request: any) => bridge(() => window.dmsg.openApproval(request.id))

  /** A quote only fixes the terms; paying it is a separate approval, whose `approval/changed` reloads the checkouts. */
  const checkout = () =>
    attempt(async () => {
      const job: any = await call('plan/quote', { sku, method: 'cash', selection: ledger })
      await approve(await call('plan/checkout', { id: job.id }))
    })

  const order = (method: string, id: string) =>
    attempt(async () => {
      orders[id] = await call(method, { id })
    })

  const balance = () =>
    attempt(async () => {
      wallet = await call('payer/balance', { ledger })
    })

  const lookup = () =>
    attempt(async () => {
      handle = await call('handle/price', { name: name.trim() })
    })

  const take = (method: 'handle/claim' | 'handle/register') => attempt(async () => approve(await call(method, { name: handle.name })))

  /** The owner offers; the target accepts; the owner commits within the minute the acceptance lasts. */
  const transfer = () =>
    attempt(async () => {
      offer = await call('handle/transfer', { name: handle.name, target: target.trim() })
      operations = await call('handle/operations')
    })

  const accept = () =>
    attempt(async () => {
      await approve(await call('handle/accept', { token: incoming.trim() }))
      incoming = ''
    })

  const commit = (id: string) => attempt(async () => approve(await call('handle/transfer_commit', { id })))

  const settle = (id: string) =>
    attempt(async () => {
      await call('handle/reconcile', { id, block: Number(blocks[id]) })
      operations = await call('handle/operations')
    })

  onMount(() => {
    void reload()
    return onChange(['approval/changed', 'session/changed'], reload)
  })

  const price = (cents: number) => `$${(cents / 100).toFixed(2)}`
</script>

<Page>
  {#if error}<Notice tone="danger" testid="account-error">{error}</Notice>{/if}

  <section class="overflow-hidden rounded-2xl border border-line bg-surface shadow-xs">
    <div class="flex flex-wrap items-start gap-4 bg-gradient-to-br from-pale to-surface px-6 py-5">
      <span class="grid size-12 shrink-0 place-items-center rounded-2xl bg-forest text-on-forest [&_svg]:size-6"><Icon name="plan" /></span>
      <div class="min-w-0 flex-1">
        <p class="text-[12px] font-medium text-faint">{t('plan')}</p>
        <p class="text-[26px] leading-tight font-semibold" data-testid="plan-name">{plan?.plan ?? '—'}</p>
        {#if plan}
          <div class="mt-1.5 flex flex-wrap items-center gap-1.5 text-[12px] text-muted">
            {#if plan.source}<Pill>{plan.source}</Pill>{/if}
            {#if plan.eligibility}<Pill tone="forest">{plan.eligibility}</Pill>{/if}
            {#if plan.valid_until_ms}<span>{t('validUntil')} {when(plan.valid_until_ms)}</span>{/if}
          </div>
        {/if}
      </div>
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => attempt(() => load(true))}><Icon name="sync-now" />{t('refresh')}</button>
    </div>
    {#if plan}
      <dl class="divide-y divide-line border-t border-line px-6">
        <Fact label={t('storage')}>{size(plan.storage_bytes ?? 0)}</Fact>
        {#if plan.effective_stop_at_ms}<Fact label={t('stopsAt')}>{when(plan.effective_stop_at_ms)}</Fact>{/if}
      </dl>
    {/if}
  </section>

  {#if offered.length}
    <Section title={t('plans')} description={t('plansHint')} icon="coins">
      <RadioGroup.Root bind:value={sku} class="grid gap-2 sm:grid-cols-3" aria-label={t('plans')}>
        {#each offered as p (p.plan_id)}
          {@const value = String(p.plan_id).toLowerCase()}
          <RadioGroup.Item
            {value}
            class="group grid gap-2 rounded-xl border border-line bg-surface p-4 text-start outline-none transition-colors duration-150 hover:border-edge-soft focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=checked]:border-forest data-[state=checked]:bg-pale"
            data-testid={`plan-${value}`}
          >
            <div class="flex items-center justify-between gap-2">
              <b class="text-[15px] font-semibold">{p.plan_id}</b>
              {#if plan?.plan === p.plan_id}<Pill tone="forest">{t('current')}</Pill>{/if}
            </div>
            <p><span class="text-[22px] font-semibold tabular-nums">{price(p.price_cents)}</span> <span class="text-[12px] text-faint">/ {t('year')}</span></p>
            <ul class="grid gap-1 text-[12px] text-muted">
              <li class="flex items-center gap-1.5"><Icon name="sync" class="size-3.5 text-faint" />{size(p.limits.storage_bytes)}</li>
              <li class="flex items-center gap-1.5"><Icon name="channels" class="size-3.5 text-faint" />{t('channelSlots', { n: p.limits.active_channels })}</li>
              <li class="flex items-center gap-1.5"><Icon name="signature" class="size-3.5 text-faint" />{t('executionUnits', { n: p.limits.monthly_execution_units })}</li>
            </ul>
          </RadioGroup.Item>
        {/each}
      </RadioGroup.Root>
      {#if unlocked}
        <div class="mt-4 flex flex-wrap items-end justify-end gap-2">
          <Field label={t('payWith')}>
            <Select bind:value={ledger} testid="plan-asset" placeholder={t('choose')} options={assets.map((a) => ({ value: a.ledger, label: a.asset }))} />
          </Field>
          <button class={buttonClass('primary')} disabled={busy || !ledger || !sku} onclick={checkout}>{t('subscribe')}</button>
        </div>
      {/if}
    </Section>
  {/if}

  {#if checkouts.length}
    <Section title={t('orders')} icon="audit" flush>
      <ul class="divide-y divide-line border-t border-line" data-testid="checkouts">
        {#each checkouts as job (job.id)}
          <li class="flex flex-wrap items-center gap-3 px-5 py-3">
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-1.5">
                <b class="text-[13px] font-medium">{job.sku}</b>
                <Pill>{job.stage}</Pill>
                {#if orders[job.id]}<Pill tone="forest">{orders[job.id].status}</Pill>{/if}
              </div>
              <p class="text-[12px] text-muted tabular-nums">
                {job.terms.method === 'Cash' ? `${units(job.terms.amount, job.terms.decimals)} ${job.terms.asset}` : 'PANDA'}
              </p>
            </div>
            <div class="flex flex-wrap gap-1.5">
              <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => order('plan/order', job.id)}>{t('status')}</button>
              {#if job.stage === 'review'}
                <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => attempt(async () => approve(await call('plan/checkout', { id: job.id })))}>{t('pay')}</button>
              {:else}
                <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => order('plan/reconcile', job.id)}>{t('reconcile')}</button>
                <button class={buttonClass('danger', 'xs')} disabled={busy} onclick={() => order('plan/cancel', job.id)}>{t('cancel')}</button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    </Section>
  {/if}

  <Section title={t('names')} description={t('namesHint')} icon="user">
    <div class="grid gap-4">
      <div class="flex gap-2">
        <label class="relative min-w-0 flex-1">
          <span class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint">@</span>
          <input bind:value={name} class="ps-7" placeholder={t('name')} data-testid="handle-name" onkeydown={(e) => e.key === 'Enter' && name.trim() && lookup()} />
        </label>
        <button class={buttonClass('secondary')} disabled={busy || !name.trim()} onclick={lookup}><Icon name="search" />{t('lookUp')}</button>
      </div>
      {#if handle}
        <div class="flex flex-wrap items-center gap-3 rounded-xl border border-line px-4 py-3" data-testid="handle-state">
          <b class="text-[15px] font-semibold">@{handle.name}</b>
          {#if handle.owner}
            <Pill>{t('taken')}</Pill><Id id={handle.owner} />
          {:else if handle.reserved}
            <Pill tone="gem">{t('reserved')}</Pill>
          {:else}
            <Pill tone="ok">{t('available')}</Pill><span class="text-[13px] tabular-nums">{units(handle.price, 8)} PANDA</span>
          {/if}
          <span class="flex-1"></span>
          {#if unlocked && !handle.owner}
            <button class={buttonClass('primary', 'sm')} disabled={busy} onclick={() => take(handle.reserved ? 'handle/claim' : 'handle/register')}>
              {handle.reserved ? t('claim') : t('register')}
            </button>
          {/if}
        </div>
        {#if unlocked && handle.owner === status?.account}
          <div class="grid gap-2 rounded-xl border border-line bg-surface-2/60 p-3">
            <div class="flex items-end gap-2">
              <Field label={t('transferName')} class="flex-1"><input bind:value={target} placeholder={t('targetAccount')} class="font-mono text-[12.5px] placeholder:font-sans" data-testid="transfer-target" /></Field>
              <button class={buttonClass('secondary')} disabled={busy || !target.trim()} onclick={transfer}>{t('transferName')}</button>
            </div>
            {#if offer}
              <Notice>
                <p>{t('transferHint')}</p>
                <p class="mt-1 font-mono text-[11.5px] break-all select-all" data-testid="transfer-token">{offer.token}</p>
                {#snippet action()}<button class={buttonClass('secondary', 'xs')} onclick={() => copyText(offer.token)}><Icon name="copy" />{t('copy')}</button>{/snippet}
              </Notice>
            {/if}
          </div>
        {/if}
      {/if}
      {#if unlocked}
        <div class="flex items-end gap-2">
          <Field label={t('acceptName')} class="flex-1"><input bind:value={incoming} placeholder={t('token')} class="font-mono text-[12.5px] placeholder:font-sans" data-testid="accept-token" /></Field>
          <button class={buttonClass('secondary')} disabled={busy || !incoming.trim()} onclick={accept}>{t('acceptName')}</button>
        </div>
      {/if}
      {#if operations.length}
        <h3 class="-mb-2 text-[12px] font-medium text-faint">{t('nameOperations')}</h3>
        <ul class="divide-y divide-line rounded-xl border border-line" data-testid="name-operations">
          {#each operations as op (op.id)}
            <li class="flex flex-wrap items-center gap-2 px-3.5 py-2.5 text-[12.5px]">
              <span class="min-w-0 flex-1">@{op.name} · {op.action}{op.to ? ` → ${op.to}` : ''}</span>
              <Pill>{op.phase}</Pill>
              {#if op.action === 'Transfer' && op.phase === 'prepared'}
                <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => commit(op.id)}>{t('commit')}</button>
              {/if}
              {#if op.action === 'Register' && op.phase === 'unknown'}
                <input class="h-6 w-24 text-[12px]" placeholder={t('block')} bind:value={blocks[op.id]} />
                <button class={buttonClass('secondary', 'xs')} disabled={busy || !/^\d+$/.test(blocks[op.id] ?? '')} onclick={() => settle(op.id)}>{t('reconcile')}</button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </Section>

  {#if unlocked}
    <Section title={t('wallet')} description={t('walletHint')} icon="coins">
      {#snippet actions()}
        <button class={buttonClass('secondary', 'sm')} disabled={busy || !ledger} onclick={balance}>{t('balance')}</button>
      {/snippet}
      {#if wallet}
        <dl class="divide-y divide-line">
          <Fact label={t('payer')}><Id id={wallet.principal} full /></Fact>
          <Fact label={t('balance')} testid="wallet-balance">
            <span class={cn('text-[15px] font-semibold tabular-nums')}>{asset ? `${units(wallet.balance, asset.decimals)} ${asset.asset}` : wallet.balance}</span>
          </Fact>
        </dl>
      {:else}
        <p class="text-[12.5px] text-faint">{asset ? asset.asset : '-'}</p>
      {/if}
    </Section>
    <Operations />
  {/if}
</Page>
