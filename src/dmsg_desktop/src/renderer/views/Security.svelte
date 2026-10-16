<script lang="ts">
  import { onMount } from 'svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import Icon from '../components/Icon.svelte'
  import Field from '../components/page/Field.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import Segmented from '../components/Segmented.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, when } from '../rpc'

  /** Bound logins, approvals of other devices and logins, the signing policy and recovery. Every change is an approval request. */
  let { status, refresh }: { status: any; refresh: () => Promise<void> } = $props()

  let security = $state<any>(null)
  let request = $state('')
  let shown = $state('')
  let daily = $state('')
  let purposes = $state<string[]>([])
  let days = $state('')
  let error = $state('')
  let busy = $state(false)

  async function load() {
    try {
      security = await call('account/security')
      daily = String(security.policy.daily_executions)
      purposes = [...security.policy.allowed_purposes]
      days = String(Math.round(security.recovery_delay_ms / 86_400_000))
    } catch (e) {
      security = null
      error = code(e) === 'AUTH_REQUIRED' ? '' : `${t('error')}: ${code(e)}`
    }
  }

  async function guard(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  /** Each change is an approval request; the approval window decides it. */
  const ask = (method: string, params: unknown) =>
    guard(async () => {
      const created: any = await call(method, params)
      if (created?.id) await bridge(() => window.dmsg.openApproval(created.id))
    })

  /** A pairing request carries a device ID; a binding request a login. */
  async function approve() {
    await guard(async () => {
      if (pairing) {
        await call('device/approve', { token: request.trim(), device: shown.trim() })
      } else {
        const created: any = await call('account/bind_approve', { token: request.trim(), principal: shown.trim() })
        await bridge(() => window.dmsg.openApproval(created.id))
      }
      request = ''
      shown = ''
    })
    await load()
    await refresh()
  }

  /** The second login happens in the browser; its approval request opens when it arrives. */
  const bindLogin = () =>
    guard(async () => {
      const started: any = await bridge(() => window.dmsg.login('https://dmsg.net', true))
      while (Date.now() < started.expires_at) {
        await new Promise((resolve) => setTimeout(resolve, 1000))
        const result: any = await call('session/login_result')
        if (result.pending) continue
        if (result.error) error = `${t('error')}: ${result.error}`
        if (result.approval) await bridge(() => window.dmsg.openApproval(result.approval.id))
        break
      }
    })

  /** A device ID names a pairing request; anything else a login. */
  const pairing = $derived(/^[0-9a-f]{64}$/.test(shown.trim()))

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['approval/changed', 'session/changed'].includes(event.type)) void load()
    })
  })
</script>

{#if error}<Notice tone="danger">{error}</Notice>{/if}
{#if security}
  <Section title={t('logins')} description={t('bindLoginHint')} icon="login" flush testid="security">
    {#snippet actions()}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} data-testid="bind-login" onclick={bindLogin}><Icon name="add" />{t('bindLogin')}</button>
    {/snippet}
    <ul class="divide-y divide-line border-t border-line" data-testid="logins">
      {#each security.logins as login (login.principal)}
        <li class="flex items-center gap-2 px-5 py-3">
          <Id id={login.principal} full class="max-w-[70%]" />
          {#if login.current}<Pill tone="forest">{t('current')}</Pill>{/if}
          <span class="flex-1"></span>
          {#if !login.current && security.logins.length > 1}
            <button class={buttonClass('danger', 'xs')} disabled={busy} onclick={() => ask('account/unbind', { principal: login.principal })}>{t('remove')}</button>
          {/if}
        </li>
      {/each}
    </ul>
  </Section>

  <Section title={t('approveOthers')} description={t('approveOthersHint')} icon="pair">
    <div class="grid gap-2.5">
      <Field label={t('pasteRequest')}><textarea bind:value={request} rows="3" class="font-mono text-[11.5px]" data-testid="approve-request"></textarea></Field>
      <div class="flex flex-wrap items-end gap-2">
        <Field label={`${t('deviceShown')} / ${t('principalShown')}`} class="min-w-60 flex-1">
          <input bind:value={shown} class="font-mono text-[12.5px]" data-testid="approve-shown" />
        </Field>
        <button class={buttonClass('primary')} disabled={busy || !request.trim() || !shown.trim()} onclick={approve}>
          {pairing ? t('approveDevice') : t('approveLogin')}
        </button>
      </div>
    </div>
  </Section>

  <Section title={t('signingPolicy')} description={t('signingPolicyHint')} icon="signature">
    {#snippet actions()}
      <Pill tone={security.policy.frozen ? 'warn' : 'ok'}>{security.policy.frozen ? t('frozen') : t('formalOn')}</Pill>
    {/snippet}
    <div class="grid gap-4">
      <div class="flex flex-wrap items-end gap-2">
        <Field label={t('dailyExecutions')}><input bind:value={daily} inputmode="numeric" class="w-28 tabular-nums" /></Field>
        <button class={buttonClass('secondary')} disabled={busy || !/^\d+$/.test(daily)} onclick={() => ask('account/policy', { daily_executions: Number(daily) })}>
          {t('save')}
        </button>
        <span class="flex-1"></span>
        <button class={buttonClass(security.policy.frozen ? 'primary' : 'danger')} disabled={busy} onclick={() => ask('account/policy', { frozen: !security.policy.frozen })}>
          {security.policy.frozen ? t('unfreeze') : t('freeze')}
        </button>
      </div>
      <div class="grid gap-2">
        <span class="text-[12px] text-faint">{t('purposes')}</span>
        <div class="flex flex-wrap items-center gap-x-5 gap-y-2">
          {#each ['FileAttestation', 'Statement', 'AppAction'] as p (p)}
            <Checkbox checked={purposes.includes(p)} onCheckedChange={(on) => (purposes = on ? [...purposes, p] : purposes.filter((x) => x !== p))}>{p}</Checkbox>
          {/each}
          <span class="flex-1"></span>
          <button class={buttonClass('secondary')} disabled={busy} onclick={() => ask('account/policy', { allowed_purposes: purposes })}>{t('submit')}</button>
        </div>
      </div>
    </div>
  </Section>

  <Section title={t('recovery')} description={t('recoveryDelayHint')} icon="recovery">
    <div class="grid gap-3">
      <div class="flex flex-wrap items-end gap-2">
        <Field label={t('recoveryDays')}>
          <Segmented bind:value={days} label={t('recoveryDelay')} options={['1', '2', '3', '4', '5', '6', '7'].map((d) => ({ value: d, label: d }))} testid="recovery-days" />
        </Field>
        <button class={buttonClass('secondary')} disabled={busy || !/^[1-7]$/.test(days)} onclick={() => ask('account/recovery_delay', { days: Number(days) })}>
          {t('recoveryDelay')}
        </button>
      </div>
      {#if security.pending_recovery}
        <Notice tone="warn" testid="pending-recovery">
          {t('pendingRecovery')}: {t('loginPrincipal')} <span class="font-mono">{security.pending_recovery.new_auth}</span> · {t('executeAfter')}
          {when(security.pending_recovery.execute_after)}
          {#snippet action()}
            <button class={buttonClass('danger', 'xs')} disabled={busy} onclick={() => ask('recovery/dispute', {})}>{t('disputeRecovery')}</button>
          {/snippet}
        </Notice>
      {:else}
        <p class="flex items-center gap-1.5 text-[12.5px] text-faint"><Icon name="check" class="text-ok" />{t('noRecovery')}</p>
      {/if}
    </div>
  </Section>
{/if}
