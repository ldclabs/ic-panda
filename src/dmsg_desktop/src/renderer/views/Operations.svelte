<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'

  /**
   * Orders, cash transfers and PANDA commitments the wallet may read as payer,
   * merchant or operator. Every action repeats an original operation with its
   * frozen parameters; none starts a new payment.
   */
  let orders = $state<any>(null)
  let transfers = $state<any>(null)
  let claims = $state<any>(null)
  let notice = $state('')
  let error = $state('')
  let busy = $state(false)

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    notice = ''
    try {
      await work()
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  async function all() {
    orders = await call('ops/orders')
    transfers = await call('ops/transfers')
    claims = await call('ops/claims')
  }

  const page = (method: string, after: string) =>
    attempt(async () => {
      const next = await call(method, { after })
      if (method === 'ops/orders') orders = next
      else if (method === 'ops/transfers') transfers = next
      else claims = next
    })

  const act = (method: string, params: unknown) =>
    attempt(async () => {
      const result: any = await call(method, params)
      notice = result.status ?? (result.swept !== undefined ? String(result.swept) : '')
      await all()
    })

  const finished = (status: string) => ['Succeeded', 'Superseded'].includes(status)
</script>

{#snippet heading(title: string)}
  <h3 class="border-t border-line bg-surface-2/60 px-5 py-1.5 text-[11.5px] font-medium text-faint">{title}</h3>
{/snippet}

<Section title={t('operations')} description={t('operationsHint')} icon="transfer" flush testid="operations">
  {#snippet actions()}
    <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => attempt(all)}><Icon name="sync-now" />{orders ? t('refresh') : t('open')}</button>
  {/snippet}
  {#if error || notice}
    <div class="px-5 pb-3">
      {#if error}<Notice tone="danger">{error}</Notice>{:else}<Notice tone="ok">{notice}</Notice>{/if}
    </div>
  {/if}
  {#if orders}
    {@render heading(t('orders'))}
    <ul class="divide-y divide-line border-t border-line">
      {#each orders.orders as o (o.order_id)}
        <li class="grid gap-1 px-5 py-2.5">
          <div class="flex flex-wrap items-center gap-2">
            <span class="min-w-0 flex-1 text-[13px]">{o.product_id} · {o.sku}</span>
            <Pill>{o.status}</Pill>
            <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => act('ops/reconcile_order', { id: o.order_id })}>{t('reconcile')}</button>
          </div>
          {#each o.balances as b (b.ledger)}
            <p class="font-mono text-[11.5px] text-faint">
              {b.ledger}: {t('opsBalances', { incoming: b.incoming, refundable: b.refundable, reserve: b.service_reserve, fee: b.fee_reserve, outgoing: b.outgoing })}
            </p>
          {/each}
        </li>
      {:else}
        <li class="px-5 py-2.5 text-[12.5px] text-faint">-</li>
      {/each}
    </ul>
    {#if orders.next}<div class="px-5 py-2"><button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => page('ops/orders', orders.next)}>{t('more')}</button></div>{/if}
  {/if}
  {#if transfers}
    {@render heading(t('transfers'))}
    <ul class="divide-y divide-line border-t border-line">
      {#each transfers.transfers as x (x.transfer_id)}
        <li class="grid gap-1 px-5 py-2.5">
          <div class="flex flex-wrap items-center gap-2">
            <Pill tone={finished(x.status) ? 'ok' : 'warn'}>{x.status}</Pill>
            <span class="min-w-0 flex-1 truncate text-[13px] tabular-nums">{x.amount} → <bdi dir="ltr" class="font-mono text-[12px]">{x.to}</bdi></span>
            {#if !finished(x.status)}
              <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => act('ops/process_transfer', { id: x.transfer_id })}>{t('retry')}</button>
            {/if}
            {#if x.revisable}
              <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => act('ops/revise_transfer', { id: x.transfer_id, fee: x.expected_fee })}>{t('revise')}</button>
            {/if}
          </div>
          <p class="font-mono text-[11.5px] text-faint">
            {x.ledger} · {t('opsFee', { fee: x.fee, max: x.max_fee })}{x.expected_fee ? ` · ${t('ledgerAsks', { fee: x.expected_fee })}` : ''}{x.error_code ? ` · ${x.error_code}` : ''}
          </p>
        </li>
      {:else}
        <li class="px-5 py-2.5 text-[12.5px] text-faint">-</li>
      {/each}
    </ul>
    {#if transfers.next}<div class="px-5 py-2"><button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => page('ops/transfers', transfers.next)}>{t('more')}</button></div>{/if}
  {/if}
  {#if claims}
    {@render heading(t('claims'))}
    <ul class="divide-y divide-line border-t border-line">
      {#each claims.claims as c (c.claim_id)}
        <li class="flex flex-wrap items-center gap-2 px-5 py-2.5 text-[13px]">
          <span class="min-w-0 flex-1">{c.product_id} · {c.sku} · {c.eligibility} · {t('validUntil')} {when(c.valid_until_ms)}</span>
          <Pill>{c.status}</Pill>
          <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => act('ops/claim', { id: c.claim_id, refresh: c.refreshable })}>
            {c.refreshable ? t('refreshClaim') : t('reconcile')}
          </button>
        </li>
      {:else}
        <li class="px-5 py-2.5 text-[12.5px] text-faint">-</li>
      {/each}
    </ul>
    <div class="flex gap-2 border-t border-line px-5 py-2.5">
      {#if claims.next}<button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => page('ops/claims', claims.next)}>{t('more')}</button>{/if}
      <span class="flex-1"></span>
      <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => act('ops/sweep', {})}>{t('sweep')}</button>
    </div>
  {/if}
</Section>
