<script lang="ts">
  import { onMount, untrack } from 'svelte'
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
  import { w } from './text'
  let { wallet, metadata, now, onOpen }: { wallet: Wallet; metadata: Asset[]; now: number; onOpen: (id: number) => void } = $props()
  let rows = $state<Proposal[]>([]),
    notes = $state<Record<string, string>>({}),
    search = $state(''),
    from = $state(''),
    to = $state(''),
    busy = $state(false),
    error = $state(''),
    complete = $state(false)
  let alive = true,
    generation = 0
  const shown = $derived(
    rows.filter((p) => {
      const text = [p.id, intentHeadline(p.intent, metadata).text, notes[p.id] ?? '', p.proposer].join(' ').toLowerCase()
      return (
        text.includes(search.trim().toLowerCase()) &&
        (!from || p.created_at >= new Date(`${from}T00:00:00`).getTime()) &&
        (!to || p.created_at <= new Date(`${to}T23:59:59.999`).getTime())
      )
    })
  )
  async function load() {
    const seq = ++generation
    busy = true
    error = ''
    complete = false
    try {
      const [history, local] = await Promise.all([
        call<any>('wallet/history', { wallet: wallet.wallet }),
        call<Record<string, string>>('wallet/notes', { wallet: wallet.wallet })
      ])
      if (alive && seq === generation) {
        rows = history.proposals
        notes = local
        complete = history.complete === true
      }
    } catch (e) {
      if (alive && seq === generation) error = walletError(e)
    } finally {
      if (alive && seq === generation) busy = false
    }
  }
  async function save() {
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
    await bridge(() => window.dmsg.saveReport({ format: 'dmsg-wallet-history/1', rows: report }))
  }
  $effect(() => {
    wallet
    untrack(() => void load())
  })
  onMount(() => () => {
    alive = false
    generation++
  })
</script>

<section class="grid gap-4" data-testid="wallet-history" aria-busy={busy}>
  <p class="text-muted text-[12px]">{w('historyCoverage')}</p>
  <div class="grid gap-3 sm:grid-cols-[minmax(0,2fr)_1fr_1fr]">
    <Field label={w('searchHistory')}><input bind:value={search} /></Field>
    <Field label={w('fromDate')}><input type="date" bind:value={from} /></Field>
    <Field label={w('toDate')}><input type="date" bind:value={to} /></Field>
  </div>
  {#if error}<Notice failure={error} tone="danger">{error}<button class={buttonClass('ghost', 'sm')} onclick={load}>{w('retry')}</button></Notice>{/if}
  <div class="flex flex-wrap items-center justify-between gap-3"
    ><p class="text-muted text-[12px]" role="status">{busy ? w('loading') : `${shown.length}`}</p><button
      class={buttonClass('secondary', 'sm')}
      disabled={busy || !complete || !shown.length}
      onclick={save}>{w('exportHistory')}</button
    ></div
  >
  {#each shown as p (p.id)}
    {@const summary = intentHeadline(p.intent, metadata)}
    <button class="border-line grid gap-2 border-b py-3 text-start hover:bg-wash" onclick={() => onOpen(p.id)}>
      <div class="flex flex-wrap items-center gap-2"
        ><Pill>#{p.id}</Pill><b class="text-[13px]">{w(summary.kind)}</b><Pill>{w(resultState(p, now))}</Pill></div
      >
      <p class="text-[13px] break-all">{summary.text}</p>
      {#if notes[p.id]}<p class="text-[12px]">{w('privateNote')}: {notes[p.id]}</p>{/if}
      <p class="text-muted text-[12px]">{when(p.created_at)}</p>
    </button>
  {:else}{#if !busy && complete}<p class="text-muted">{w('noHistory')}</p>{/if}{/each}
</section>
