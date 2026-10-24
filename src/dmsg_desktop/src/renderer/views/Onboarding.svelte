<script lang="ts">
  import Checkbox from '../components/Checkbox.svelte'
  import Icon from '../components/Icon.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { call, errorText, when } from '../rpc'

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
      error = errorText(e)
    } finally {
      busy = false
      await refresh()
    }
  }

  const create = () => attempt(async () => void (await call('account/create')))

  const bindRequest = () =>
    attempt(async () => {
      token = ((await call('account/bind_request', { account: account.trim() })) as any).token
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
  const joined = () => attempt(async () => void (await call('session/unlock')))

  const recoveryCall = (method: string) =>
    attempt(async () => {
      const value = await call(method)
      if (method !== 'recovery/complete') recovery = value
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
      <div class="grid gap-2 rounded-lg bg-surface-2/60 p-3.5">
        <b class="text-[13px] font-semibold">{t('joinAccount')}</b>
        <p class="text-[12.5px] text-muted">{t('joinAccountHint')}</p>
        <div class="flex gap-2">
          <input placeholder={t('accountId')} bind:value={account} class="font-mono text-[12.5px] placeholder:font-sans" data-testid="bind-account" />
          <button class={buttonClass('secondary')} disabled={busy || !account.trim()} onclick={bindRequest}>{t('bindRequest')}</button>
        </div>
        {#if token}
          <p class="rounded-lg bg-surface px-2.5 py-2 font-mono text-[11.5px] break-all select-all" data-testid="bind-token">{token}</p>
          <button class={buttonClass('primary', 'md', 'justify-self-start')} disabled={busy} onclick={bindAccept}>{t('bindAccept')}</button>
        {/if}
      </div>
    </div>
  </Section>
{:else if status?.state === 'pairing_required'}
  <Section title={t('pairingRequired')} icon="pair" testid="onboarding">
    <div class="grid gap-3">
      <div class="flex flex-wrap items-center gap-1.5 text-[12.5px] text-muted">
        {t('account')} {#if status.account}<Id id={status.account} />{:else}-{/if} · {t('device')} <Id id={status.device_id} />
      </div>
      <Checkbox bind:checked={member}>{t('memberRole')}</Checkbox>
      <div class="flex flex-wrap gap-2">
        <button class={buttonClass('primary')} disabled={busy} data-testid="pair-request" onclick={pair}>{t('pairRequest')}</button>
        <button class={buttonClass('secondary')} disabled={busy} onclick={joined}>{t('checkApproval')}</button>
      </div>
      {#if pairing}
        <Notice>
          <p>{t('pairHint')}</p>
          <p class="mt-1.5 font-mono text-[11.5px] break-all select-all" data-testid="pair-token">{pairing.token}</p>
          <p class="mt-1.5">{t('device')}: <b class="font-mono text-[12px]">{pairing.device_id}</b></p>
        </Notice>
      {/if}
    </div>
  </Section>
{/if}
{#if ['pairing_required', 'root_required'].includes(status?.state)}
  <Section title={t('recovery')} description={t('recoveryHint')} icon="recovery">
    <div class="grid gap-3">
      <div class="flex flex-wrap gap-2">
        <button class={buttonClass('secondary')} disabled={busy} onclick={() => recoveryCall('recovery/status')}>{t('recoveryStatus')}</button>
        {#if status?.state === 'pairing_required'}
          <button class={buttonClass('secondary')} disabled={busy} data-testid="recovery-request" onclick={() => recoveryCall('recovery/request')}>{t('requestRecovery')}</button>
        {/if}
        <button class={buttonClass('primary')} disabled={busy} onclick={() => recoveryCall('recovery/complete')}>{t('completeRecovery')}</button>
      </div>
      {#if recovery}
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
