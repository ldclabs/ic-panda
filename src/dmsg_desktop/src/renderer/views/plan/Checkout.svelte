<script lang="ts">
  import { RadioGroup } from 'bits-ui'
  import { onDestroy, untrack } from 'svelte'
  import CopyButton from '../../components/CopyButton.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Field from '../../components/page/Field.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Radio from '../../components/Radio.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { call, code, errorText, single, size, units, when } from '../../rpc'
  import { reviewApproval } from './approval'
  import { cashTotal, neuronUnlockAt, reusableQuote } from './plan'

  /**
   * Paying for an offer: a plan card's subscription, renewal, switch or
   * upgrade, or a storage pack. Cash picks the asset; PANDA picks a neuron
   * the payer may act for, whose stake and lock cover the term. The quote
   * fixes the terms here; approving them is the approval window's.
   */
  let { offer = $bindable(null), onPaid, onWallet, onOrders, available, onRefresh }: { offer: any; available: boolean; onRefresh: () => Promise<void>; onPaid: () => void; onWallet: (orderId?: string) => void; onOrders: () => void } = $props()

  let alive = true
  let payRun = 0
  let quotedFor = ''
  let loadingPanda = $state(false), loadingAssets = $state(false)
  let assetError = $state(''), pandaError = $state(''), errorKey = $state('')
  let openingApproval = $state(false), shortBalance = $state(false)
  let method = $state('Cash')
  let assets = $state<any[]>([])
  let ledger = $state('')
  let panda = $state<any>(null)
  let neuron = $state('')
  let error = $state('')
  let busy = $state(false)
  let checkOrders = $state(false)
  /** The quote this click fixed, for the amount a short balance needs. */
  let quoted = $state<any>(null)

  const pandaAllowed = $derived<boolean>(offer?.methods?.includes('Panda') ?? false)
  /** What this plan needs staked at the current rate; none without a rate. */
  const required = $derived<string | null>(panda?.plans?.find((p: any) => p.plan_id === offer?.plan)?.required_stake_e8s ?? null)
  const enough = (n: any) => required != null && BigInt(n.stake_e8s) >= BigInt(required)
  /** The term end the neuron's lock must cover; the quote fixes it exactly. */
  const lockUntil = $derived<number | null>(offer?.ends_at_ms ?? null)
  const covered = (n: any) => lockUntil == null || neuronUnlockAt(n) >= Number(lockUntil)

  // Each offer opens on cash; the assets are read once.
  $effect(() => {
    const opened = offer
    untrack(() => {
      payRun += 1
      busy = openingApproval = shortBalance = checkOrders = false
      if (!opened) return
      method = 'Cash'
      neuron = ''
      error = ''
      quoted = null
      if (!assets.length) void readAssets()
    })
  })

  $effect(() => {
    if (offer && method === 'Panda') untrack(() => void readPanda())
  })

  const readAssets = single(async () => {
    if (!alive) return
    loadingAssets = true
    try {
      const next = await call('plan/assets')
      if (!alive) return
      assets = next
      if (!assets.some(a => a.ledger === ledger)) ledger = assets[0]?.ledger ?? ''
      assetError = ''
    } catch (e) { if (alive) assetError = errorText(e) }
    finally { if (alive) loadingAssets = false }
  })

  const readPanda = single(async () => {
    if (!alive) return
    loadingPanda = true
    try {
      const next = await call('plan/panda')
      if (alive) {
        panda = next
        pandaError = ''
        if (neuron && !canUseNeuron(next.neurons?.find((n: any) => n.id === neuron))) neuron = ''
      }
    } catch (e) {
      if (alive) pandaError = errorText(e)
    } finally {
      loadingPanda = false
    }
  })

  onDestroy(() => { alive = false })
  const selectionKey = () => JSON.stringify([offer?.sku, method, method === 'Cash' ? ledger : neuron])

  function canUseNeuron(n: any) {
    return !!n && !panda?.paused && !panda?.neurons_error && enough(n) && covered(n)
  }
  const canPay = $derived(available && (method === 'Cash'
    ? !!ledger && !assetError && !loadingAssets && assets.some(a => a.ledger === ledger)
    : !pandaError && !loadingPanda && canUseNeuron(panda?.neurons?.find((n: any) => n.id === neuron))))
  const selection = $derived(selectionKey())
  async function pay() {
    if (busy || !offer || !canPay || (checkOrders && errorKey === selection)) return
    const opened = offer
    const run = ++payRun
    const key = selectionKey()
    const current = () => alive && run === payRun && offer === opened && available && selectionKey() === key
    busy = true
    errorKey = key
    error = ''
    shortBalance = checkOrders = false
    try {
      const cash = method === 'Cash'
      if (quotedFor !== key || !reusableQuote(quoted)) {
        quoted = null
        quotedFor = ''
      }
      const job: any = quoted ?? await call('plan/quote', { sku: opened.sku, method: cash ? 'cash' : 'panda', selection: cash ? ledger : neuron })
      if (!current()) return
      quoted = job
      quotedFor = key
      if (!cash) {
        // The quote fixes the term end and stake; do not approve a neuron the claim will reject.
        const chosen = panda?.neurons?.find((n: any) => n.id === neuron)
        const stake = job?.terms?.required_stake_e8s
        const end = Number(job?.expires_at_ms ?? 0)
        if (chosen && stake != null && BigInt(chosen.stake_e8s) < BigInt(stake)) {
          error = t('stakeTooLow')
          return
        }
        if (chosen && end > 0 && neuronUnlockAt(chosen) < end) {
          error = t('unlockTooEarly')
          return
        }
      }
      openingApproval = true
      if (!await reviewApproval('checkout:' + job.id, () => call('plan/checkout', { id: job.id }), current)) return
      offer = null
      onPaid()
    } catch (e) {
      if (!current()) return
      if (['EXPIRED', 'POLICY_STALE', 'VERSION_CONFLICT', 'FEE_BLOCKED', 'FORBIDDEN'].includes(code(e))) {
        quoted = null
        quotedFor = ''
      }
      const total = quoted?.terms?.method === 'Cash' ? cashTotal(quoted.terms) : null
      shortBalance = code(e) === 'INSUFFICIENT_FUNDS'
      checkOrders = ['EXECUTION_UNKNOWN', 'PENDING', 'RESULT_EXPIRED'].includes(code(e))
      error = checkOrders ? t('operationUnknown') : shortBalance && total !== null
        ? t('insufficientFunds', { asset: quoted.terms.asset, amount: units(total.toString(), quoted.terms.decimals) })
        : errorText(e)
    } finally {
      if (run === payRun) busy = openingApproval = false
    }
  }

  const price = (cents: number) => new Intl.NumberFormat(locale(), { style: 'currency', currency: 'USD' }).format(cents / 100)
  const title = $derived(
    !offer ? '' : offer.kind === 'storage' ? t('storagePackOf', { size: size(offer.bytes ?? 0) }) : offer.action === 'upgrade' ? t('upgradeTo', { plan: offer.plan }) : String(offer.plan)
  )
  const term = $derived(
    !offer
      ? ''
      : offer.kind === 'storage'
        ? t('storagePackTerm', { time: when(offer.until) })
        : offer.action === 'upgrade'
          ? t('upgradeTerm')
          : offer.action === 'subscribe'
            ? t('termStartsNow')
            : t('termStartsAt', { time: when(offer.starts_at_ms) })
  )
</script>

<Dialog bind:open={() => offer !== null, (open) => (offer = open ? offer : null)} dismissible={!openingApproval} {title} description={term} icon="coins" testid="checkout">
  {#if offer}
    <div class="grid gap-4">
      {#if !available}<Notice tone="warn" testid="checkout-offer-changed">{t('membershipRefreshHint')}
        <button class={buttonClass('subtle', 'xs')} onclick={() => { offer = null; void onRefresh() }}>{t('refresh')}</button>
      </Notice>{/if}
      {#if method === 'Cash'}
        {#if offer.action === 'upgrade' || offer.kind === 'storage'}
          <p class="text-title-3 font-semibold" data-testid="checkout-prorated">{t(offer.action === 'upgrade' ? 'upgradeProrated' : 'planStorageProrated')}</p>
          <p class="text-callout text-muted">{t('planAnnualReference', { price: price(offer.price_cents) })} · {t('amountInApproval')}</p>
        {:else}<p class="text-body">
          <b class="text-title-2 font-semibold tabular-nums">{price(offer.price_cents)}</b>
          <span class="text-muted">/ {t('year')} · {t('amountInApproval')}</span>
        </p>{/if}
      {/if}
      {#if pandaAllowed}
        <Segmented bind:value={method} disabled={busy} label={t('payWith')} options={[{ value: 'Cash', label: t('methodCash') }, { value: 'Panda', label: t('methodPanda') }]} testid="checkout-method" />
      {/if}
      {#if method === 'Cash'}
        <Field label={t('wallet.asset')}>
          <Select bind:value={ledger} label={t('wallet.asset')} disabled={busy || loadingAssets} testid="plan-asset" placeholder={t('choose')} options={assets.map((a) => ({ value: a.ledger, label: a.asset }))} />
        </Field>
        {#if loadingAssets}<p role="status" class="text-callout text-muted">{t('loading')}</p>
        {:else if assetError || !assets.length}<Notice tone="warn" testid="checkout-assets-error">{assetError || t('planNoAssets')}
          <button class={buttonClass('subtle', 'xs')} onclick={readAssets}>{t('retry')}</button></Notice>{/if}
      {:else if pandaError || !panda}
        {#if loadingPanda}<p role="status" class="text-callout text-muted">{t('loading')}</p>
        {:else if pandaError}<Notice tone="warn" testid="checkout-panda-error">{pandaError}
          <button class={buttonClass('subtle', 'xs')} onclick={readPanda}>{t('retry')}</button></Notice>{/if}
      {:else}
        <div class="grid gap-3" data-testid="checkout-panda">
          {#if panda.paused}<Notice tone="warn" icon="warning">{t('pandaPaused')}</Notice>{/if}
          <Notice tone="info" icon="info">
            <p>{t('pandaHotkeyHint')}</p>
            <p class="mt-1"><Id id={panda.payer} full wrap testid="checkout-payer" /></p>
            {#snippet action()}<CopyButton class={buttonClass('subtle', 'xs')} value={() => panda.payer} label={t('copy')} />{/snippet}
          </Notice>
          <dl>
            <Fact label={t('stake')} testid="checkout-stake">{required != null ? `${units(required, 8)} PANDA` : t('pandaNoRate')}</Fact>
            {#if lockUntil != null}
              <Fact label={t('lockRequirement')} testid="checkout-lock">{t('lockNotBefore', { time: when(lockUntil) })}</Fact>
            {/if}
          </dl>
          <p class="text-callout text-muted">{t('pandaTermsHint')}</p>
          {#if panda.neurons_error}
            <Notice tone="danger">{t('error')}: {panda.neurons_error}</Notice>
          {:else if !panda.neurons?.length}
            <p class="text-callout text-muted" data-testid="checkout-no-neurons">{t('noNeurons')}</p>
          {:else}
            <RadioGroup.Root bind:value={neuron} aria-label={t('neuron')}>
              <ul class="divide-y divide-line">
                {#each panda.neurons as n (n.id)}
                  <li class={cn('px-3.5 py-2.5', neuron === n.id && 'bg-pale/60')}>
                    <Radio value={n.id} disabled={busy || loadingPanda || !canUseNeuron(n)} testid="checkout-neuron">
                      <span class="flex flex-wrap items-center gap-1.5 text-callout">
                        <span class="font-mono">{n.id.slice(0, 12)}…</span>
                        <span class="tabular-nums">{units(n.stake_e8s, 8)} PANDA</span>
                        <span class="text-faint">{t('lockedUntil', { time: when(neuronUnlockAt(n)) })}</span>
                        {#if n.dissolving}<Pill tone="warn">{t('dissolving')}</Pill>{/if}
                        {#if required != null && !enough(n)}<Pill tone="warn">{t('stakeTooLow')}</Pill>{/if}
                        {#if !covered(n)}<Pill tone="warn">{t('unlockTooEarly')}</Pill>{/if}
                      </span>
                    </Radio>
                  </li>
                {/each}
              </ul>
            </RadioGroup.Root>
          {/if}
          <div>
            <button class={buttonClass('ghost', 'xs')} disabled={busy || loadingPanda} onclick={readPanda}><Icon name="sync-now" />{t('refresh')}</button>
          </div>
        </div>
      {/if}
      {#if error && errorKey === selection}<Notice tone={checkOrders ? 'warn' : 'danger'} testid="checkout-error">{error}
        {#if shortBalance || checkOrders}<button class={buttonClass('secondary', 'sm')} onclick={() => {
          offer = null
          if (checkOrders) onOrders(); else onWallet(quoted?.id)
        }}>{t(checkOrders ? 'orders' : 'openWallet')}</button>{/if}
      </Notice>{/if}
    </div>
  {/if}
  {#snippet footer()}
    <button class={buttonClass('secondary')} disabled={openingApproval} onclick={() => (offer = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !canPay || (checkOrders && errorKey === selection)} aria-busy={busy} onclick={pay} data-testid="checkout-pay">{busy ? t('loading') : t('continueToApprove')}</button>
  {/snippet}
</Dialog>
