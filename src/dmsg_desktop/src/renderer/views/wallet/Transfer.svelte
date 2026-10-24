<script lang="ts">
  import { onMount } from 'svelte'
  import Collapsible from '../../components/Collapsible.svelte'
  import { walletError as errorText } from './errors'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass } from '../../components/ui'
  import { call } from '../../rpc'
  import { t, locale } from '../../i18n/index.svelte'
  import Intent from './Intent.svelte'
  import {
    assetChain,
    assetIdentity,
    assetKey,
    amountText,
    networkName,
    durationText,
    batchLimit,
    decimal,
    feeAsset,
    toAtomic,
    type Asset,
    type Wallet
  } from './model'
  import { w } from './text'
  let {
    wallet,
    metadata,
    book = [],
    balances,
    onPrepareNonce,
    draft = $bindable(),
    busy = $bindable(false),
    onDone
  }: {
    wallet: Wallet
    metadata: Asset[]
    book?: any[]
    balances?: any
    onPrepareNonce: (address: any) => void
    draft: any
    busy?: boolean
    onDone: (p: any) => void
  } = $props()
  const noteHintId = $props.id()
  let alive = true
  onMount(() => () => {
    alive = false
    busy = false
  })
  let attempted = $state(false)
  let error = $state(''),
    preview = $state<any>(null),
    lastAsset = ''
  const address = $derived(wallet.addresses.find((a) => String(a.id) === draft.from))
  const available = $derived(metadata.filter((a) => assetChain(a) === address?.chain))
  const asset = $derived(available.find((a) => assetKey(a) === draft.asset))
  const fee = $derived(asset ? feeAsset(asset) : undefined)
  const fingerprint = $derived(JSON.stringify([draft, asset?.decimals, fee?.decimals, wallet.policy.version]))
  const reviewed = $derived(preview?.fingerprint === fingerprint)
  $effect(() => {
    const key = `${draft.from}/${draft.asset}/${asset?.decimals ?? 'atomic'}`
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
  const sourceBalance = $derived(
    balances?.addresses?.find((r: any) => r.address === address?.id)?.balances?.find((b: any) => assetKey(b.metadata ?? b) === draft.asset)
  )
  const rowErrors = $derived(
    draft.outputs.map((o: any) => {
      try {
        if (!asset || !o.to.trim() || /\s/.test(o.to.trim()) || BigInt(toAtomic(o.amount.trim(), asset.decimals ?? 0)) <= 0n) return true
        if (address?.chain === 'Evm' && !/^0x[0-9a-fA-F]{40}$/.test(o.to.trim())) return true
        if (address?.chain === 'Solana' && !/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(o.to.trim())) return true
        return false
      } catch {
        return true
      }
    })
  )
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
    attempted = true
    if (rowErrors.some(Boolean) || draft.outputs.length > batchLimit(address?.chain)) return
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
      if (alive) error = e instanceof Error && ['amount', 'precision', 'overflow'].includes(e.message) ? w('invalidAmount') : errorText(e)
    } finally {
      if (alive) busy = false
    }
  }
  async function submit() {
    if (!reviewed || preview.blockers.length) return
    busy = true
    error = ''
    try {
      const note = draft.note?.trim() ?? ''
      const p: any = await call('wallet/transfer', { wallet: wallet.wallet, draft: payload(), policy_version: preview.policy_version })
      if (!alive) return
      if (note) {
        try {
          await call('wallet/notes', { wallet: wallet.wallet, proposal: p.id, note })
          draft.note = ''
        } catch {
          p.note_error = true
        }
      }
      draft.outputs = [{ to: '', amount: '' }]
      preview = null
      onDone(p)
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      if (alive) busy = false
    }
  }
</script>

<div class="grid gap-4" data-testid="wallet-transfer" aria-busy={busy}>
  {#if asset}<p class="text-[13px] font-medium">{w('network')}: {networkName(asset)}</p>{/if}
  {#if !reviewed}
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
    <p class="text-[13px]" role="status"
      >{w('sourceBalance')}: {sourceBalance?.balance?.Ok != null && asset
        ? amountText(sourceBalance.balance.Ok, asset, w('atomic'))
        : w('noBalance')}</p
    >
    {#if address?.chain === 'Solana' && !address.sol_nonce}<Notice
        >{w('noncePurposeHint')}<button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => onPrepareNonce(address)}
          >{w('noncePurpose')}</button
        ></Notice
      >{/if}
    {#if !available.length}<Notice>{w('noAssets')}</Notice>{/if}
    {#each draft.outputs as output, i (i)}
      <div class="grid items-end gap-3 sm:grid-cols-[minmax(0,1fr)_10rem_auto]">
        <Field label={`${w('recipient')} ${i + 1}`}>
          <input
            aria-invalid={attempted && rowErrors[i]}
            bind:value={output.to}
            autocomplete="off"
            spellcheck="false"
            dir="ltr"
            data-testid="wallet-to"
          />
          {#if recipients.length}<Select
              value=""
              label={w('book')}
              placeholder={w('book')}
              options={recipients.map((b) => ({ value: b.destination, label: `${b.label} · ${b.destination}` }))}
              onChange={(value) => (output.to = value)}
            />{/if}
        </Field>
        <Field label={`${w('amount')} · ${asset?.symbol || asset?.asset || ''}${asset?.decimals == null ? ` (${w('atomic')})` : ''}`}
          ><input
            aria-invalid={attempted && rowErrors[i]}
            bind:value={output.amount}
            inputmode="decimal"
            dir="ltr"
            data-testid="wallet-amount"
          /></Field
        >
        <button
          class={buttonClass('ghost', 'sm')}
          disabled={busy || draft.outputs.length === 1}
          onclick={() => (draft.outputs = draft.outputs.filter((_: any, n: number) => n !== i))}
          aria-label={`${w('remove')} ${i + 1}`}>{w('remove')}</button
        >
      </div>
      {#if attempted && rowErrors[i]}<p class="text-danger text-[12px]" role="alert">{w('recipient')} {i + 1}: {w('invalidRecipient')}</p
        >{/if}
    {/each}
    <p class="text-muted text-[12px]">{draft.outputs.length} / {batchLimit(address?.chain)} · {w('recipientLimit')}</p>
    {#if address?.chain !== 'Evm'}<button
        class={buttonClass('secondary', 'sm', 'justify-self-start')}
        disabled={busy || draft.outputs.length >= batchLimit(address?.chain)}
        onclick={() => (draft.outputs = [...draft.outputs, { to: '', amount: '' }])}>{w('addRecipient')}</button
      >{/if}
    <Field label={w('privateNote')} hint={w('privateNoteHint')} hintId={noteHintId}
      ><textarea aria-label={w('privateNote')} aria-describedby={noteHintId} bind:value={draft.note} maxlength="500" rows="2"></textarea></Field
    >
    <Collapsible
      >{#snippet title()}<span class="text-[13px]">{t('advanced')}</span>{/snippet}
      <Field label={`${w('fee')} · ${fee?.symbol || fee?.asset || w('atomic')}`}
        ><input bind:value={draft.maxFee} inputmode="decimal" placeholder={w('estimated')} dir="ltr" data-testid="wallet-fee" /></Field
      >
      {#if address?.chain === 'Icp' && draft.outputs.length === 1}
        <Field label={w('memo')}><input bind:value={draft.memo} pattern="[a-fA-F0-9]*" maxlength="64" dir="ltr" /></Field>
      {/if}
    </Collapsible>
    {#if error}<Notice tone="danger" testid="wallet-transfer-error">{error}</Notice>{/if}
    <button
      class={buttonClass('secondary', 'sm', 'justify-self-start')}
      disabled={busy || !asset}
      onclick={estimate}
      data-testid="wallet-estimate">{w('reviewTransfer')}</button
    >
  {/if}
  {#if reviewed}
    <section class="border-forest/30 bg-pale/30 grid gap-3 rounded-xl border p-4" data-testid="wallet-transfer-preview">
      <h3 class="text-[15px] font-semibold">{w('reviewTransfer')} · {wallet.name}</h3>
      <Intent intent={preview.intent} {metadata} before={wallet} />
      <p class="text-[13px]"
        >{w('sourceBalance')}: {asset ? amountText(preview.asset_balance ?? preview.native_balance, asset, w('atomic')) : '—'}</p
      >
      <p class="text-[13px] font-semibold"
        >{w('maximumDebit')}: {preview.native_asset && asset
          ? amountText(
              (BigInt(preview.total_amount ?? '0') + BigInt(preview.max_fee) + BigInt(preview.additional_reserve ?? '0')).toString(),
              asset,
              w('atomic')
            )
          : `${asset ? amountText(preview.total_amount ?? '0', asset, w('atomic')) : ''} + ${decimal((BigInt(preview.max_fee) + BigInt(preview.additional_reserve ?? '0')).toString(), preview.fee_decimals)} ${preview.fee_asset}`}</p
      >
      {#if draft.note}<p class="text-[12px]">{w('privateNote')}: {draft.note}</p><p class="text-muted text-[12px]">{w('privateNoteHint')}</p
        >{/if}
      <p class="text-[12px]">{w('estimated')}: {decimal(preview.estimated_fee, preview.fee_decimals)} {preview.fee_asset}</p>
      {#if BigInt(preview.additional_reserve || '0') > 0n}<p class="text-[12px]"
          >{w('reserve')}: {decimal(preview.additional_reserve, preview.fee_decimals)} {preview.fee_asset}</p
        >{/if}
      <p class="text-[12px]"
        >{w('approvals')}: {preview.requirement.approvals} · {w('delay_ms')}: {durationText(preview.requirement.delay_ms, locale())}</p
      >
      <p class="text-muted text-[12px]">{w('previewHint')}</p>
      {#each preview.blockers as blocker}<Notice tone="danger">{w(blocker)}</Notice>{/each}
      <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => (preview = null)}>{w('editTransfer')}</button>
      <button
        class={buttonClass('primary', 'sm', 'justify-self-end')}
        disabled={busy || preview.blockers.length > 0}
        onclick={submit}
        data-testid="wallet-send">{w('proposePayment')}</button
      >
    </section>
  {/if}
</div>
