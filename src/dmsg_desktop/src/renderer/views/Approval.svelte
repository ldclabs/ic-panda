<script lang="ts">
  import { onMount } from 'svelte'
  import { t, stateLabel } from '../i18n'
  import { call, code, when } from '../rpc'

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
    {request.type === 'connect' ? d.app_id : request.type === 'attest' ? t('newStatement') : request.type}
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
