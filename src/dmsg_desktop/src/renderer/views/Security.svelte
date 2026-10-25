<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
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
  import { bridge, call, code, errorText, onChange, single, when } from '../rpc'
  import { startLogin } from '../shell/login.svelte'
  import { hardwarePurposeLabels, purposeLabels } from '../approval'

  /**
   * Bound logins, approvals of other devices and logins, the signing
   * policy, approval keys, device-bound unlock and recovery. Every account
   * change is an approval request.
   */
  let alive = true
  onDestroy(() => { alive = false })
  let security = $state<any>(null)
  let request = $state('')
  let shown = $state('')
  let daily = $state('')
  let purposes = $state<string[]>([])
  let days = $state('')
  let error = $state('')
  let busy = $state(false)
  /** Unlocked without a login, as a fast unlock without Touch ID is; the login opens the page. */
  let needsLogin = $state(false)
  /** This device's approval key, every device's and the purposes they are required for. */
  let keys = $state<any>(null)
  let biometryOnly = $state(false)
  let extra = $state<string[]>([])
  /** A key request this device made for another device to approve. */
  let made = $state<any>(null)
  let keyRequest = $state('')
  let keyDevice = $state('')
  /** What binding to the hardware left uncovered. */
  let unprotected = $state<any>(null)
  const OPTIONAL = ['payment', 'attestation']
  const CONTROL = ['account_control', 'device_control', 'principal_control']

  // A field follows the account only when the account's value changed, so an edit under way stays.
  const reload = single(async () => {
    if (!alive) return
    try {
      const next: any = await call('account/security')
      if (!alive) return
      error = ''
      if (next.policy.daily_executions !== security?.policy.daily_executions) daily = String(next.policy.daily_executions)
      if (String(next.policy.allowed_purposes) !== String(security?.policy.allowed_purposes)) purposes = [...next.policy.allowed_purposes]
      if (next.recovery_delay_ms !== security?.recovery_delay_ms) days = String(Math.round(next.recovery_delay_ms / 86_400_000))
      security = next
      needsLogin = false
      const key: any = await call('device/key').catch(() => null)
      if (!alive) return
      if (String(key?.policy) !== String(keys?.policy)) extra = (key?.policy ?? []).filter((p: string) => OPTIONAL.includes(p))
      keys = key
    } catch (e) {
      if (!alive) return
      security = null
      needsLogin = code(e) === 'AUTH_REQUIRED'
      error = needsLogin ? '' : errorText(e)
    }
  })

  async function guard(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      if (!alive) return
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  /** Each change is an approval request; the approval window decides it. */
  const ask = (method: string, params: unknown) =>
    guard(async () => {
      const created: any = await call(method, params)
      if (!alive) return
      if (created?.id) await bridge(() => window.dmsg.openApproval(created.id))
    })

  /** A pairing request carries a device ID; a binding request a login. Either becomes an approval request the approval window decides. */
  const approve = () =>
    guard(async () => {
      const created: any = pairing
        ? await call('device/approve', { token: request.trim(), device: shown.trim() })
        : await call('account/bind_approve', { token: request.trim(), principal: shown.trim() })
      if (!alive) return
      await bridge(() => window.dmsg.openApproval(created.id))
      request = ''
      shown = ''
    })

  /** A key made here for a device that has one to register; this device asks for Touch ID now. */
  const requestKey = () =>
    guard(async () => {
      made = await call('device/key_request', { biometry_only: biometryOnly })
    })

  /** Binding to the hardware rewraps the local data key; Touch ID is asked once. */
  const bind = (enable: boolean) =>
    guard(async () => {
      const answer: any = await call('session/device_bound', { enable, biometry_only: biometryOnly })
      if (!alive) return
      unprotected = enable ? answer.unprotected : null
      await reload()
    })

  const purposeText = (list: string[]) => list.map((p) => (hardwarePurposeLabels[p] ? t(hardwarePurposeLabels[p]) : p)).join(', ')
  const required = $derived((keys?.policy ?? []).length > 0)
  const own = $derived(keys?.key ?? null)
  const missing = $derived((keys?.devices ?? []).filter((d: any) => !d.key_id))

  /** The second login happens in the browser, followed by the login dialog; its approval request opens when it arrives. */
  const bindLogin = () => startLogin(true)

  /** A device ID names a pairing request; anything else a login. */
  const pairing = $derived(/^[0-9a-f]{64}$/.test(shown.trim()))

  onMount(() => {
    void reload()
    return onChange(['approval/changed', 'session/changed'], reload)
  })
</script>

{#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
{#if needsLogin}
  <Notice icon="login" testid="security-needs-login">
    {t('securityNeedsLogin')}
    {#snippet action()}
      <button class={buttonClass('primary', 'sm')} onclick={() => startLogin()} data-testid="security-login"><Icon name="login" />{t('loginWithIi')}</button>
    {/snippet}
  </Notice>
{/if}
{#if security}
  <Section title={t('logins')} description={t('bindLoginHint')} icon="login" flush testid="security">
    {#snippet actions()}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} data-testid="bind-login" onclick={bindLogin}><Icon name="add" />{t('bindLogin')}</button>
    {/snippet}
    <ul class="divide-y divide-line" data-testid="logins">
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
      <Pill tone={security.policy.frozen ? 'warn' : 'ok'}>{security.policy.frozen ? t('frozen') : t('policyOpen')}</Pill>
    {/snippet}
    <div class="grid gap-4">
      <div class="flex flex-wrap items-end gap-2">
        <Field label={t('dailyExecutions')}><input bind:value={daily} inputmode="numeric" class="w-28 tabular-nums" data-testid="daily-executions" /></Field>
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
            <Checkbox checked={purposes.includes(p)} onCheckedChange={(on) => (purposes = on ? [...purposes, p] : purposes.filter((x) => x !== p))}>{t(purposeLabels[p])}</Checkbox>
          {/each}
          <span class="flex-1"></span>
          <button class={buttonClass('secondary')} disabled={busy} onclick={() => ask('account/policy', { allowed_purposes: purposes })}>{t('submit')}</button>
        </div>
      </div>
    </div>
  </Section>

  {#if keys}
    <Section title={t('approvalKeys')} description={t('approvalKeysHint')} icon="key" testid="approval-keys">
      {#snippet actions()}
        <Pill tone={required ? 'forest' : 'neutral'}>{required ? t('keysRequired') : t('keysNotRequired')}</Pill>
      {/snippet}
      <div class="grid gap-4">
        {#if !keys.available}
          <Notice testid="approval-keys-unavailable">{t('approvalKeysUnavailable')}</Notice>
        {/if}
        <div class="flex flex-wrap items-center gap-2" data-testid="approval-key-own">
          <span class="text-[12.5px] text-faint">{t('thisDevice')}</span>
          {#if own?.fingerprints_changed}
            <Pill tone="danger">{t('fingerprintsChanged')}</Pill>
          {:else if own?.here}
            <Pill tone="ok">{t('keyRegistered')}</Pill>
            {#if own.biometry_only}<Pill tone="neutral">{t('touchIdOnly')}</Pill>{/if}
          {:else if own}
            <Pill tone="warn">{t('keyElsewhere')}</Pill>
          {:else}
            <Pill tone="neutral">{t('keyNotRegistered')}</Pill>
          {/if}
          <span class="flex-1"></span>
          {#if keys.available}
            {#if keys.biometry}<Checkbox checked={biometryOnly} onCheckedChange={(on) => (biometryOnly = on)}>{t('biometryOnly')}</Checkbox>{/if}
            <button class={buttonClass('secondary', 'sm')} disabled={busy} data-testid="approval-key-register" onclick={() => ask('device/key_register', { biometry_only: biometryOnly })}>
              <Icon name="key" />{own ? t('replaceKey') : t('registerKey')}
            </button>
          {/if}
          {#if own}
            <button class={buttonClass('danger', 'sm')} disabled={busy} onclick={() => ask('device/key_retire', {})}>{t('retireKey')}</button>
          {/if}
        </div>
        <div class="grid gap-2">
          <span class="text-[12px] text-faint">{t('keysRequiredFor')}: {purposeText(CONTROL)}</span>
          <div class="flex flex-wrap items-center gap-x-5 gap-y-2">
            {#each OPTIONAL as p (p)}
              <Checkbox checked={extra.includes(p)} onCheckedChange={(on) => (extra = on ? [...extra, p] : extra.filter((x) => x !== p))}>{t(hardwarePurposeLabels[p])}</Checkbox>
            {/each}
            <span class="flex-1"></span>
            {#if required}
              <button class={buttonClass('danger', 'sm')} disabled={busy} onclick={() => ask('account/hardware_policy', { purposes: [] })}>{t('stopRequiring')}</button>
            {/if}
            <button class={buttonClass('primary', 'sm')} disabled={busy || !own?.here} data-testid="approval-keys-require" onclick={() => ask('account/hardware_policy', { purposes: [...CONTROL, ...extra] })}>
              {required ? t('save') : t('requireKeys')}
            </button>
          </div>
          {#if missing.length}
            <p class="text-[12px] text-faint" data-testid="approval-keys-missing">{t('devicesWithoutKeysHint', { n: missing.length })}</p>
          {/if}
        </div>
        {#if keys.available}
          <div class="grid gap-2 border-t border-line pt-3">
            <span class="text-[12px] text-faint">{t('keyRequestHint')}</span>
            <div class="flex flex-wrap items-center gap-2">
              <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={requestKey}>{t('keyRequest')}</button>
            </div>
            {#if made}
              <Field label={t('deviceShown')}><input readonly value={made.device_id} class="font-mono text-[12.5px]" /></Field>
              <Field label={t('pasteRequest')}><textarea readonly rows="2" value={made.token} class="font-mono text-[11.5px]" data-testid="approval-key-token"></textarea></Field>
            {/if}
          </div>
        {/if}
        <div class="grid gap-2 border-t border-line pt-3">
          <Field label={t('pasteRequest')}><textarea bind:value={keyRequest} rows="2" class="font-mono text-[11.5px]" data-testid="approval-key-request"></textarea></Field>
          <div class="flex flex-wrap items-end gap-2">
            <Field label={t('deviceShown')} class="min-w-60 flex-1"><input bind:value={keyDevice} class="font-mono text-[12.5px]" /></Field>
            <button class={buttonClass('secondary')} disabled={busy || !keyRequest.trim() || !keyDevice.trim()} onclick={() => ask('device/key_approve', { token: keyRequest.trim(), device: keyDevice.trim() })}>
              {t('approveKeyRequest')}
            </button>
          </div>
        </div>
      </div>
    </Section>

    {#if keys.available}
      <Section title={t('deviceBound')} description={t('deviceBoundHint')} icon="lock" testid="device-bound">
        {#snippet actions()}
          <Pill tone={keys.device_bound ? 'forest' : 'neutral'}>{keys.device_bound ? t('on') : t('off')}</Pill>
        {/snippet}
        <div class="flex flex-wrap items-center gap-2">
          {#if !keys.device_bound && keys.biometry}<Checkbox checked={biometryOnly} onCheckedChange={(on) => (biometryOnly = on)}>{t('biometryOnly')}</Checkbox>{/if}
          <span class="flex-1"></span>
          <button class={buttonClass(keys.device_bound ? 'secondary' : 'primary', 'sm')} disabled={busy} data-testid="device-bound-toggle" onclick={() => bind(!keys.device_bound)}>
            {keys.device_bound ? t('deviceBoundOff') : t('deviceBoundOn')}
          </button>
        </div>
        {#if unprotected && (unprotected.background_online || unprotected.resident_agents)}
          <Notice tone="warn" testid="device-bound-unprotected">
            {t('deviceBoundUncovered')}
            {#if unprotected.background_online}{t('uncoveredResident')}{/if}
            {#if unprotected.resident_agents}{t('uncoveredAgents', { n: unprotected.resident_agents })}{/if}
          </Notice>
        {/if}
      </Section>
    {/if}
  {/if}

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
