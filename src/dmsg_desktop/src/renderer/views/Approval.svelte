<script lang="ts">
  import { onMount } from 'svelte'
  import { t, stateLabel } from '../i18n'
  import { call, code, units, when } from '../rpc'

  let { id }: { id: string } = $props()
  let request = $state<any>(null)
  let error = $state('')
  let busy = $state(false)
  let outcome = $state('')

  async function load() {
    try {
      request = await call('approval/show', { id })
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    }
  }

  /** Decide exactly what is shown: the digest of this view goes with the answer. */
  async function decide(approve: boolean) {
    busy = true
    error = ''
    try {
      const decided: any = await window.dmsg.decide(request.id, request.digest, approve)
      outcome = decided.state
      request = decided
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
      await load()
    } finally {
      busy = false
    }
  }

  onMount(() => {
    void load()
  })
  const d = $derived(request?.detail ?? {})
  const s = $derived(d.statement ?? d.content?.statement)
</script>

{#if !request}
  <p class="muted">{error || '…'}</p>
{:else}
  <h1 data-testid="approval-title">
    {request.type === 'connect'
      ? d.app_id
      : request.type === 'attest'
        ? t('newStatement')
        : request.type === 'checkout'
          ? t('payPlan')
          : request.type === 'letter'
            ? t('payLetter')
            : request.type === 'handle'
              ? `${t('names')}: ${d.name}`
              : request.type}
  </h1>
  <div class="card">
    <dl>
      <dt>{t('from')}</dt>
      <dd data-testid="approval-source">{request.source}</dd>
      <dt>{t('status')}</dt>
      <dd>{stateLabel(request.state)}</dd>
      <dt>{t('expires')}</dt>
      <dd>{when(request.expires_at)}</dd>
      {#if request.type === 'connect'}
        <dt>{t('origin')}</dt>
        <dd data-testid="approval-origin">{d.origin}</dd>
        <dt>{t('browser')}</dt>
        <dd>{d.pairing_label}</dd>
        <dt>{t('account')}</dt>
        <dd class="mono">{d.account}</dd>
        <dt>{t('method')}</dt>
        <dd>{d.method} · v{d.app_version}</dd>
        <dt>{t('requested')}</dt>
        <dd>{when(d.requested_at)}</dd>
      {/if}
      {#if request.type === 'device_capabilities'}
        <dt>{t('device')}</dt>
        <dd class="mono">{d.device_id}{d.this_device ? ` (${t('thisDevice')})` : ''}</dd>
        <dt>{t('capabilities')}</dt>
        <dd>{d.capabilities?.join(', ')}</dd>
      {/if}
      {#if request.type === 'checkout'}
        <dt>{t('plan')}</dt>
        <dd data-testid="approval-sku">{d.sku} · {d.origin}</dd>
        <dt>{t('price')}</dt>
        <dd>${units(d.amount_usd_micros, 6)}</dd>
        {#if d.terms?.method === 'Cash'}
          <dt>{t('pay')}</dt>
          <dd data-testid="approval-amount">
            {units(d.terms.amount, d.terms.decimals)} {d.terms.asset} + {t('fee')} {units(d.terms.fee_reserve, d.terms.decimals)}
          </dd>
          <dt>{t('ledger')}</dt>
          <dd class="mono">{d.terms.ledger}</dd>
          <dt>{t('deadline')}</dt>
          <dd>{when(d.terms.funding_deadline_ms)}</dd>
        {:else if d.terms}
          <dt>{t('neuron')}</dt>
          <dd class="mono">{d.terms.neuron}</dd>
          <dt>{t('stake')}</dt>
          <dd data-testid="approval-amount">{units(d.terms.required_stake_e8s, 8)} PANDA</dd>
          <dt>{t('deadline')}</dt>
          <dd>{when(d.terms.application_deadline_ms)}</dd>
        {/if}
        <dt>{t('payer')}</dt>
        <dd class="mono">{d.actor}</dd>
      {/if}
      {#if request.type === 'letter'}
        <dt>{t('recipient')}</dt>
        <dd class="mono" data-testid="approval-recipient">{d.recipient}</dd>
        <dt>{t('pay')}</dt>
        <dd data-testid="approval-amount">{d.amount} ({t('baseUnits')})</dd>
        <dt>{t('ledger')}</dt>
        <dd class="mono">{d.ledger}</dd>
      {/if}
      {#if request.type === 'handle'}
        <dt>{t('method')}</dt>
        <dd>{d.action}</dd>
        {#if d.total}<dt>{t('pay')}</dt><dd data-testid="approval-amount">{units(d.total, 8)} PANDA</dd>{/if}
        <dt>{t('payer')}</dt>
        <dd class="mono">{d.caller}</dd>
      {/if}
      {#if request.type === 'device_revoke'}
        <dt>{t('revoke')}</dt>
        <dd class="mono">{d.device_id}</dd>
      {/if}
    </dl>
    {#if request.type === 'connect'}
      <p class="warn" data-testid="approval-relayed">{t('relayed')}</p>
    {/if}
  </div>
  {#if s}
    <div class="card">
      <h2>{t('content')}</h2>
      <dl>
        <dt>{t('profile')}</dt>
        <dd>{s.profile}</dd>
        <dt>{t('issuer')}</dt>
        <dd class="mono">{s.issuer}</dd>
        {#if s.subject}<dt>{t('subject')}</dt><dd>{s.subject}</dd>{/if}
        {#if s.content?.text}<dt>{t('statement')}</dt><dd data-testid="approval-text">{s.content.text}</dd>{/if}
        {#if s.content?.sha256}<dt>sha256</dt><dd class="mono">{s.content.sha256}</dd>{/if}
        {#if d.origin && request.type === 'attest'}<dt>{t('origin')}</dt><dd>{d.origin}</dd>{/if}
      </dl>
      {#if s.content?.sha256}
        <p class={d.file_checked ? 'muted' : 'warn'}>
          {d.file ? (d.file_checked ? t('fileChecked') : t('fileChanged')) : t('digestOnly')}
        </p>
      {/if}
    </div>
  {/if}
  {#if error}<p class="error" data-testid="approval-error">{error}</p>{/if}
  {#if request.state === 'awaiting_user'}
    <div class="row">
      <button class="primary" disabled={busy} data-testid="approve" onclick={() => decide(true)}>{t('approve')}</button>
      <button disabled={busy} data-testid="reject" onclick={() => decide(false)}>{t('reject')}</button>
    </div>
  {:else}
    <p data-testid="approval-outcome">{t('decided')}: {stateLabel(outcome || request.state)}</p>
  {/if}
{/if}
