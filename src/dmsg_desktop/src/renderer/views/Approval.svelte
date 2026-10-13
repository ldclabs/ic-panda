<script lang="ts">
  import { onMount } from 'svelte'
  import { t, stateLabel } from '../i18n'
  import { call, code, units, when } from '../rpc'

  let { id }: { id: string } = $props()
  let request = $state<any>(null)
  let error = $state('')
  let busy = $state(false)
  let outcome = $state('')
  let assets = $state<any[]>([])
  let selection = $state('')

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

  /** A web checkout is quoted here first, so the amount approved is the amount shown. */
  async function quote() {
    busy = true
    error = ''
    try {
      request = await call('approval/quote', { id: request.id, selection: selection.trim() })
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  onMount(() => {
    void load().then(async () => {
      if (request?.detail?.method === 'checkout' && request.detail.content?.method === 'Cash') {
        assets = await call('plan/assets').catch(() => [])
        if (assets.length) selection = assets[0].ledger
      }
    })
  })
  const d = $derived(request?.detail ?? {})
  /** A statement to sign: this device's own, or a page's document or action. */
  const s = $derived(d.statement ?? d.content?.statement ?? (d.method === 'signAction' ? d.content : null))
  /** Terms of a checkout: a web checkout's quote, or this device's own. */
  const terms = $derived(request?.type === 'connect' ? d.checkout : request?.type === 'checkout' ? d : null)
  const changes: Record<string, string> = {
    bind_login: t('bindLogin'),
    remove_login: `${t('remove')} ${t('loginPrincipal')}`,
    policy: t('signingPolicy'),
    recovery_delay: t('recoveryDelay'),
    dispute_recovery: t('disputeRecovery')
  }
  const unquoted = $derived(request?.type === 'connect' && d.method === 'checkout' && !d.checkout)
</script>

{#if !request}
  <p class="muted">{error || '…'}</p>
{:else}
  <h1 data-testid="approval-title">
    {request.type === 'connect'
      ? d.app_id
      : request.type === 'attest'
        ? s?.profile === 'app_action'
          ? t('appAction')
          : t('newStatement')
        : request.type === 'checkout'
          ? t('payPlan')
          : request.type === 'letter'
            ? t('payLetter')
            : request.type === 'handle'
              ? `${t('names')}: ${d.name}`
              : request.type === 'delegation'
                ? d.kind?.type === 'revoke'
                  ? t('delegationRevoke')
                  : t('delegationGrant')
                : request.type === 'principal'
                  ? t('principalChange')
                  : request.type === 'account'
                    ? t('accountChange')
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
      {#if request.type === 'connect' && d.method === 'checkout'}
        <dt>{t('plan')}</dt>
        <dd>{d.content?.product_id} · {d.content?.sku}</dd>
        <dt>{t('price')}</dt>
        <dd>${units(d.content?.amount_usd_micros ?? '0', 6)} · {d.content?.method}</dd>
      {/if}
      {#if terms}
        {#if request.type === 'checkout'}
          <dt>{t('plan')}</dt>
          <dd data-testid="approval-sku">{terms.sku} · {terms.origin}</dd>
          <dt>{t('price')}</dt>
          <dd>${units(terms.amount_usd_micros, 6)}</dd>
        {/if}
        {#if terms.terms?.method === 'Cash'}
          <dt>{t('pay')}</dt>
          <dd data-testid="approval-amount">
            {units(terms.terms.amount, terms.terms.decimals)} {terms.terms.asset} + {t('fee')} {units(terms.terms.fee_reserve, terms.terms.decimals)}
          </dd>
          <dt>{t('ledger')}</dt>
          <dd class="mono">{terms.terms.ledger}</dd>
          <dt>{t('deadline')}</dt>
          <dd>{when(terms.terms.funding_deadline_ms)}</dd>
        {:else if terms.terms}
          <dt>{t('neuron')}</dt>
          <dd class="mono">{terms.terms.neuron}</dd>
          <dt>{t('stake')}</dt>
          <dd data-testid="approval-amount">{units(terms.terms.required_stake_e8s, 8)} PANDA</dd>
          <dt>{t('deadline')}</dt>
          <dd>{when(terms.terms.application_deadline_ms)}</dd>
        {/if}
        <dt>{t('payer')}</dt>
        <dd class="mono">{terms.actor}</dd>
      {/if}
      {#if request.type === 'account'}
        <dt>{t('change')}</dt>
        <dd data-testid="approval-change">{changes[d.change] ?? d.change}</dd>
        {#if d.principal}<dt>{t('loginPrincipal')}</dt><dd class="mono" data-testid="approval-principal">{d.principal}</dd>{/if}
        {#if d.change === 'policy'}
          <dt>{t('signingPolicy')}</dt>
          <dd>{d.frozen ? t('frozen') : t('formalOn')} · {d.daily_executions} · {d.allowed_purposes?.join(', ')}</dd>
        {/if}
        {#if d.delay_ms}<dt>{t('recoveryDelay')}</dt><dd>{Math.round(d.delay_ms / 86_400_000)}</dd>{/if}
        {#if d.op_id}<dt>{t('recovery')}</dt><dd class="mono">{d.op_id}</dd>{/if}
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
      {#if request.type === 'principal'}
        <dt>{t('method')}</dt>
        <dd data-testid="approval-change">{d.change}</dd>
        {#if d.generation}<dt>{t('controllers')}</dt><dd>#{d.generation}{d.name ? ` · ${d.name}` : ''}</dd>{/if}
        {#if d.delegation}
          <dt>{t('ceiling')}</dt>
          <dd>{d.delegation === 'Unrestricted' ? t('everything') : JSON.stringify(d.delegation)}</dd>
        {/if}
        {#if d.principal_type}<dt>{t('principal')}</dt><dd>{d.principal_type}</dd>{/if}
        {#if d.invalid_from}<dt>{t('deadline')}</dt><dd>{when(d.invalid_from)}</dd>{/if}
      {/if}
      {#if request.type === 'delegation'}
        <dt>{t('controllers')}</dt>
        <dd>#{d.generation}</dd>
        <dt>ID</dt>
        <dd class="mono">{d.kind?.id}</dd>
        {#if d.kind?.type === 'grant'}
          <dt>{t('grantee')}</dt>
          <dd class="mono" data-testid="approval-subject">{d.kind.subject}</dd>
          <dt>{t('scopeLabel')}</dt>
          <dd>{d.kind.scopes.join(', ')}</dd>
          <dt>{t('audienceLabel')}</dt>
          <dd>{d.kind.audiences.join(', ')}</dd>
          <dt>{t('expires')}</dt>
          <dd>{when(d.kind.expires_at)}</dd>
          {#if d.kind.relationship}<dt>{t('subject')}</dt><dd>{d.kind.relationship}</dd>{/if}
        {/if}
      {/if}
      {#if request.type === 'device_revoke'}
        <dt>{t('revoke')}</dt>
        <dd class="mono">{d.device_id}</dd>
      {/if}
    </dl>
    {#if request.type === 'connect'}
      <p class="warn" data-testid="approval-relayed">{t('relayed')}</p>
    {/if}
    {#if request.type === 'delegation'}
      <p class="warn">{t('delegationWarn')}</p>
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
        {#if s.profile === 'app_action'}
          <dt>{t('application')}</dt>
          <dd data-testid="approval-app">{s.content.app_id}</dd>
          <dt>{t('command')}</dt>
          <dd data-testid="approval-command">{s.content.command}</dd>
          {#each s.content.args as arg (arg.name)}<dt class="mono">{arg.name}</dt><dd class="mono">{arg.value}</dd>{/each}
          <dt>{t('operation')}</dt>
          <dd class="mono">{s.content.operation_id}</dd>
          <dt>{t('expires')}</dt>
          <dd>{when(s.content.expires_at)}</dd>
        {/if}
        {#if d.origin && request.type === 'attest'}<dt>{t('origin')}</dt><dd>{d.origin}</dd>{/if}
      </dl>
      {#if s.profile === 'app_action'}<p class="warn">{t('appActionWarn')}</p>{/if}
      {#if s.content?.sha256}
        <p class={d.file_checked ? 'muted' : 'warn'}>
          {d.file ? (d.file_checked ? t('fileChecked') : t('fileChanged')) : t('digestOnly')}
        </p>
      {/if}
    </div>
  {/if}
  {#if unquoted && request.state === 'awaiting_user'}
    <div class="card" data-testid="checkout-quote">
      <p class="warn">{t('quoteFirst')}</p>
      <div class="row">
        {#if d.content?.method === 'Cash'}
          <select bind:value={selection} data-testid="quote-asset">
            {#each assets as a (a.ledger)}<option value={a.ledger}>{a.asset}</option>{/each}
          </select>
        {:else}
          <input placeholder={t('neuronHint')} bind:value={selection} />
        {/if}
        <button disabled={busy || !selection.trim()} onclick={quote}>{t('quote')}</button>
      </div>
    </div>
  {/if}
  {#if error}<p class="error" data-testid="approval-error">{error}</p>{/if}
  {#if request.state === 'awaiting_user'}
    <div class="row">
      <button class="primary" disabled={busy || unquoted} data-testid="approve" onclick={() => decide(true)}>{t('approve')}</button>
      <button disabled={busy} data-testid="reject" onclick={() => decide(false)}>{t('reject')}</button>
    </div>
  {:else}
    <p data-testid="approval-outcome">{t('decided')}: {stateLabel(outcome || request.state)}</p>
  {/if}
{/if}
