<script lang="ts">
  import { t } from '../i18n'
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

  const load = () =>
    attempt(async () => {
      orders = await call('ops/orders')
      transfers = await call('ops/transfers')
      claims = await call('ops/claims')
    })

  const page = (method: string, after: string) =>
    attempt(async () => {
      const next = await call(method, { after })
      if (method === 'ops/orders') orders = next
      else if (method === 'ops/transfers') transfers = next
      else claims = next
    })

  const act = (method: string, params: unknown) =>
    attempt(async () => {
      const result = await call(method, params)
      notice = result.status ?? (result.swept !== undefined ? String(result.swept) : '')
      orders = await call('ops/orders')
      transfers = await call('ops/transfers')
      claims = await call('ops/claims')
    })
</script>

<div class="card" data-testid="operations">
  <div class="row spread">
    <h2>{t('operations')}</h2>
    <button disabled={busy} onclick={load}>{t('refresh')}</button>
  </div>
  <p class="muted">{t('operationsHint')}</p>
  {#if error}<p class="error">{error}</p>{/if}
  {#if notice}<p class="muted">{notice}</p>{/if}
  {#if orders}
    <h3>{t('orders')}</h3>
    <ul class="list">
      {#each orders.orders as o (o.order_id)}
        <li>
          <div class="row spread">
            <span>{o.product_id} · {o.sku} <span class="badge">{o.status}</span></span>
            <button disabled={busy} onclick={() => act('ops/reconcile_order', { id: o.order_id })}>{t('reconcile')}</button>
          </div>
          {#each o.balances as b (b.ledger)}
            <div class="mono muted">{b.ledger}: in {b.incoming} · refundable {b.refundable} · reserve {b.service_reserve} · fee {b.fee_reserve} · out {b.outgoing}</div>
          {/each}
        </li>
      {/each}
    </ul>
    {#if orders.next}<button disabled={busy} onclick={() => page('ops/orders', orders.next)}>{t('next')}</button>{/if}
  {/if}
  {#if transfers}
    <h3>{t('transfers')}</h3>
    <ul class="list">
      {#each transfers.transfers as x (x.transfer_id)}
        <li>
          <div class="row spread">
            <span><span class="badge">{x.status}</span> {x.amount} → <span class="mono">{x.to}</span></span>
            <span class="row">
              {#if !['Succeeded', 'Superseded'].includes(x.status)}
                <button disabled={busy} onclick={() => act('ops/process_transfer', { id: x.transfer_id })}>{t('retry')}</button>
              {/if}
              {#if x.revisable}
                <button disabled={busy} onclick={() => act('ops/revise_transfer', { id: x.transfer_id, fee: x.expected_fee })}>{t('revise')}</button>
              {/if}
            </span>
          </div>
          <div class="mono muted">{x.ledger} · fee {x.fee} / {x.max_fee}{x.expected_fee ? ` · ledger asks ${x.expected_fee}` : ''}{x.error_code ? ` · ${x.error_code}` : ''}</div>
        </li>
      {/each}
    </ul>
    {#if transfers.next}<button disabled={busy} onclick={() => page('ops/transfers', transfers.next)}>{t('next')}</button>{/if}
  {/if}
  {#if claims}
    <h3>{t('claims')}</h3>
    <ul class="list">
      {#each claims.claims as c (c.claim_id)}
        <li class="row spread">
          <span>{c.product_id} · {c.sku} <span class="badge">{c.status}</span> {c.eligibility} · {t('validUntil')} {when(c.valid_until_ms)}</span>
          <button disabled={busy} onclick={() => act('ops/claim', { id: c.claim_id, refresh: c.refreshable })}>
            {c.refreshable ? t('refreshClaim') : t('reconcile')}
          </button>
        </li>
      {/each}
    </ul>
    {#if claims.next}<button disabled={busy} onclick={() => page('ops/claims', claims.next)}>{t('next')}</button>{/if}
    <button disabled={busy} onclick={() => act('ops/sweep', {})}>{t('sweep')}</button>
  {/if}
</div>
