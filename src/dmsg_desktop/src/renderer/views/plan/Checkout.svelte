<script lang="ts">
  import { RadioGroup } from 'bits-ui'
  import { untrack } from 'svelte'
  import { copyText } from '../../components/clipboard'
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
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, errorText, size, units, when } from '../../rpc'

  /**
   * Paying for an offer: a plan card's subscription, renewal, switch or
   * upgrade, or a storage pack. Cash picks the asset; PANDA picks a neuron
   * the payer may act for, whose stake and lock cover the term. The quote
   * fixes the terms here; approving them is the approval window's.
   */
  let { offer = $bindable(null), onPaid }: { offer: any; onPaid: () => void } = $props()

  let method = $state('Cash')
  let assets = $state<any[]>([])
  let ledger = $state('')
  let panda = $state<any>(null)
  let neuron = $state('')
  let error = $state('')
  let busy = $state(false)

  const pandaAllowed = $derived<boolean>(offer?.methods?.includes('Panda') ?? false)
  /** What this plan needs staked at the current rate; none without a rate. */
  const required = $derived<string | null>(panda?.plans?.find((p: any) => p.plan_id === offer?.plan)?.required_stake_e8s ?? null)
  const enough = (n: any) => required != null && BigInt(n.stake_e8s) >= BigInt(required)
  /** The term end the neuron's lock must cover; the quote fixes it exactly. */
  const lockUntil = $derived<number | null>(offer?.ends_at_ms ?? null)
  const covered = (n: any) => lockUntil == null || Number(n.unlock_at_ms) >= Number(lockUntil)

  // Each offer opens on cash; the assets are read once.
  $effect(() => {
    if (!offer) return
    untrack(() => {
      method = 'Cash'
      neuron = ''
      error = ''
      if (!assets.length) void readAssets()
    })
  })

  $effect(() => {
    if (offer && method === 'Panda' && !panda) untrack(() => void readPanda())
  })

  async function readAssets() {
    try {
      assets = await call('plan/assets')
      if (!ledger && assets.length) ledger = assets[0].ledger
    } catch (e) {
      error = errorText(e)
    }
  }

  /** The PANDA terms and the payer's neurons, read again once the payer is a hotkey. */
  async function readPanda() {
    busy = true
    try {
      panda = await call('plan/panda')
      error = ''
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  async function pay() {
    busy = true
    error = ''
    try {
      const cash = method === 'Cash'
      const job: any = await call('plan/quote', { sku: offer.sku, method: cash ? 'cash' : 'panda', selection: cash ? ledger : neuron })
      if (!cash) {
        // The quote fixes the term end and stake; do not approve a neuron the claim will reject.
        const chosen = panda?.neurons?.find((n: any) => n.id === neuron)
        const stake = job?.terms?.required_stake_e8s
        const end = Number(job?.expires_at_ms ?? 0)
        if (chosen && stake != null && BigInt(chosen.stake_e8s) < BigInt(stake)) {
          error = t('stakeTooLow')
          return
        }
        if (chosen && end > 0 && Number(chosen.unlock_at_ms) < end) {
          error = t('unlockTooEarly')
          return
        }
      }
      const request: any = await call('plan/checkout', { id: job.id })
      await bridge(() => window.dmsg.openApproval(request.id))
      offer = null
      onPaid()
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  const price = (cents: number) => `$${(cents / 100).toFixed(2)}`
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

<Dialog bind:open={() => offer !== null, (open) => (offer = open ? offer : null)} {title} description={term} icon="coins" testid="checkout">
  {#if offer}
    <div class="grid gap-4">
      <p class="text-[13px]">
        <b class="text-[20px] font-semibold tabular-nums">{price(offer.price_cents)}</b>
        <span class="text-faint">/ {t('year')} · {t('amountInApproval')}</span>
      </p>
      {#if pandaAllowed}
        <Segmented bind:value={method} label={t('payWith')} options={[{ value: 'Cash', label: t('methodCash') }, { value: 'Panda', label: t('methodPanda') }]} testid="checkout-method" />
      {/if}
      {#if method === 'Cash'}
        <Field label={t('payWith')}>
          <Select bind:value={ledger} testid="plan-asset" placeholder={t('choose')} options={assets.map((a) => ({ value: a.ledger, label: a.asset }))} />
        </Field>
      {:else if panda}
        <div class="grid gap-3" data-testid="checkout-panda">
          {#if panda.paused}<Notice tone="warn" icon="warning">{t('pandaPaused')}</Notice>{/if}
          <Notice tone="info" icon="info">
            <p>{t('pandaHotkeyHint')}</p>
            <p class="mt-1"><Id id={panda.payer} full wrap testid="checkout-payer" /></p>
            {#snippet action()}<button class={buttonClass('secondary', 'xs')} onclick={() => copyText(panda.payer)}><Icon name="copy" />{t('copy')}</button>{/snippet}
          </Notice>
          <dl class="divide-y divide-line">
            <Fact label={t('stake')} testid="checkout-stake">{required != null ? `${units(required, 8)} PANDA` : t('pandaNoRate')}</Fact>
            {#if lockUntil != null}
              <Fact label={t('lockRequirement')} testid="checkout-lock">{t('lockNotBefore', { time: when(lockUntil) })}</Fact>
            {/if}
          </dl>
          <p class="text-[12px] text-muted">{t('pandaTermsHint')}</p>
          {#if panda.neurons_error}
            <Notice tone="danger">{t('error')}: {panda.neurons_error}</Notice>
          {:else if !panda.neurons?.length}
            <p class="text-[12.5px] text-faint" data-testid="checkout-no-neurons">{t('noNeurons')}</p>
          {:else}
            <RadioGroup.Root bind:value={neuron} aria-label={t('neuron')}>
              <ul class="divide-y divide-line rounded-xl border border-line">
                {#each panda.neurons as n (n.id)}
                  <li class={cn('px-3.5 py-2.5', neuron === n.id && 'bg-pale/60')}>
                    <Radio value={n.id} disabled={!enough(n) || !covered(n)} testid="checkout-neuron">
                      <span class="flex flex-wrap items-center gap-1.5 text-[12.5px]">
                        <span class="font-mono">{n.id.slice(0, 12)}…</span>
                        <span class="tabular-nums">{units(n.stake_e8s, 8)} PANDA</span>
                        <span class="text-faint">{t('lockedUntil', { time: when(n.unlock_at_ms) })}</span>
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
            <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={readPanda}><Icon name="sync-now" />{t('refresh')}</button>
          </div>
        </div>
      {/if}
      {#if error}<Notice tone="danger" testid="checkout-error">{error}</Notice>{/if}
    </div>
  {/if}
  {#snippet footer()}
    <button class={buttonClass('secondary')} onclick={() => (offer = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || (method === 'Cash' ? !ledger : !neuron)} onclick={pay} data-testid="checkout-pay">{t('continueToApprove')}</button>
  {/snippet}
</Dialog>
