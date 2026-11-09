<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { display, watchDisplays, type Mode } from '../display.svelte'
  import { noticeVisibility } from '../notification-visibility'
  import DisplayNotice from '../components/DisplayNotice.svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import Dialog from '../components/Dialog.svelte'
  import Segmented from '../components/Segmented.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Field from '../components/page/Field.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, onChange, single, when } from '../rpc'
  import { startLogin } from '../shell/login.svelte'

  let { status }: { status: any } = $props()
  type Pending = 'recovery' | 'cancel'
  let alive = true
  onDestroy(() => { alive = false })
  let security = $state<any>(null)
  let loading = $state(true)
  let loadError = $state('')
  let error = $state('')
  let needsLogin = $state(false)
  let busy = $state(false)
  let open = $state(false)
  let enabled = $state(false)
  let days = $state('')
  let pending = $state<Partial<Record<Pending, string>>>({})
  const blocked = $derived(busy || needsLogin || !security || !!loadError)
  const changed = $derived(/^[1-7]$/.test(days) && (enabled !== security?.recovery_enabled || (enabled && Number(days) * 86_400_000 !== security?.recovery_delay_ms)))

  const reload = single(async (mode: Mode = 'swr') => {
    if (!alive) return
    try {
      const next: any = await display('account/security', {}, mode)
      if (!alive) return
      if (!security || Number(days) * 86_400_000 === security.recovery_delay_ms) days = String(next.recovery_delay_ms / 86_400_000)
      if (!security || enabled === security.recovery_enabled) enabled = next.recovery_enabled
      security = next; needsLogin = false; loadError = ''
    } catch (e) {
      if (!alive) return
      needsLogin = code(e) === 'AUTH_REQUIRED'
      loadError = needsLogin ? '' : errorText(e)
    } finally { if (alive) loading = false }
    for (const [group, id] of Object.entries(pending)) {
      try {
        const result: any = await call('approval/show', { id })
        if (!alive) return
        if (['done', 'rejected', 'cancelled', 'expired', 'failed', 'elsewhere'].includes(result?.state) && pending[group as Pending] === id) delete pending[group as Pending]
      } catch { /* Keep the receipt until its outcome can be read. */ }
    }
  })

  const reportNotice = noticeVisibility(() => security?.notification && status?.state === 'unlocked'
    ? { account: status.account, session: status.session, ...security.notification } : null, '[data-testid="pending-recovery"]')
  $effect(() => { void security?.notification; void status?.session; reportNotice() })

  function begin() {
    if (blocked) return
    enabled = security.recovery_enabled; days = String(security.recovery_delay_ms / 86_400_000)
    error = ''; open = true
  }

  async function ask(group: Pending, method?: string, params?: unknown) {
    if (blocked) return
    busy = true; error = ''
    try {
      const id = pending[group] || (method && (await call<any>(method, params)).id)
      if (!alive || !id) return
      pending[group] = id
      await bridge(() => window.dmsg.openApproval(id))
      if (!alive) return
      open = false; void reload()
    } catch (e) { if (alive) error = errorText(e) }
    finally { if (alive) busy = false }
  }

  function submit() {
    if (!changed || pending.recovery) return
    return ask('recovery', 'account/recovery_policy', { enabled, days: enabled ? Number(days) : security.recovery_delay_ms / 86_400_000 })
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
      {#snippet action()}<button class={buttonClass('secondary', 'xs')} disabled={blocked} onclick={() => ask(group)}>{t('securityViewApproval')}</button>{/snippet}
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
  <Notice>{t('securityNeedsLogin')}
    {#snippet action()}<button class={buttonClass('primary', 'sm')} onclick={() => startLogin()}>{t('login')}</button>{/snippet}
  </Notice>
{/if}
{#if loading && !security}<p role="status" class="text-[13px] text-muted">{t('loading')}</p>{/if}
{#if security}
  <Section title={t('onChainRecovery')} description={t(security.recovery_enabled ? 'recoveryOnHint' : 'recoveryOffHint')} icon="recovery" testid="security-recovery">
    <Pill tone={security.recovery_enabled ? 'forest' : 'neutral'}>{t(security.recovery_enabled ? 'on' : 'off')}</Pill>
    {#if security.recovery_updating}<Notice tone="warn">{t('recoveryUpdating')}</Notice>{/if}
    {#if security.recovery_enabled}<dl><Fact label={t('recoveryDays')}>{t('daysN', { n: security.recovery_delay_ms / 86_400_000 })}</Fact></dl>{/if}
    <button class={buttonClass('secondary', 'sm', 'mt-3')} disabled={blocked || !!pending.recovery} aria-label={`${t('edit')} · ${t('recovery')}`} data-testid="security-recovery-edit" onclick={begin}>{t('edit')}</button>
    {@render receipt('recovery')}
  </Section>
  {#if security.pending_recovery}
    <Section title={t('pendingRecovery')} icon="recovery" testid="pending-recovery">
      <dl>
        <Fact label={t('loginPrincipal')}><Id id={security.pending_recovery.new_auth} full /></Fact>
        <Fact label={t('executeAfter')}>{when(security.pending_recovery.execute_after)}</Fact>
        {#if security.pending_recovery.expires_at}<Fact label={t('expires')}>{when(security.pending_recovery.expires_at)}</Fact>{/if}
      </dl>
      <button class={buttonClass('danger', 'sm', 'mt-3')} disabled={blocked || !!pending.cancel} data-testid="recovery-cancel" onclick={() => ask('cancel', 'recovery/dispute', {})}>{t('disputeRecovery')}</button>
      {@render receipt('cancel')}
    </Section>
  {:else}<p class="text-[12.5px] text-faint" data-testid="recovery-empty">{t('noRecovery')}</p>{/if}
{/if}

<Dialog bind:open={() => open, (value) => { open = value; if (!value) error = '' }} title={t('onChainRecovery')} description={t('securityApprovalHint')} icon="recovery" dismissible={!busy} testid="security-editor">
  <div class="grid gap-4">
    {#if error}<Notice failure={error} tone="danger" testid="security-editor-error">{error}</Notice>{/if}
    {#if needsLogin || loadError}<Notice tone="warn">{needsLogin ? t('securityNeedsLogin') : loadError}</Notice>{/if}
    <Checkbox bind:checked={enabled} disabled={blocked || !!pending.recovery} testid="recovery-enabled">{t('onChainRecovery')}</Checkbox>
    <Notice tone="warn">{t(enabled ? 'recoveryOnHint' : 'recoveryOffHint')}</Notice>
    {#if enabled}<Field label={t('recoveryDays')}><Segmented bind:value={days} disabled={blocked || !!pending.recovery} label={t('recoveryDelay')} options={['1', '2', '3', '4', '5', '6', '7'].map((d) => ({ value: d, label: d }))} testid="recovery-days" /></Field>{/if}
    {@render receipt('recovery')}
  </div>
  {#snippet footer()}
    {#if !changed}<span class="text-[12px] text-faint" role="status">{t('securityUnchanged')}</span>{/if}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy} onclick={() => { open = false; error = '' }}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={blocked || !changed || !!pending.recovery} data-testid="security-submit" onclick={submit}>{t('submit')}</button>
  {/snippet}
</Dialog>
