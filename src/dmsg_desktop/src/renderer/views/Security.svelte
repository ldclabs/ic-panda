<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { offerSetupAgain } from '../notice-setup.svelte'
  import NoticeSetup from './NoticeSetup.svelte'
  let { status, go }: { status: any; go: (path: string) => void } = $props()
  import { display, watchDisplays, type Mode } from '../display.svelte'
  import DisplayNotice from '../components/DisplayNotice.svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import Collapsible from '../components/Collapsible.svelte'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import SecretInput from '../components/SecretInput.svelte'
  import TextField from '../components/TextField.svelte'
  import TextArea from '../components/TextArea.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { copyText } from '../components/clipboard'
  import { buttonClass } from '../components/ui'
  import { t, type Label } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, onChange, single } from '../rpc'
  import { choosePasskeyBinding, startLogin } from '../shell/login.svelte'
  import { hardwarePurposeLabels, purposeLabels } from '../approval'
  import { dailyLimit, policyChanges, sameSelection } from './security'
  import LocalOnly from './LocalOnly.svelte'

  /** The page shows effective settings. One dialog edits one task; the approval window decides account changes. */
  type Editor = 'policy' | 'hardware' | 'login' | 'key' | 'key-request' | 'key-approve' | 'bound'
  type Pending = 'policy' | 'hardware' | 'key'
  const titles: Record<Editor, Label> = {
    policy: 'signingPolicy', hardware: 'approvalKeys', login: 'approveLogin',
    key: 'approvalKey', 'key-request': 'keyRequest', 'key-approve': 'approveKeyRequest', bound: 'deviceBound'
  }
  const descriptions: Record<Editor, Label> = {
    policy: 'securityApprovalHint', hardware: 'securityApprovalHint', login: 'approveLoginHint',
    key: 'approvalKeysHint', 'key-request': 'keyRequestHint', 'key-approve': 'keyRequestHint', bound: 'deviceBoundHint'
  }
  const OPTIONAL = ['payment', 'attestation']
  const CONTROL = ['account_control', 'device_control', 'principal_control']
  let alive = true
  onDestroy(() => { alive = false })
  let security = $state<any>(null)
  let keys = $state<any>(null)
  let loading = $state(true)
  let loadError = $state('')
  let keyError = $state('')
  let error = $state('')
  let busy = $state(false)
  let needsLogin = $state(false)
  let editor = $state<Editor>('policy')
  let open = $state(false)
  let daily = $state('')
  let purposes = $state<string[]>([])
  let extra = $state<string[]>([])
  let request = $state('')
  let shown = $state('')
  let biometryOnly = $state(false)
  let made = $state<any>(null)
  let unprotected = $state<any>(null)
  let pending = $state<Partial<Record<Pending, string>>>({})

  const blocked = $derived(busy || needsLogin || !security || !!loadError)
  const own = $derived(keys?.key ?? null)
  const required = $derived((keys?.policy ?? []).length > 0)
  const missing = $derived((keys?.devices ?? []).filter((device: any) => !device.key_id))
  const policy = $derived(security ? policyChanges(security.policy, daily, purposes) : null)
  const hardwareChanged = $derived(!sameSelection([...CONTROL, ...extra], keys?.policy ?? []))
  const validDeviceId = $derived(/^[0-9a-f]{64}$/.test(shown.trim()))
  const canSubmit = $derived.by(() => {
    if (blocked) return false
    switch (editor) {
      case 'policy': return !!policy && !pending.policy
      case 'hardware': return hardwareChanged && !!own?.here && !own.fingerprints_changed && !keyError && !pending.hardware
      case 'login': return !!request.trim() && !!shown.trim()
      case 'key-approve': return !!request.trim() && validDeviceId && !keyError
      case 'key': return !!keys?.available && !keyError && !pending.key
      case 'key-request': return !!keys?.available && !keyError && !made
      case 'bound': return !!keys?.available && !keys.device_bound && !keyError
    }
  })
  const unchanged = $derived(editor === 'policy' ? dailyLimit(daily) !== null && !policy : editor === 'hardware' ? !hardwareChanged : false)
  const purposeText = (list: string[]) => list.map((p) => hardwarePurposeLabels[p] ? t(hardwarePurposeLabels[p]) : p).join(', ')

  const reload = single(async (mode: Mode = 'swr') => {
    if (!alive) return
    try {
      const next: any = await display('account/security', {}, mode)
      if (!alive) return
      // Rebase untouched fields only; background refreshes must not erase a draft or its error.
      if (!security || dailyLimit(daily) === security.policy.daily_executions) daily = String(next.policy.daily_executions)
      if (!security || sameSelection(purposes, security.policy.allowed_purposes)) purposes = [...next.policy.allowed_purposes]
      // A newly bound login can start a recovery: offer security alerts again.
      if (security && next.logins.length > security.logins.length && status?.account) offerSetupAgain(status.account)
      security = next
      needsLogin = false
      loadError = ''
    } catch (e) {
      if (!alive) return
      needsLogin = code(e) === 'AUTH_REQUIRED'
      loadError = needsLogin ? '' : errorText(e)
    } finally {
      if (alive) loading = false
    }
    try {
      const next: any = await call('device/key')
      if (!alive) return
      if (!keys || sameSelection(extra, keys.policy.filter((p: string) => OPTIONAL.includes(p)))) extra = next.policy.filter((p: string) => OPTIONAL.includes(p))
      keys = next
      keyError = ''
    } catch (e) {
      if (alive) keyError = errorText(e)
    }
    // Keep submitted settings unavailable until the request has a terminal outcome.
    for (const [group, id] of Object.entries(pending)) {
      try {
        const result: any = await call('approval/show', { id })
        if (!alive) return
        if (['done', 'rejected', 'cancelled', 'expired', 'failed', 'elsewhere'].includes(result?.state) && pending[group as Pending] === id) {
          delete pending[group as Pending]
        }
      } catch { /* retain the receipt when its result cannot yet be read */ }
    }
  })

  function begin(next: Editor) {
    if (blocked) return
    editor = next
    daily = String(security.policy.daily_executions)
    purposes = [...security.policy.allowed_purposes]
    extra = (keys?.policy ?? []).filter((p: string) => OPTIONAL.includes(p))
    request = ''
    shown = ''
    biometryOnly = false
    made = null
    error = ''
    open = true
  }

  function setOpen(value: boolean) {
    open = value
    if (!value) error = ''
  }

  async function guard(work: () => Promise<void>) {
    if (blocked) return
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      if (alive) busy = false
    }
  }

  /** Keep the receipt before opening its window, so an IPC failure never creates a second request. */
  function ask(method: string, params: unknown, group?: Pending) {
    return guard(async () => {
      const id = (group && pending[group]) || (await call<any>(method, params)).id
      if (!alive) return
      if (group) pending[group] = id
      await bridge(() => window.dmsg.openApproval(id))
      if (!alive) return
      open = false
      void reload()
    })
  }

  function showPending(group: Pending) {
    return guard(async () => { await bridge(() => window.dmsg.openApproval(pending[group]!)) })
  }

  function submit() {
    if (!canSubmit) return
    if (editor === 'policy') return ask('account/policy', policy, 'policy')
    if (editor === 'hardware') return ask('account/hardware_policy', { purposes: [...CONTROL, ...extra] }, 'hardware')
    if (editor === 'key') return ask('device/key_register', { biometry_only: biometryOnly }, 'key')
    if (editor === 'login') return ask('account/bind_approve', { token: request.trim(), principal: shown.trim() })
    if (editor === 'key-approve') return ask('device/key_approve', { token: request.trim(), device: shown.trim() })
    if (editor === 'key-request') return guard(async () => {
      const result = await call('device/key_request', { biometry_only: biometryOnly })
      if (alive) made = result
    })
    return bind(true)
  }

  function bind(enable: boolean) {
    return guard(async () => {
      const answer: any = await call('session/device_bound', { enable, biometry_only: enable && biometryOnly })
      if (!alive) return
      unprotected = enable ? answer.unprotected : null
      open = false
      await reload()
    })
  }

  onMount(() => {
    void reload()
    const off = onChange(['approval/changed', 'session/changed', 'recovery/changed'], reload)
    const stop = watchDisplays(['account'], (mode) => reload(mode))
    return () => { off(); stop() }
  })
</script>

{#snippet receipt(group: Pending)}
  {#if pending[group]}
    <Notice tone="info" testid={`security-pending-${group}`}>
      {t('securityApprovalPending')}
      {#snippet action()}
        <button class={buttonClass('secondary', 'xs')} disabled={blocked} onclick={() => showPending(group)}>{t('securityViewApproval')}</button>
      {/snippet}
    </Notice>
  {/if}
{/snippet}

<DisplayNotice methods={['account/security']} />
{#if loadError}
  <Notice failure={loadError} tone="danger">{loadError}
    {#snippet action()}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => reload('fresh')}>{t('retry')}</button>{/snippet}
  </Notice>
{/if}
{#if error && !open}<Notice failure={error} tone="danger">{error}</Notice>{/if}
{#if needsLogin}
  <Notice icon="login" testid="security-needs-login">
    {t('securityNeedsLogin')}
    {#snippet action()}
      <button class={buttonClass('primary', 'sm')} onclick={() => startLogin()} data-testid="security-login"><Icon name="login" />{t(status?.passkey_available ? 'loginWithPasskey' : 'loginWithIi')}</button>
    {/snippet}
  </Notice>
{/if}
{#if loading && !security}<p class="px-5 text-[13px] text-muted" role="status">{t('loading')}</p>{/if}
<NoticeSetup {status} {go} />
{#if security}
  <Section title={t('logins')} description={t('bindLoginHint')} icon="login" flush testid="security">
    <ul class="divide-y divide-line" data-testid="logins">
      {#each security.logins as login (login.principal)}
        <li class="flex flex-wrap items-center gap-2 px-5 py-2">
          <Id id={login.principal} full class="min-w-0 max-w-full" />
          {#if login.current}<Pill tone="forest">{t('current')}</Pill>{/if}
          {#if !login.current && security.logins.length > 1}
            <button class={buttonClass('danger', 'xs', 'ms-auto')} disabled={blocked} onclick={() => ask('account/unbind', { principal: login.principal })}>{t('remove')}</button>
          {/if}
        </li>
      {:else}<li class="px-5 py-2 text-[13px] text-muted">{t('securityNoLogins')}</li>{/each}
    </ul>
    <div class="mt-3 flex flex-wrap gap-2 px-5">
      {#if status?.passkey_available}
        <button class={buttonClass('secondary', 'sm')} disabled={blocked} data-testid="bind-passkey" onclick={choosePasskeyBinding}><Icon name="add" />{t('bindPasskey')}</button>
      {/if}
      <button class={buttonClass('secondary', 'sm')} disabled={blocked} data-testid="bind-login" onclick={() => startLogin(true, 'https://dmsg.net', 'ii')}><Icon name="add" />{t('bindLogin')}</button>
      <button class={buttonClass('secondary', 'sm')} disabled={blocked} data-testid="security-approve-login" onclick={() => begin('login')}><Icon name="login" />{t('approveLogin')}</button>
    </div>
    {#if status?.login_renewable}
      <div class="mt-3 grid justify-items-start gap-2 px-5">
        <button class={buttonClass('danger', 'sm')} disabled={busy} data-testid="logout-all-logins" onclick={() => startLogin(false, 'https://dmsg.net', 'passkey', 'logout_all')}>{t('logoutAllLogins')}</button>
        <p class="text-[12.5px] text-muted">{t('logoutAllLoginsHint')}</p>
      </div>
    {/if}
  </Section>

  <Section title={t('signingPolicy')} description={t('signingPolicyHint')} icon="signature" testid="security-policy">
    <div class="mb-2"><Pill tone={security.policy.frozen ? 'warn' : 'ok'}>{security.policy.frozen ? t('frozen') : t('policyOpen')}</Pill></div>
    <dl>
      <Fact label={t('dailyExecutions')} testid="security-policy-daily">{security.policy.daily_executions}</Fact>
      <Fact label={t('purposes')}>{security.policy.allowed_purposes.map((p: string) => t(purposeLabels[p])).join(', ') || t('nothingRequired')}</Fact>
    </dl>
    <div class="mt-3 flex flex-wrap gap-2">
      <button class={buttonClass('secondary', 'sm')} disabled={blocked || !!pending.policy} aria-label={`${t('edit')} · ${t('signingPolicy')}`} data-testid="security-policy-edit" onclick={() => begin('policy')}>{t('edit')}</button>
      <button class={buttonClass(security.policy.frozen ? 'secondary' : 'danger', 'sm')} disabled={blocked || !!pending.policy} data-testid="security-policy-freeze" onclick={() => ask('account/policy', { frozen: !security.policy.frozen }, 'policy')}>
        {security.policy.frozen ? t('unfreeze') : t('freeze')}
      </button>
    </div>
    {@render receipt('policy')}
  </Section>

  <Section title={t('recovery')} description={t('recoveryHint')} icon="recovery">
    <button class={buttonClass('secondary', 'sm')} aria-label={`${t('manage')} · ${t('recovery')}`} data-testid="security-recovery-link" onclick={() => go('/settings/recovery')}>{t('manage')}</button>
  </Section>

  {#if keyError}
    <Notice failure={keyError} tone="danger" testid="security-key-error">{keyError}
      {#snippet action()}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => reload('fresh')}>{t('retry')}</button>{/snippet}
    </Notice>
  {/if}
  {#if keys}
    <Section title={t('approvalKeys')} description={t('approvalKeysHint')} icon="key" testid="approval-keys">
      <div class="mb-2"><Pill tone={required ? 'forest' : 'neutral'}>{required ? t('keysRequired') : t('keysNotRequired')}</Pill></div>
      {#if !keys.available}<Notice testid="approval-keys-unavailable">{t('approvalKeysUnavailable')}</Notice>{/if}
      <dl>
        <Fact label={t('thisDevice')} testid="approval-key-own">
          {#if own?.fingerprints_changed}<Pill tone="danger">{t('fingerprintsChanged')}</Pill>
          {:else if own?.here}<Pill tone="ok">{t('keyRegistered')}</Pill>{#if own.biometry_only}<Pill tone="neutral">{t('touchIdOnly')}</Pill>{/if}
          {:else if own}<Pill tone="warn">{t('keyElsewhere')}</Pill>
          {:else}<Pill tone="neutral">{t('keyNotRegistered')}</Pill>{/if}
        </Fact>
        <Fact label={t('approvalKeysRequiredFor')}>{required ? purposeText(keys.policy) : t('nothingRequired')}</Fact>
      </dl>
      <div class="mt-3 flex flex-wrap gap-2">
        {#if keys.available}<button class={buttonClass('secondary', 'sm')} disabled={blocked || !!keyError || !!pending.key} data-testid="security-key-edit" onclick={() => begin('key')}>{own ? t('replaceKey') : t('registerKey')}</button>{/if}
        <button class={buttonClass('secondary', 'sm')} disabled={blocked || !!keyError || !own?.here || own.fingerprints_changed || !!pending.hardware} aria-label={required ? `${t('edit')} · ${t('approvalKeys')}` : t('requireKeys')} data-testid="approval-keys-require" onclick={() => begin('hardware')}>{required ? t('edit') : t('requireKeys')}</button>
        {#if own}<button class={buttonClass('danger', 'sm')} disabled={blocked || !!keyError || !!pending.key} onclick={() => ask('device/key_retire', {}, 'key')}>{t('retireKey')}</button>{/if}
      </div>
      {@render receipt('key')}
      {@render receipt('hardware')}
      {#if required && missing.length}<p class="mt-3 text-[12px] text-faint" data-testid="approval-keys-missing">{t('devicesWithoutKeysHint', { n: missing.length })}</p>{/if}
      <Collapsible class="mt-3" testid="security-key-transfer">
        {#snippet title()}<span class="text-[12.5px]">{t('securityKeyTransfer')}</span>{/snippet}
        <p class="py-2 text-[12px] text-muted">{t('keyRequestHint')}</p>
        <div class="flex flex-wrap gap-2">
          {#if keys.available}<button class={buttonClass('secondary', 'sm')} disabled={blocked || !!keyError} data-testid="security-key-request" onclick={() => begin('key-request')}>{t('keyRequest')}</button>{/if}
          <button class={buttonClass('secondary', 'sm')} disabled={blocked || !!keyError} data-testid="security-key-approve" onclick={() => begin('key-approve')}>{t('approveKeyRequest')}</button>
        </div>
      </Collapsible>
    </Section>

    {#if keys.available}
      <Section title={t('deviceBound')} description={t('deviceBoundHint')} icon="lock" testid="device-bound">
        <div class="flex flex-wrap items-center gap-3">
          <Pill tone={keys.device_bound ? 'forest' : 'neutral'}>{keys.device_bound ? t('on') : t('off')}</Pill>
          <button class={buttonClass('secondary', 'sm')} disabled={blocked || !!keyError} data-testid="device-bound-toggle" onclick={() => keys.device_bound ? bind(false) : begin('bound')}>{keys.device_bound ? t('deviceBoundOff') : t('deviceBoundOn')}</button>
        </div>
        {#if unprotected && (unprotected.background_online || unprotected.resident_agents)}
          <Notice tone="warn" testid="device-bound-unprotected">{t('deviceBoundUncovered')}{#if unprotected.background_online}{t('uncoveredResident')}{/if}{#if unprotected.resident_agents}{t('uncoveredAgents', { n: unprotected.resident_agents })}{/if}</Notice>
        {/if}
      </Section>
    {/if}
  {/if}
{/if}

<Dialog bind:open={() => open, setOpen} title={editor === 'key' ? (own ? t('replaceKey') : t('registerKey')) : t(titles[editor])} description={t(descriptions[editor])} icon={editor === 'policy' ? 'signature' : editor === 'login' ? 'login' : 'key'} dismissible={!busy} testid="security-editor">
  <div class="grid gap-4">
    {#if error}<Notice failure={error} tone="danger" testid="security-editor-error">{error}</Notice>{/if}
    {#if needsLogin || loadError}<Notice tone="warn">{needsLogin ? t('securityNeedsLogin') : loadError}</Notice>{/if}
    {#if keyError && ['hardware', 'key', 'key-request', 'key-approve', 'bound'].includes(editor)}<Notice failure={keyError} tone="danger">{keyError}</Notice>{/if}
    {#if editor === 'policy'}
      <TextField label={t('dailyExecutions')} bind:value={daily} inputmode="numeric" disabled={blocked || !!pending.policy} hint={t('securityDailyHint')} error={dailyLimit(daily) === null ? t('securityDailyRange') : ''} data-testid="daily-executions" />
      <fieldset class="grid gap-2" disabled={blocked || !!pending.policy}>
        <legend class="mb-2 text-[12px] text-muted">{t('purposes')}</legend>
        {#each ['FileAttestation', 'Statement', 'AppAction'] as p (p)}
          <Checkbox checked={purposes.includes(p)} disabled={blocked || !!pending.policy} onCheckedChange={(on) => (purposes = on ? [...purposes, p] : purposes.filter((x) => x !== p))} testid={`security-purpose-${p}`}>{t(purposeLabels[p])}</Checkbox>
        {/each}
      </fieldset>
      {#if dailyLimit(daily) === 0 || !purposes.length}<Notice tone="warn">{t('securityNoSigning')}</Notice>{/if}
      {@render receipt('policy')}
    {:else if editor === 'hardware'}
      <p class="text-[12.5px] text-muted">{t('keysRequiredFor')}: {purposeText(CONTROL)}</p>
      {#each OPTIONAL as p (p)}<Checkbox checked={extra.includes(p)} disabled={blocked || !!pending.hardware} onCheckedChange={(on) => (extra = on ? [...extra, p] : extra.filter((x) => x !== p))} testid={`security-hardware-${p}`}>{t(hardwarePurposeLabels[p])}</Checkbox>{/each}
      {#if missing.length}<Notice tone="warn">{t('devicesWithoutKeysHint', { n: missing.length })}</Notice>{/if}
      {@render receipt('hardware')}
      {#if required}
        <div class="border-t border-line pt-3">
          <button class={buttonClass('danger', 'sm')} disabled={blocked || !!pending.hardware || !!keyError} onclick={() => ask('account/hardware_policy', { purposes: [] }, 'hardware')}>{t('stopRequiring')}</button>
        </div>
      {/if}
    {:else if editor === 'login' || editor === 'key-approve'}
      <SecretInput label={t('pasteRequest')} bind:value={request} multiline disabled={blocked} testid={editor === 'login' ? 'approve-request' : 'approval-key-request'} />
      <TextField label={editor === 'login' ? t('principalShown') : t('deviceShown')} bind:value={shown} mono disabled={blocked} error={editor === 'key-approve' && shown.trim() && !validDeviceId ? t('securityDeviceIdHint') : ''} data-testid="approve-shown" />
    {:else if made && editor === 'key-request'}
      <TextField label={`${t('device')} · ${t('thisDevice')}`} readonly value={made.device_id} mono />
      <TextArea label={t('requestToken')} readonly rows={3} value={made.token} mono data-testid="approval-key-token" />
      <button class={buttonClass('secondary', 'sm', 'justify-self-start')} onclick={() => copyText(made.token)} data-testid="approval-key-copy"><Icon name="copy" />{t('copy')}</button>
    {:else}
      {#if editor === 'bound'}<LocalOnly testid="device-bound-local-only" />{/if}
      {#if keys?.biometry}<Checkbox checked={biometryOnly} disabled={blocked || !!pending.key && editor === 'key'} onCheckedChange={(on) => (biometryOnly = on)} testid="security-biometry">{t('biometryOnly')}</Checkbox>{/if}
      {#if editor === 'key'}{@render receipt('key')}{/if}
    {/if}
  </div>
  {#snippet footer()}
    {#if unchanged}<span class="text-[12px] text-faint" role="status" data-testid="security-unchanged">{t('securityUnchanged')}</span>{/if}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy} onclick={() => setOpen(false)}>{made ? t('close') : t('cancel')}</button>
    {#if !(editor === 'key-request' && made)}
      <button class={buttonClass('primary')} disabled={!canSubmit} data-testid={editor === 'key' ? 'approval-key-register' : 'security-submit'} onclick={submit}>
        {editor === 'bound' ? t('deviceBoundOn') : editor === 'key-request' ? t('keyRequest') : editor === 'login' ? t('approveLogin') : t('submit')}
      </button>
    {/if}
  {/snippet}
</Dialog>
