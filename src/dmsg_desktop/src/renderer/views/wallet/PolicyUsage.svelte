<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Field from '../../components/page/Field.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass } from '../../components/ui'
  import { call, single, when } from '../../rpc'
  import { locale } from '../../i18n/index.svelte'
  import {
    assetKey,
    assetIdentity,
    assetFromKey,
    amountText,
    durationText,
    policyExample,
    toAtomic,
    type Asset,
    type Wallet
  } from './model'
  import { w } from './text'
  let { wallet, policy = wallet.policy, metadata = [] }: { wallet: Wallet; policy?: any; metadata?: Asset[] } = $props()
  let usage = $state<any>(null),
    error = $state(''),
    key = $state(''),
    amount = $state('1'),
    alive = true
  const options = $derived(
    policy.spendable_assets.map((a: any) => ({
      value: typeof a === 'string' ? a : assetKey(a),
      label: assetIdentity(typeof a === 'string' ? assetFromKey(a, metadata) : a)
    }))
  )
  $effect(() => {
    if (!options.some((o: any) => o.value === key)) key = options[0]?.value ?? ''
  })
  const asset = $derived(assetFromKey(key, metadata))
  const row = $derived(usage?.assets.find((r: any) => assetKey(r.asset) === key))
  const example = $derived.by(() => {
    if (!usage || !key) return null
    try {
      return policyExample(
        policy,
        key,
        toAtomic(amount, asset.decimals ?? 0),
        row?.reserved ?? '0',
        row?.executed ?? '0',
        wallet.members.filter((m) => m.status === 'Active' && ['Admin', 'Approver'].includes(m.role)).length
      )
    } catch {
      return null
    }
  })
  const load = single(async () => {
    const head = wallet.audit_head
    error = ''
    try {
      const result = await call('wallet/policy_usage', { wallet: wallet.wallet })
      if (alive && head === wallet.audit_head) usage = result
    } catch {
      if (alive) error = w('policyUsageUnavailable')
    }
  })
  $effect(() => {
    wallet.audit_head
    untrack(() => void load())
  })
  onMount(() => () => {
    alive = false
  })
</script>

<section class="border-line grid gap-3 rounded-xl border p-4" data-testid="wallet-policy-usage">
  <h3 class="text-[14px] font-semibold">{w('policyExample')}</h3>
  {#if error}<Notice>{error}</Notice>{/if}
  {#if options.length}<div class="grid gap-3 sm:grid-cols-2">
      <Field label={w('asset')}><Select bind:value={key} {options} /></Field>
      <Field label={`${w('amount')} · ${asset.symbol || asset.asset}${asset.decimals == null ? ` (${w('atomic')})` : ''}`}
        ><input bind:value={amount} inputmode="decimal" dir="ltr" aria-invalid={!!usage && !example} /></Field
      >
    </div>
    {#if usage}
      <p class="text-[13px]">{w('reservedAmount')}: {amountText(row?.reserved ?? '0', asset, w('atomic'))}</p>
      <p class="text-[13px]">{w('executedWindow')}: {amountText(row?.executed ?? '0', asset, w('atomic'))}</p>
      {#if example}<p class="text-[13px] font-medium" aria-live="polite"
          >{example.approvals}
          {w('approvals')} · {durationText(example.delay, locale())}{example.all ? ` · ${w('allVoters')}` : ''}{example.book
            ? ` · ${w('book_only')}`
            : ''}</p
        >{/if}
    {/if}
  {:else}<p class="text-muted text-[12px]">{w('noAssets')}</p>{/if}
  <div class="flex flex-wrap items-center justify-between gap-2"
    ><p class="text-muted text-[12px]">{usage ? `${w('readAt')}: ${when(usage.read_at)}` : ''}</p><button
      class={buttonClass('ghost', 'sm')}
      onclick={load}>{w('retry')}</button
    ></div
  >
  <p class="text-muted text-[12px]">{w('exampleHint')}</p>
</section>
