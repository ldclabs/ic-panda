<script lang="ts">
  import { t } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'

  /** What a login without a ready workspace does next: create or join an account, pair, or recover. */
  let { status, refresh }: { status: any; refresh: () => Promise<void> } = $props()
  let account = $state('')
  let token = $state('')
  let pairing = $state<any>(null)
  let member = $state(false)
  let recovery = $state<any>(null)
  let error = $state('')
  let busy = $state(false)

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
      await refresh()
    }
  }

  const create = () =>
    attempt(async () => {
      await call('account/create')
    })

  const bindRequest = () =>
    attempt(async () => {
      token = (await call('account/bind_request', { account: account.trim() })).token
    })

  const bindAccept = () =>
    attempt(async () => {
      await call('account/bind_accept', { account: account.trim() })
      token = ''
    })

  const pair = () =>
    attempt(async () => {
      pairing = await call('device/pair_request', { role: member ? 'member' : 'administrator' })
    })

  /** An approved device joins on the next unlock with the login. */
  const joined = () =>
    attempt(async () => {
      await call('session/unlock')
    })

  const recoveryCall = (method: string) =>
    attempt(async () => {
      const value = await call(method)
      if (method !== 'recovery/complete') recovery = value
    })
</script>

{#if error}<p class="error" data-testid="onboarding-error">{error}</p>{/if}
{#if status?.state === 'no_account'}
  <div class="card" data-testid="onboarding">
    <h2>{t('noAccount')}</h2>
    <p class="muted">{t('loginPrincipal')}: <span class="mono">{status.principal}</span></p>
    <button class="primary" disabled={busy} data-testid="create-account" onclick={create}>{t('createAccount')}</button>
    <h2 style="margin-top: 16px">{t('joinAccount')}</h2>
    <p class="muted">{t('joinAccountHint')}</p>
    <div class="row">
      <input placeholder={t('accountId')} bind:value={account} data-testid="bind-account" />
      <button disabled={busy || !account.trim()} onclick={bindRequest}>{t('bindRequest')}</button>
    </div>
    {#if token}
      <p class="mono" data-testid="bind-token" style="word-break: break-all">{token}</p>
      <button class="primary" disabled={busy} onclick={bindAccept}>{t('bindAccept')}</button>
    {/if}
  </div>
{:else if status?.state === 'pairing_required'}
  <div class="card" data-testid="onboarding">
    <h2>{t('pairingRequired')}</h2>
    <p class="muted">{t('account')}: <span class="mono">{status.account ?? '-'}</span> · {t('device')}: <span class="mono">{status.device_id}</span></p>
    <label class="row"><input type="checkbox" bind:checked={member} /> {t('memberRole')}</label>
    <div class="row" style="margin-top: 8px">
      <button class="primary" disabled={busy} data-testid="pair-request" onclick={pair}>{t('pairRequest')}</button>
      <button disabled={busy} onclick={joined}>{t('checkApproval')}</button>
    </div>
    {#if pairing}
      <p class="muted">{t('pairHint')}</p>
      <p class="mono" data-testid="pair-token" style="word-break: break-all">{pairing.token}</p>
      <p>{t('device')}: <strong class="mono">{pairing.device_id}</strong></p>
    {/if}
  </div>
{/if}
{#if ['pairing_required', 'root_required'].includes(status?.state)}
  <div class="card">
    <h2>{t('recovery')}</h2>
    <p class="muted">{t('recoveryHint')}</p>
    <div class="row">
      <button disabled={busy} onclick={() => recoveryCall('recovery/status')}>{t('recoveryStatus')}</button>
      {#if status?.state === 'pairing_required'}
        <button disabled={busy} data-testid="recovery-request" onclick={() => recoveryCall('recovery/request')}>{t('requestRecovery')}</button>
      {/if}
      <button class="primary" disabled={busy} onclick={() => recoveryCall('recovery/complete')}>{t('completeRecovery')}</button>
    </div>
    {#if recovery}
      {#if recovery.pending}
        <p data-testid="recovery-pending">
          {t('pendingRecovery')} · {t('loginPrincipal')} <span class="mono">{recovery.pending.new_auth}</span>
          · {t('device')} <span class="mono">{recovery.pending.device_id.slice(0, 12)}</span>{recovery.pending.this_device ? ` (${t('thisDevice')})` : ''}
          · {t('executeAfter')} {when(recovery.pending.execute_after)}
        </p>
      {:else}
        <p class="muted">{t('noRecovery')}</p>
      {/if}
    {/if}
  </div>
{/if}
