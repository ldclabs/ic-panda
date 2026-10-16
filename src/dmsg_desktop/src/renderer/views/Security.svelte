<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'
  import Checkbox from '../components/Checkbox.svelte'
  import Segmented from '../components/Segmented.svelte'
  import { buttonClass } from '../components/ui'

  /** Bound logins, approvals of other devices and logins, the signing policy and recovery. */
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

  /** Each change is an approval request; the approval window decides it. */
  async function ask(method: string, params: unknown) {
    busy = true
    error = ''
    try {
      const created = await call(method, params)
      if (created?.id) await window.dmsg.openApproval(created.id)
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  /** A pairing request carries a device ID; a binding request a login. */
  async function approve() {
    busy = true
    error = ''
    try {
      if (pairing) {
        await call('device/approve', { token: request.trim(), device: shown.trim() })
      } else {
        const created = await call('account/bind_approve', { token: request.trim(), principal: shown.trim() })
        await window.dmsg.openApproval(created.id)
      }
      request = ''
      shown = ''
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
      await load()
      await refresh()
    }
  }

  /** The second login happens in the browser; its approval request opens when it arrives. */
  async function bindLogin() {
    busy = true
    error = ''
    try {
      const started: any = await window.dmsg.login('https://dmsg.net', true)
      while (Date.now() < started.expires_at) {
        await new Promise((resolve) => setTimeout(resolve, 1000))
        const result = await call('session/login_result')
        if (result.pending) continue
        if (result.error) error = `${t('error')}: ${result.error}`
        if (result.approval) await window.dmsg.openApproval(result.approval.id)
        break
      }
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  /** A device ID names a pairing request; anything else a login. */
  const pairing = $derived(/^[0-9a-f]{64}$/.test(shown.trim()))

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['approval/changed', 'session/changed'].includes(event.type)) void load()
    })
  })
</script>

{#if security}
  <div class="card" data-testid="security">
    <h2>{t('security')}</h2>
    {#if error}<p class="error">{error}</p>{/if}
    <h3>{t('logins')}</h3>
    <ul class="list" data-testid="logins">
      {#each security.logins as login (login.principal)}
        <li class="row spread">
          <span class="mono">{login.principal}{login.current ? ` (${t('current')})` : ''}</span>
          {#if !login.current && security.logins.length > 1}
            <button class={buttonClass('danger')} disabled={busy} onclick={() => ask('account/unbind', { principal: login.principal })}>{t('remove')}</button>
          {/if}
        </li>
      {/each}
    </ul>
    <p class="muted">{t('bindLoginHint')}</p>
    <button class={buttonClass()} disabled={busy} data-testid="bind-login" onclick={bindLogin}>{t('bindLogin')}</button>

    <h3>{t('approveOthers')}</h3>
    <textarea placeholder={t('pasteRequest')} bind:value={request} data-testid="approve-request"></textarea>
    <div class="row" style="margin-top: 8px">
      <input placeholder={`${t('deviceShown')} / ${t('principalShown')}`} bind:value={shown} data-testid="approve-shown" />
      <button class={buttonClass('primary')} disabled={busy || !request.trim() || !shown.trim()} onclick={approve}>
        {pairing ? t('approveDevice') : t('approveLogin')}
      </button>
    </div>

    <h3>{t('signingPolicy')}</h3>
    <p>{security.policy.frozen ? t('frozen') : t('formalOn')}</p>
    <div class="row">
      <button class={buttonClass()} disabled={busy} onclick={() => ask('account/policy', { frozen: !security.policy.frozen })}>
        {security.policy.frozen ? t('unfreeze') : t('freeze')}
      </button>
      <input style="max-width: 90px" bind:value={daily} aria-label={t('dailyExecutions')} />
      <button class={buttonClass()} disabled={busy || !/^\d+$/.test(daily)} onclick={() => ask('account/policy', { daily_executions: Number(daily) })}>{t('dailyExecutions')}</button>
    </div>
    <div class="row" style="margin-top: 8px">
      <span class="muted">{t('purposes')}</span>
      {#each ['FileAttestation', 'Statement', 'AppAction'] as p (p)}
        <Checkbox checked={purposes.includes(p)} onCheckedChange={(on) => (purposes = on ? [...purposes, p] : purposes.filter((x) => x !== p))}
          >{p}</Checkbox
        >
      {/each}
      <button class={buttonClass()} disabled={busy} onclick={() => ask('account/policy', { allowed_purposes: purposes })}>{t('submit')}</button>
    </div>

    <h3>{t('recovery')}</h3>
    <div class="row">
      <Segmented
        bind:value={days}
        label={t('recoveryDelay')}
        options={['1', '2', '3', '4', '5', '6', '7'].map((d) => ({ value: d, label: d }))}
        testid="recovery-days"
      />
      <button class={buttonClass()} disabled={busy || !/^[1-7]$/.test(days)} onclick={() => ask('account/recovery_delay', { days: Number(days) })}>{t('recoveryDelay')}</button>
    </div>
    {#if security.pending_recovery}
      <p class="warn" data-testid="pending-recovery">
        {t('pendingRecovery')}: {t('loginPrincipal')} <span class="mono">{security.pending_recovery.new_auth}</span>
        · {t('executeAfter')} {when(security.pending_recovery.execute_after)}
      </p>
      <button class={buttonClass('danger')} disabled={busy} onclick={() => ask('recovery/dispute', {})}>{t('disputeRecovery')}</button>
    {:else}
      <p class="muted">{t('noRecovery')}</p>
    {/if}
  </div>
{:else if status?.state === 'unlocked' && error}
  <p class="error">{error}</p>
{/if}
