<script lang="ts">
  import TextArea from '../components/TextArea.svelte'
  import TextField from '../components/TextField.svelte'
  import { onDestroy, onMount } from 'svelte'
  import Collapsible from '../components/Collapsible.svelte'

  import Checkbox from '../components/Checkbox.svelte'
  import Icon from '../components/Icon.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { copyText } from '../components/clipboard'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { call, errorText, onChange, when } from '../rpc'

  /** What a login without a ready workspace does next: create or join an account, pair, or recover. */
  let { status, refresh }: { status: any; refresh: () => Promise<void> } = $props()

  let account = $state('')
  let token = $state('')
  let pairing = $state<any>(null)
  let member = $state(false)
  let predecessor = $state('')
  let succession = $state<any>(null)
  let recovery = $state<any>(null)
  let error = $state('')
  let busy = $state(false)
  let alive = true
  let now = $state(Date.now())
  const recoverable = $derived(status?.state === 'pairing_required' || (status?.state === 'root_required' && !status.setup_pending))
  const canComplete = $derived(!!recovery && (recovery.ready || (recovery.pending?.this_device && now >= recovery.pending.execute_after) || (status?.state === 'root_required' && !status.setup_pending && recovery.member)))
  onDestroy(() => { alive = false })

  async function attempt(work: () => Promise<void>) {
    if (busy || !alive) return
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      if (alive) { busy = false; await refresh() }
    }
  }

  const create = () => attempt(async () => void (await call('account/create')))

  const bindRequest = () =>
    attempt(async () => {
      const target = account.trim()
      const result: any = await call('account/bind_request', { account: target })
      if (alive && target === account.trim()) token = result.token
    })

  const bindAccept = () =>
    attempt(async () => {
      await call('account/bind_accept', { account: account.trim() })
      token = ''
    })

  const pair = () =>
    attempt(async () => {
      const value = await call('device/pair_request', { role: member ? 'member' : 'administrator' })
      if (alive) pairing = value
    })

  /** Ask to replace an administrator device: it approves this one in its place. */
  const succeed = () =>
    attempt(async () => {
      const value = await call('device/succession_request', { predecessor: predecessor.trim().toLowerCase() })
      if (alive) succession = value
    })

  /** An approved device joins on the next unlock with the login. */
  const joined = () => attempt(async () => void (await call('session/unlock')))

  const recoveryCall = (method: string) =>
    attempt(async () => {
      if (method === 'recovery/complete' && !canComplete) return
      if (method === 'recovery/request' && (!recovery || recovery.enabled === false || recovery.pending)) return
      const value = await call(method)
      if (alive && method !== 'recovery/complete') recovery = value
    })

  onMount(() => {
    if (recoverable) void recoveryCall('recovery/status')
    const off = onChange(['recovery/changed'], () => { if (recoverable) void recoveryCall('recovery/status') })
    const timer = setInterval(() => { now = Date.now() }, 1000)
    return () => { clearInterval(timer); off() }
  })
</script>

{#if error}<Notice failure={error} tone="danger" testid="onboarding-error">{error}</Notice>{/if}
{#if status?.state === 'no_account'}
  <Section title={t('noAccount')} icon="add" testid="onboarding">
    <div class="grid gap-4">
      <div class="flex flex-wrap items-center gap-2 text-[12.5px] text-muted">{t('loginPrincipal')} <Id id={status.principal} full /></div>
      <button class={buttonClass('primary', 'lg', 'justify-self-start')} disabled={busy} data-testid="create-account" onclick={create}>
        <Icon name="add" />{t('createAccount')}
      </button>
      <Collapsible testid="join-account">
        {#snippet title()}<b class="text-[13px] font-semibold">{t('joinAccount')}</b>{/snippet}
        <div class="grid gap-2 py-2">
        <p class="text-[12.5px] text-muted">{t('joinAccountHint')}</p>
        <div class="flex flex-wrap items-end gap-2">
          <TextField label={t('accountId')} bind:value={account} oninput={() => (token = '')} mono disabled={busy} class="min-w-48 flex-1" data-testid="bind-account" />
          <button class={buttonClass('secondary')} disabled={busy || !account.trim() || !!token} onclick={bindRequest}>{t('bindRequest')}</button>
        </div>
        {#if token}
          <p class="rounded-lg bg-surface px-2.5 py-2 font-mono text-[11.5px] break-all select-all" data-testid="bind-token">{token}</p>
          <button class={buttonClass('primary', 'md', 'justify-self-start')} disabled={busy} onclick={bindAccept}>{t('bindAccept')}</button>
        {/if}
        </div>
      </Collapsible>
    </div>
  </Section>
{:else if status?.state === 'pairing_required'}
  <Section title={t('pairingRequired')} icon="pair" testid="onboarding">
    <div class="grid gap-3">
      <div class="flex flex-wrap items-center gap-1.5 text-[12.5px] text-muted">
        {t('account')} {#if status.account}<Id id={status.account} />{:else}-{/if} · {t('device')} <Id id={status.device_id} />
      </div>
      <Checkbox bind:checked={member} disabled={busy} onCheckedChange={() => (pairing = null)}>{t('memberRole')}</Checkbox>
      <div class="flex flex-wrap gap-2">
        <button class={buttonClass('primary')} disabled={busy || (!!pairing && (!pairing.expires_at || pairing.expires_at > now))} data-testid="pair-request" onclick={pair}>{t('pairRequest')}</button>
        <button class={buttonClass('secondary')} disabled={busy} onclick={joined}>{t('checkApproval')}</button>
      </div>
      {#if pairing}
        <Notice>
          <p>{t('pairHint')}</p>
          {#if pairing.qr}
            <img src={pairing.qr} alt={t('pairRequest')} width="224" height="224" class="mt-2 rounded-lg bg-white p-2" data-testid="pair-qr" />
          {/if}
          <p class="mt-1.5 font-mono text-[11.5px] break-all select-all" data-testid="pair-token">{pairing.token}</p>
          <p class="mt-1.5">{t('device')}: <b class="font-mono text-[12px]">{pairing.device_id}</b></p>
        </Notice>
      {/if}
      <Collapsible testid="succession-request">
        {#snippet title()}<b class="text-[13px] font-semibold">{t('successionTitle')}</b>{/snippet}
        <div class="grid gap-2 py-2">
          <p class="text-[12.5px] text-muted">{t('successionRequestHint')}</p>
          <TextField label={t('successionPredecessor')} hint={t('successionPredecessorHint')} bind:value={predecessor} oninput={() => (succession = null)} mono disabled={busy} data-testid="succession-predecessor" />
          <button class={buttonClass('secondary', 'md', 'justify-self-start')} disabled={busy || !/^[0-9a-f]{64}$/.test(predecessor.trim().toLowerCase()) || !!succession} onclick={succeed} data-testid="succession-request-create">{t('successionRequest')}</button>
          {#if succession}
            <Notice testid="succession-request-made">
              <p>{t('successionRequestShown')}</p>
              {#if succession.qr}
                <img src={succession.qr} alt={t('requestToken')} width="224" height="224" class="mt-2 rounded-lg bg-white p-2" data-testid="succession-qr" />
              {/if}
              <p class="mt-1.5">{t('device')}: <b class="font-mono text-[12px]">{succession.device_id}</b></p>
              <p class="mt-1.5 flex flex-wrap gap-1">
                <Pill tone={succession.approval_key ? 'ok' : 'neutral'}>{t('successionApprovalKey')}: {succession.approval_key ? t('successionMade') : t('successionNotMade')}</Pill>
                <Pill tone={succession.cosigner ? 'ok' : 'neutral'}>{t('successionCosigner')}: {succession.cosigner ? t('successionMade') : t('successionNotMade')}</Pill>
              </p>
            </Notice>
            <TextArea label={t('requestToken')} readonly rows={3} value={succession.token} mono data-testid="succession-token-made" />
            <div class="flex flex-wrap gap-2">
              <button class={buttonClass('secondary')} onclick={() => copyText(succession.token)} data-testid="succession-token-copy"><Icon name="copy" />{t('copy')}</button>
              <button class={buttonClass('primary')} disabled={busy} onclick={joined}>{t('checkApproval')}</button>
            </div>
          {/if}
        </div>
      </Collapsible>
    </div>
  </Section>
{:else if status?.state === 'root_required'}
  <Section title={t('accountSetup')} description={t(status.setup_pending ? 'accountSetupHint' : 'accountOpenHint')} icon="account" testid="account-setup">
    <button class={buttonClass('primary')} disabled={busy} onclick={status.setup_pending ? create : joined} data-testid="retry-account-setup">
      {t(status.setup_pending ? 'retryAccountSetup' : 'checkApproval')}
    </button>
  </Section>
{/if}
{#if status?.state === 'pairing_required' || (status?.state === 'root_required' && !status.setup_pending)}
  <Section title={t('recovery')} description={t('recoveryHint')} icon="recovery">
    <div class="grid gap-3">
      <div class="flex flex-wrap gap-2">
        <button class={buttonClass('secondary')} disabled={busy} onclick={() => recoveryCall('recovery/status')}>{t('recoveryStatus')}</button>
        {#if status?.state === 'pairing_required'}
          <button class={buttonClass('secondary')} disabled={busy || !recovery || recovery.enabled === false || !!recovery.pending} data-testid="recovery-request" onclick={() => recoveryCall('recovery/request')}>{t('requestRecovery')}</button>
        {/if}
        <button class={buttonClass('primary')} disabled={busy || !canComplete} onclick={() => recoveryCall('recovery/complete')} data-testid="recovery-complete">{t('completeRecovery')}</button>
      </div>
      {#if recovery}
        {#if recovery.enabled === false}<Notice>{t('recoveryOffHint')}</Notice>{/if}
        {#if recovery.pending}
          <Notice tone="warn" testid="recovery-pending">
            {t('pendingRecovery')} · {t('loginPrincipal')} <span class="font-mono">{recovery.pending.new_auth}</span> · {t('device')}
            <span class="font-mono">{recovery.pending.device_id.slice(0, 12)}</span>{recovery.pending.this_device ? ` (${t('thisDevice')})` : ''} ·
            {t('executeAfter')} {when(recovery.pending.execute_after)}
          </Notice>
        {:else}
          <p class="text-[12.5px] text-faint">{t('noRecovery')}</p>
        {/if}
      {/if}
    </div>
  </Section>
{/if}
