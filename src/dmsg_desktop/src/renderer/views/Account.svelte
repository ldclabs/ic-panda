<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
  import { call, code, units, when } from '../rpc'

  let { status }: { status: any } = $props()
  let plan = $state<any>(null)
  let catalog = $state<any>(null)
  let assets = $state<any[]>([])
  let checkouts = $state<any[]>([])
  let orders = $state<Record<string, any>>({})
  let sku = $state('plus')
  let ledger = $state('')
  let wallet = $state<any>(null)
  let name = $state('')
  let handle = $state<any>(null)
  let error = $state('')
  let busy = $state(false)

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  const load = (refresh = false) =>
    attempt(async () => {
      plan = await call('plan/status', { refresh })
      catalog ??= await call('plan/catalog')
      assets = await call('plan/assets')
      checkouts = await call('plan/checkouts')
      if (!ledger && assets.length) ledger = assets[0].ledger
    })

  /** A quote only fixes the terms; paying it is a separate approval. */
  const checkout = () =>
    attempt(async () => {
      const job = await call('plan/quote', { sku: sku.trim(), method: 'cash', selection: ledger })
      const request = await call('plan/checkout', { id: job.id })
      await window.dmsg.openApproval(request.id)
      checkouts = await call('plan/checkouts')
    })

  const order = (method: string, id: string) =>
    attempt(async () => {
      orders[id] = await call(method, { id })
    })

  const balance = () =>
    attempt(async () => {
      wallet = await call('wallet/balance', { ledger })
    })

  const lookup = () =>
    attempt(async () => {
      handle = await call('handle/price', { name: name.trim() })
    })

  const take = (method: 'handle/claim' | 'handle/register') =>
    attempt(async () => {
      const request = await call(method, { name: handle.name })
      await window.dmsg.openApproval(request.id)
    })

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['approval/changed', 'session/changed'].includes(event.type)) void load()
    })
  })

  const unlocked = $derived(status?.state === 'unlocked')
  const asset = $derived(assets.find((a) => a.ledger === ledger))
</script>

<h1>{t('planAndNames')}</h1>
{#if error}<p class="error" data-testid="account-error">{error}</p>{/if}
<div class="card">
  <div class="row spread">
    <h2>{t('plan')}</h2>
    <button disabled={busy} onclick={() => load(true)}>{t('refresh')}</button>
  </div>
  {#if plan}
    <dl>
      <dt>{t('plan')}</dt>
      <dd data-testid="plan-name">{plan.plan}</dd>
      <dt>{t('storage')}</dt>
      <dd>{units(String(plan.storage_bytes ?? 0), 9)} GB</dd>
      {#if plan.valid_until_ms}<dt>{t('validUntil')}</dt><dd>{when(plan.valid_until_ms)}</dd>{/if}
    </dl>
  {/if}
  {#if catalog}
    <ul class="list">
      {#each catalog.plans as p (p.plan)}
        <li class="row spread">
          <span>{p.plan}</span>
          <span class="muted">${(p.price_cents / 100).toFixed(2)} · {units(String(p.storage_bytes), 9)} GB</span>
        </li>
      {/each}
    </ul>
  {/if}
  {#if unlocked}
    <div class="row" style="margin-top: 8px">
      <input placeholder="sku" bind:value={sku} style="max-width: 120px" />
      <select bind:value={ledger} data-testid="plan-asset">
        {#each assets as a (a.ledger)}<option value={a.ledger}>{a.asset}</option>{/each}
      </select>
      <button class="primary" disabled={busy || !ledger || !sku.trim()} onclick={checkout}>{t('subscribe')}</button>
    </div>
  {/if}
</div>
{#if checkouts.length}
  <div class="card">
    <h2>{t('orders')}</h2>
    <ul class="list" data-testid="checkouts">
      {#each checkouts as job (job.id)}
        <li>
          <div class="row spread">
            <span>{job.sku} · {job.terms.method === 'Cash' ? `${units(job.terms.amount, job.terms.decimals)} ${job.terms.asset}` : 'PANDA'}</span>
            <span class="muted">{job.stage}{orders[job.id] ? ` · ${orders[job.id].status}` : ''}</span>
          </div>
          <div class="row">
            <button disabled={busy} onclick={() => order('plan/order', job.id)}>{t('status')}</button>
            {#if job.stage === 'review'}
              <button disabled={busy} onclick={() => attempt(async () => window.dmsg.openApproval((await call('plan/checkout', { id: job.id })).id))}>{t('pay')}</button>
            {:else}
              <button disabled={busy} onclick={() => order('plan/reconcile', job.id)}>{t('reconcile')}</button>
              <button class="danger" disabled={busy} onclick={() => order('plan/cancel', job.id)}>{t('cancel')}</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  </div>
{/if}
<div class="card">
  <h2>{t('names')}</h2>
  <div class="row">
    <input placeholder={t('name')} bind:value={name} data-testid="handle-name" />
    <button disabled={busy || !name.trim()} onclick={lookup}>{t('lookUp')}</button>
  </div>
  {#if handle}
    <p data-testid="handle-state">
      <strong>{handle.name}</strong> ·
      {#if handle.owner}{t('taken')} <span class="mono">{handle.owner}</span>{:else if handle.reserved}{t('reserved')}{:else}{t('available')} · {units(handle.price, 8)} PANDA{/if}
    </p>
    {#if unlocked && !handle.owner}
      {#if handle.reserved}
        <button class="primary" disabled={busy} onclick={() => take('handle/claim')}>{t('claim')}</button>
      {:else}
        <button class="primary" disabled={busy} onclick={() => take('handle/register')}>{t('register')}</button>
      {/if}
    {/if}
  {/if}
</div>
{#if unlocked}
  <div class="card">
    <h2>{t('wallet')}</h2>
    <p class="muted">{t('walletHint')}</p>
    <button disabled={busy || !ledger} onclick={balance}>{t('balance')}</button>
    {#if wallet}
      <dl>
        <dt>{t('payer')}</dt>
        <dd class="mono">{wallet.principal}</dd>
        <dt>{t('balance')}</dt>
        <dd data-testid="wallet-balance">{asset ? `${units(wallet.balance, asset.decimals)} ${asset.asset}` : wallet.balance}</dd>
      </dl>
    {/if}
  </div>
{/if}
