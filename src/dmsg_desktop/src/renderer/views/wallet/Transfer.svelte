<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import Collapsible from '../../components/Collapsible.svelte'
  import { walletError as errorText } from './errors'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass } from '../../components/ui'
  import { call, code } from '../../rpc'
  import { t, locale } from '../../i18n/index.svelte'
  import Intent from './Intent.svelte'
  import {
    assetChain,
    assetIdentity,
    assetKey,
    amountText,
    networkName,
    selectTransferSource,
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
    metadataReady,
    assetError,
    onReadAssets,
    book = [],
    balances,
    onPrepareNonce,
    onCheckStatus,
    draft = $bindable(),
    busy = $bindable(false),
    onDone
  }: {
    wallet: Wallet
    metadata: Asset[]
    metadataReady: boolean
    assetError: string
    onReadAssets: () => void
    book?: any[]
    balances?: any
    onPrepareNonce: (address: any) => void
    onCheckStatus: () => void
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
  let error = $state('')
  let form: HTMLDivElement
  let reviewHeading = $state<HTMLHeadingElement>()
  function selectSource(from: string, key: string) {
    selectTransferSource(draft, from, key, wallet.addresses)
    attempted = false
    error = ''
  }
  type Reviewed = { preview: any; input: ReturnType<typeof payload>; wallet: string }
  type Operation = { kind: 'editing' | 'estimating' } | ({ kind: 'reviewed' | 'submitting' } & Reviewed)
  let operation = $state.raw<Operation>({ kind: 'editing' })
  const preview = $derived('preview' in operation ? operation.preview : null)
  $effect(() => { busy = operation.kind === 'estimating' || operation.kind === 'submitting' })
  const address = $derived(wallet.addresses.find((a) => String(a.id) === draft.from))
  const available = $derived(metadata.filter((a) => assetChain(a) === address?.chain))
  const asset = $derived(available.find((a) => assetKey(a) === draft.asset))
  // While a previously used token reloads, keep the amount in the units it was entered in.
  const waitingForPrecision = $derived(asset?.decimals == null && !metadataReady && draft.precision?.key === draft.asset && draft.precision?.decimals != null)
  const fee = $derived(asset ? feeAsset(asset) : undefined)
  const fingerprint = $derived(JSON.stringify([draft, asset?.decimals, fee?.decimals, wallet.policy.version]))
  const reviewed = $derived(preview?.fingerprint === fingerprint)
  $effect(() => {
    if (operation.kind === 'reviewed' && !reviewed) operation = { kind: 'editing' }
  })
  $effect(() => {
    const key = asset ? assetKey(asset) : ''
    const precision = asset?.decimals ?? null
    if (key && !waitingForPrecision) untrack(() => {
      if (draft.precision?.key === key && draft.precision.decimals !== precision) {
        draft.outputs = draft.outputs.map((o: any) => ({ ...o, amount: '' }))
        draft.maxFee = ''
      }
      if (draft.precision?.key !== key || draft.precision.decimals !== precision) draft.precision = { key, decimals: precision }
    })
  })
  $effect(() => {
    const from = draft.from
    const next = available.some(a => assetKey(a) === draft.asset) ? draft.asset : available[0] ? assetKey(available[0]) : ''
    if (draft.asset !== next) untrack(() => selectSource(from, next))
  })
  function changeAddress(from: string) {
    const chain = wallet.addresses.find(a => String(a.id) === from)?.chain
    const options = metadata.filter(a => assetChain(a) === chain)
    selectSource(from, options.some(a => assetKey(a) === draft.asset) ? draft.asset : options[0] ? assetKey(options[0]) : '')
  }
  const sourceBalance = $derived(
    balances?.addresses?.find((r: any) => r.address === address?.id)?.balances?.find((b: any) => assetKey(b.metadata ?? b) === draft.asset)
  )
  const rowErrors = $derived(
    draft.outputs.map((o: any) => {
      const to = o.to.trim()
      const invalidTo = !to || /\s/.test(to)
        || (address?.chain === 'Evm' && !/^0x[0-9a-fA-F]{40}$/.test(to))
        || (address?.chain === 'Solana' && !/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(to))
      let invalidAmount = true
      try { invalidAmount = BigInt(toAtomic(o.amount.trim(), asset?.decimals ?? 0)) <= 0n } catch { /* incomplete or exceeds precision */ }
      return { to: invalidTo, amount: invalidAmount }
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
      // The kept draft may hold a memo its field no longer shows.
      memo: (address?.chain === 'Icp' && draft.outputs.length === 1 && draft.memo) || null
    }
  }
  async function estimate() {
    if (operation.kind !== 'editing' || waitingForPrecision) return
    attempted = true
    if (rowErrors.some((row: { to: boolean; amount: boolean }) => row.to || row.amount)) {
      await tick()
      form.querySelector<HTMLInputElement>('input[aria-invalid="true"]')?.focus()
      return
    }
    if (!address?.verified || draft.outputs.length > batchLimit(address?.chain)) return
    const estimating: Operation = { kind: 'estimating' }
    operation = estimating
    error = ''
    const reviewedFields = fingerprint,
      target = wallet.wallet
    try {
      const input = payload()
      const result: any = await call('wallet/preview', { wallet: target, draft: input })
      if (!alive || operation !== estimating || reviewedFields !== fingerprint || target !== wallet.wallet) return
      operation = { kind: 'reviewed', input: { ...input, max_fee: input.max_fee ?? result.max_fee }, wallet: target,
        preview: { ...result, fingerprint: reviewedFields } }
      await tick()
      reviewHeading?.focus()
    } catch (e) {
      if (alive && operation === estimating) error = e instanceof Error && ['amount', 'precision', 'overflow'].includes(e.message) ? w('invalidAmount') : errorText(e)
    } finally {
      if (operation === estimating) operation = { kind: 'editing' }
    }
  }
  async function submit() {
    if (operation.kind !== 'reviewed' || !reviewed || draft.checkPending || preview.blockers.length) return
    const accepted = operation
    const submitting: Operation = { ...accepted, kind: 'submitting' }
    operation = submitting
    error = ''
    try {
      const note = draft.note?.trim() ?? ''
      const p: any = await call('wallet/transfer', { wallet: accepted.wallet, draft: accepted.input, policy_version: accepted.preview.policy_version })
      if (!alive || operation !== submitting) return
      if (note) {
        try {
          await call('wallet/notes', { wallet: accepted.wallet, proposal: p.id, note })
          if (!alive || operation !== submitting) return
        } catch {
          p.note_error = note
        }
      }
      if (!alive || operation !== submitting) return
      draft.outputs = [{ to: '', amount: '' }]
      draft.note = draft.memo = draft.maxFee = ''
      operation = { kind: 'editing' }
      onDone(p)
    } catch (e) {
      if (alive && operation === submitting) {
        if (['EXECUTION_UNKNOWN', 'PENDING', 'RESULT_EXPIRED'].includes(code(e))) draft.checkPending = true
        else error = errorText(e)
      }
    } finally {
      if (operation === submitting) operation = accepted
    }
  }
</script>

<div bind:this={form} class="grid gap-4" data-testid="wallet-transfer" aria-busy={busy}>
  {#if draft.checkPending}
    <Notice tone="warn" testid="wallet-transfer-unknown">{t('operationUnknown')}</Notice>
    <button class={buttonClass('primary', 'sm', 'justify-self-end')} onclick={() => { draft.checkPending = false; onCheckStatus() }}>{w('queue')}</button>
  {:else}
  {#if asset}<p class="text-[13px] font-medium">{w('network')}: {networkName(asset)}</p>{/if}
  {#if !reviewed}
    <form class="grid gap-4" novalidate onsubmit={(event) => { event.preventDefault(); void estimate() }}>
    <div class="grid gap-3 sm:grid-cols-2">
      <Field label={w('from')}
        ><Select
          value={draft.from} onChange={changeAddress}
          label={w('from')}
          options={wallet.addresses
            .filter((a) => a.verified)
            .map((a) => ({ value: String(a.id), label: `#${a.id} · ${a.label || a.chain}` }))}
          testid="wallet-from"
        /></Field
      >
      <Field label={w('asset')}
        ><Select
          value={draft.asset} onChange={(key) => selectSource(draft.from, key)}
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
        >{w('noncePurposeHint')}<button type="button" class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => onPrepareNonce(address)}
          >{w('noncePurpose')}</button
        ></Notice
      >{/if}
    {#if assetError}<Notice tone="warn">{assetError} <button type="button" class={buttonClass('secondary', 'sm')} onclick={onReadAssets}>{w('retry')}</button></Notice>
    {:else if waitingForPrecision}<p class="text-muted text-[12px]" role="status">{w('loading')}</p>{/if}
    {#if !available.length}<Notice>{w('noAssets')}</Notice>{/if}
    {#each draft.outputs as output, i (i)}
      {@const toId = `${noteHintId}-to-${i}`}
      {@const amountId = `${noteHintId}-amount-${i}`}
      <div class="grid items-start gap-3 sm:grid-cols-[minmax(0,1fr)_10rem_auto]">
        <Field for={toId} label={`${w('recipient')} ${i + 1}`} error={attempted && rowErrors[i].to ? w('invalidDestination') : ''} errorId={toId + '-error'}>
          <input
            id={toId}
            aria-invalid={attempted && rowErrors[i].to}
            aria-describedby={attempted && rowErrors[i].to ? toId + '-error' : undefined}
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
        <Field for={amountId} error={attempted && rowErrors[i].amount ? w('invalidAmount') : ''} errorId={amountId + '-error'} label={`${w('amount')} · ${asset?.symbol || asset?.asset || ''}${asset?.decimals == null && !waitingForPrecision ? ` (${w('atomic')})` : ''}`}
          ><input
            id={amountId}
            aria-invalid={attempted && rowErrors[i].amount}
            aria-describedby={attempted && rowErrors[i].amount ? amountId + '-error' : undefined}
            disabled={waitingForPrecision}
            bind:value={output.amount}
            inputmode="decimal"
            dir="ltr"
            data-testid="wallet-amount"
          /></Field
        >
        {#if draft.outputs.length > 1}<button type="button"
          class={buttonClass('ghost', 'sm', 'sm:mt-5')}
          disabled={busy}
          onclick={() => (draft.outputs = draft.outputs.filter((_: any, n: number) => n !== i))}
          aria-label={`${w('remove')} ${i + 1}`}>{w('remove')}</button>{/if}
      </div>
    {/each}
    <p class="text-muted text-[12px]">{draft.outputs.length} / {batchLimit(address?.chain)} · {w('recipientLimit')}</p>
    {#if address?.chain !== 'Evm'}<button type="button"
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
    {#if error}<Notice failure={error} tone="danger" testid="wallet-transfer-error">{error}</Notice>{/if}
    <button
      class={buttonClass('primary', 'sm', 'justify-self-end')}
      disabled={busy || !asset || waitingForPrecision || !address?.verified}
      type="submit"
      data-testid="wallet-estimate">{w('reviewTransfer')}</button>
    </form>
  {/if}
  {#if reviewed}
    <section class="border-forest/30 bg-pale/30 grid gap-3 rounded-xl border p-4" data-testid="wallet-transfer-preview">
      <h3 bind:this={reviewHeading} tabindex="-1" class="text-[15px] font-semibold">{w('reviewTransfer')} · {wallet.name}</h3>
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
      {#if error}<Notice tone="danger" testid="wallet-transfer-error">{error}</Notice>{/if}
      <div class="flex flex-wrap items-center justify-end gap-2">
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={async () => {
        operation = { kind: 'editing' }; error = ''; await tick(); form.querySelector<HTMLInputElement>('input')?.focus()
      }}>{w('editTransfer')}</button>
      <button
        class={buttonClass('primary', 'sm', 'justify-self-end')}
        disabled={busy || preview.blockers.length > 0}
        onclick={submit}
        data-testid="wallet-send">{busy ? w('submittingProposal') : w('proposePayment')}</button>
      </div>
    </section>
  {/if}
  {/if}
</div>
