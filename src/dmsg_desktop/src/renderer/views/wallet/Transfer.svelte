<script lang="ts">
  import { onMount } from 'svelte'
  import { walletError as errorText } from './errors'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass } from '../../components/ui'
  import { call } from '../../rpc'
  import { t } from '../../i18n/index.svelte'
  import Intent from './Intent.svelte'
  import { assetChain, assetIdentity, assetKey, decimal, feeAsset, toAtomic, type Asset, type Wallet } from './model'
  import { w } from './text'
  let {
    wallet,
    metadata,
    book = [],
    draft = $bindable(),
    onDone
  }: { wallet: Wallet; metadata: Asset[]; book?: any[]; draft: any; onDone: (p: any) => void } = $props()
  let alive = true
  onMount(() => () => {
    alive = false
  })
  let busy = $state(false),
    error = $state(''),
    preview = $state<any>(null),
    lastAsset = ''
  const address = $derived(wallet.addresses.find((a) => String(a.id) === draft.from))
  const available = $derived(metadata.filter((a) => assetChain(a) === address?.chain))
  const asset = $derived(available.find((a) => assetKey(a) === draft.asset))
  const fee = $derived(asset ? feeAsset(asset) : undefined)
  const fingerprint = $derived(JSON.stringify([draft, asset?.decimals, fee?.decimals, wallet.policy.version]))
  const reviewed = $derived(preview?.fingerprint === fingerprint)
  $effect(() => {
    const key = `${draft.from}/${draft.asset}/${asset?.decimals ?? "atomic"}`
    if (lastAsset && key !== lastAsset) {
      draft.maxFee = ''
      if (!alive) return
      draft.outputs = draft.outputs.map((o: any) => ({ ...o, amount: '' }))
      preview = null
    }
    lastAsset = key
  })
  $effect(() => {
    if (!available.some((a) => assetKey(a) === draft.asset)) draft.asset = available[0] ? assetKey(available[0]) : ''
  })
  const recipients = $derived(book.filter((b) => b.chain === address?.chain))
  function payload() {
    if (!asset) throw new Error(w('noAssets'))
    return {
      from: Number(draft.from),
      asset: draft.asset,
      outputs: draft.outputs.map((o: any) => {
        const amount = toAtomic(o.amount.trim(), asset.decimals ?? 0)
        if (BigInt(amount) <= 0n || !o.to.trim()) throw new Error(w('invalidAmount'))
        return { to: o.to.trim(), amount }
      }),
      max_fee: draft.maxFee ? toAtomic(draft.maxFee.trim(), fee?.decimals ?? 0) : null,
      memo: draft.memo || null
    }
  }
  async function estimate() {
    busy = true
    error = ''
    preview = null
    const form = fingerprint,
      target = wallet.wallet
    try {
      const result: any = await call('wallet/preview', { wallet: target, draft: payload() })
      if (!alive || form !== fingerprint || target !== wallet.wallet) return
      if (!draft.maxFee) draft.maxFee = decimal(result.max_fee, result.fee_decimals)
      preview = { ...result, fingerprint: JSON.stringify([draft, asset?.decimals, fee?.decimals, wallet.policy.version]) }
    } catch (e) {
      error = e instanceof Error && ['amount', 'precision', 'overflow'].includes(e.message) ? w('invalidAmount') : errorText(e)
    } finally {
      busy = false
    }
  }
  async function submit() {
    if (!reviewed || preview.blockers.length) return
    busy = true
    error = ''
    try {
      const p = await call('wallet/transfer', { wallet: wallet.wallet, draft: payload(), policy_version: preview.policy_version })
      if (!alive) return
      draft.outputs = [{ to: '', amount: '' }]
      preview = null
      onDone(p)
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="grid gap-4" data-testid="wallet-transfer">
  <div class="grid gap-3 sm:grid-cols-2">
    <Field label={w('from')}
      ><Select
        bind:value={draft.from}
        label={w('from')}
        options={wallet.addresses
          .filter((a) => a.verified)
          .map((a) => ({ value: String(a.id), label: `#${a.id} · ${a.label || a.chain}` }))}
        testid="wallet-from"
      /></Field
    >
    <Field label={w('asset')}
      ><Select
        bind:value={draft.asset}
        label={w('asset')}
        options={available.map((a) => ({ value: assetKey(a), label: assetIdentity(a) }))}
        testid="wallet-asset"
      /></Field
    >
  </div>
  {#if !available.length}<Notice>{w('noAssets')}</Notice>{/if}
  {#each draft.outputs as output, i (i)}
    <div class="border-line bg-surface-2 grid items-end gap-3 rounded-xl border p-3 sm:grid-cols-[minmax(0,1fr)_10rem_auto]">
      <Field label={`${w('recipient')} ${i + 1}`}>
        <input bind:value={output.to} autocomplete="off" spellcheck="false" dir="ltr" data-testid="wallet-to" />
        {#if recipients.length}<Select
            value=""
            label={w('book')}
            placeholder={w('book')}
            options={recipients.map((b) => ({ value: b.destination, label: `${b.label} · ${b.destination}` }))}
            onChange={(value) => (output.to = value)}
          />{/if}
      </Field>
      <Field label={`${w('amount')} · ${asset?.symbol || asset?.asset || ''}${asset?.decimals == null ? ` (${w('atomic')})` : ''}`}
        ><input bind:value={output.amount} inputmode="decimal" dir="ltr" data-testid="wallet-amount" /></Field
      >
      <button
        class={buttonClass('ghost', 'sm')}
        disabled={busy || draft.outputs.length === 1}
        onclick={() => (draft.outputs = draft.outputs.filter((_: any, n: number) => n !== i))}
        aria-label={`${w('remove')} ${i + 1}`}>{w('remove')}</button
      >
    </div>
  {/each}
  {#if address?.chain !== 'Evm'}<button
      class={buttonClass('secondary', 'sm', 'justify-self-start')}
      disabled={busy || draft.outputs.length >= 64}
      onclick={() => (draft.outputs = [...draft.outputs, { to: '', amount: '' }])}>{w('addRecipient')}</button
    >{/if}
  <div class="grid gap-3 sm:grid-cols-2">
    <Field label={`${w('fee')} · ${fee?.symbol || fee?.asset || w('atomic')}`}
      ><input bind:value={draft.maxFee} inputmode="decimal" placeholder={w('estimated')} dir="ltr" data-testid="wallet-fee" /></Field
    >
    {#if address?.chain === 'Icp' && draft.outputs.length === 1}<Field label={w('memo')}
        ><input bind:value={draft.memo} pattern="[a-fA-F0-9]*" maxlength="64" dir="ltr" /></Field
      >{/if}
  </div>
  {#if error}<Notice tone="danger" testid="wallet-transfer-error">{error}</Notice>{/if}
  <button
    class={buttonClass('secondary', 'sm', 'justify-self-start')}
    disabled={busy || !asset}
    onclick={estimate}
    data-testid="wallet-estimate">{w('estimate')}</button
  >
  {#if reviewed}
    <section class="border-forest/30 bg-pale/30 grid gap-3 rounded-xl border p-4" data-testid="wallet-transfer-preview">
      <Intent intent={preview.intent} {metadata} />
      <p class="text-[12px]">{w('estimated')}: {decimal(preview.estimated_fee, preview.fee_decimals)} {preview.fee_asset}</p>
      {#if BigInt(preview.additional_reserve || '0') > 0n}<p class="text-[12px]"
          >{w('reserve')}: {decimal(preview.additional_reserve, preview.fee_decimals)} {preview.fee_asset}</p
        >{/if}
      <p class="text-[12px]">{w('approvals')}: {preview.requirement.approvals} · {w('delay_ms')}: {preview.requirement.delay_ms}</p>
      <p class="text-muted text-[12px]">{w('previewHint')}</p>
      {#each preview.blockers as blocker}<Notice tone="danger">{w(blocker)}</Notice>{/each}
      <button
        class={buttonClass('primary', 'sm', 'justify-self-end')}
        disabled={busy || preview.blockers.length > 0}
        onclick={submit}
        data-testid="wallet-send">{w('submit')}</button
      >
    </section>
  {/if}
</div>
