<script lang="ts">
  import DateInput from '../../components/DateInput.svelte'
  import { validDateText } from '../../components/form-values'
  import { onMount, untrack } from 'svelte'
  import { display, watchDisplays, type Mode } from '../../display.svelte'
  import DisplayNotice from '../../components/DisplayNotice.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import { buttonClass } from '../../components/ui'
  import { call, bridge, when } from '../../rpc'
  import { walletError } from './errors'
  import {
    assetIdentity,
    assetInfo,
    amountText,
    networkName,
    intentHeadline,
    resultState,
    type Asset,
    type Wallet,
    type Proposal
  } from './model'
  import { t } from '../../i18n/index.svelte'
  import { toast } from '../../components/toast.svelte'
  import { w } from './text'
  let { wallet, metadata, now, onOpen }: { wallet: Wallet; metadata: Asset[]; now: number; onOpen: (id: number) => void } = $props()
  let rows = $state<Proposal[]>([]),
    notes = $state<Record<string, string>>({}),
    search = $state(''),
    from = $state(''),
    to = $state(''),
    busy = $state(false),
    error = $state(''),
    notesError = $state(''),
    exportError = $state(''),
    saving = $state(false),
    complete = $state(false)
  let alive = true,
    generation = 0
  const rangeValid = $derived((!from || validDateText(from, 'date')) && (!to || validDateText(to, 'date')) && (!from || !to || from <= to))
  const filtered = $derived(!!(search.trim() || from || to))
  const shown = $derived.by(() => {
    if (!rangeValid) return []
    const query = search.trim().toLowerCase()
    const start = from ? new Date(`${from}T00:00:00`).getTime() : -Infinity
    const end = to ? new Date(`${to}T23:59:59.999`).getTime() : Infinity
    return rows.filter(p => {
      if (p.created_at < start || p.created_at > end) return false
      if (!query) return true
      const headline = intentHeadline(p.intent, metadata)
      const terms = [`#${p.id}`, w(headline.kind), headline.text, w(resultState(p, now)), notes[p.id], p.proposer,
        ...(p.intent.outputs ?? []).map((o: any) => o.to), ...p.signed.map(s => s.txid)]
      return terms.join(' ').toLowerCase().includes(query)
    })
  })
  async function load(mode: Mode = 'swr') {
    const seq = ++generation
    busy = true
    const current = () => alive && seq === generation
    const params = { wallet: wallet.wallet }
    await Promise.all([
      display<any>('wallet/history', params, mode).then(history => {
        if (current()) { rows = history.proposals; complete = history.complete === true; error = '' }
      }).catch(e => { if (current()) { error = walletError(e); complete = false } }),
      call<Record<string, string>>('wallet/notes', params).then(local => {
        if (current()) { notes = local; notesError = '' }
      }).catch(e => { if (current()) notesError = walletError(e) })
    ])
    if (current()) busy = false
  }
  async function save() {
    if (!rangeValid || busy || saving || notesError || !complete || !shown.length) return
    saving = true
    exportError = ''
    const report = shown.flatMap((p) =>
      (p.intent.outputs?.length ? p.intent.outputs : [{}]).map((o: any) => [
        wallet.wallet,
        String(p.id),
        w(resultState(p, now)),
        w(p.intent.kind),
        p.intent.asset ? networkName(p.intent.asset) : '',
        p.intent.asset ? assetIdentity(p.intent.asset) : '',
        o.amount && p.intent.asset ? amountText(o.amount, assetInfo(p.intent.asset, metadata), w('atomic')) : '',
        o.amount ?? '',
        o.to ?? '',
        new Date(p.created_at).toISOString(),
        p.signed.map((s) => s.txid).join(' '),
        (p.outcome?.blocks ?? []).join(' '),
        notes[p.id] ?? '',
        w('historyCoverage')
      ])
    )
    try {
      const saved = await bridge<{ path: string } | null>(() => window.dmsg.saveReport({ format: 'dmsg-wallet-history/1', rows: report }))
      if (alive && saved) toast(() => `${w('saved')} · ${saved.path}`)
    } catch (e) { if (alive) exportError = walletError(e) }
    finally { if (alive) saving = false }
  }
  $effect(() => {
    wallet
    untrack(() => void load())
  })
  onMount(() => {
    const stop = watchDisplays(['history'], load)
    return () => { alive = false; generation++; stop() }
  })
</script>
<DisplayNotice methods={['wallet/history']} params={{wallet: wallet.wallet}} />

<section class="grid gap-4" data-testid="wallet-history" aria-busy={busy}>
  <p class="text-muted text-callout">{w('historyCoverage')}</p>
  <div class="grid gap-3 sm:grid-cols-[minmax(0,2fr)_1fr_1fr]">
    <Field label={w('searchHistory')}><input bind:value={search} data-testid="wallet-history-search" /></Field>
    <DateInput label={w('fromDate')} bind:value={from} max={validDateText(to, 'date') ? to : undefined} testid="wallet-history-from" />
    <DateInput label={w('toDate')} bind:value={to} testid="wallet-history-to" />
  </div>
  {#if error || notesError}<Notice tone="danger" testid="wallet-history-error">
    {error}{error && notesError ? ' · ' : ''}{notesError ? `${w('privateNote')}: ${notesError}` : ''}
    <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => load('fresh')}>{w('retry')}</button>
  </Notice>{/if}
  {#if exportError}<Notice tone="danger" testid="wallet-history-export-error">{exportError}</Notice>{/if}
  <div class="flex flex-wrap items-center justify-between gap-3"
    ><div class="flex flex-wrap items-center gap-2"><p class="text-muted text-callout" role="status">{busy ? w('loading') : rangeValid && (rows.length || complete) ? `${shown.length}` : ''}</p>
        {#if filtered}<button class={buttonClass('ghost', 'sm')} onclick={() => { search = from = to = '' }}>{w('clearFilters')}</button>{/if}</div><button
      class={buttonClass('secondary', 'sm')}
      disabled={busy || saving || !!notesError || !rangeValid || !complete || !shown.length} aria-busy={saving}
      onclick={save}>{w('exportHistory')}</button
    ></div
  >
  {#each shown as p (p.id)}
    {@const summary = intentHeadline(p.intent, metadata)}
    <button class="border-line grid gap-2 border-b py-3 text-start hover:bg-wash" onclick={() => onOpen(p.id)}>
      <div class="flex flex-wrap items-center gap-2"
        ><Pill>#{p.id}</Pill><b class="text-body">{w(summary.kind)}</b><Pill>{w(resultState(p, now))}</Pill></div
      >
      {#if summary.text}<p class="text-body break-all">{summary.text}</p>{/if}
      {#if notes[p.id]}<p class="text-callout">{w('privateNote')}: {notes[p.id]}</p>{/if}
      <p class="text-muted text-callout">{when(p.created_at)}</p>
    </button>
  {:else}{#if !busy && complete && rangeValid}<p class="text-muted">{filtered ? w('noHistory') : t('noItems')}</p>{/if}{/each}
</section>
