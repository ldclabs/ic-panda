<script lang="ts">
  import { reminderVisibility } from '../notification-visibility'
  import { onMount, untrack } from 'svelte'
  import Switch from '../components/Switch.svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import Confirm from '../components/Confirm.svelte'
  import Select from '../components/Select.svelte'
  import SearchSelect from '../components/SearchSelect.svelte'
  import Field from '../components/page/Field.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass } from '../components/ui'
  import { toast } from '../components/toast.svelte'
  import { bridge, call, code, errorText, single, when } from '../rpc'
  import { rememberPlain, rememberedPlain } from '../view-memory'
  import { t, stateLabel, type Label } from '../i18n/index.svelte'
  import { tomorrowMorning } from '../../shared/notification-time'
  import { readReceipts, setReadReceipts } from './channels/preferences'
  import { noticeCategories, type GlobalNoticeSettings, type NoticeCategory, type NoticePermission, type NoticePreferences, type NoticeSettings, type NoticeStatus } from '../../shared/notifications'

  let { status }: { status: any } = $props()
  let preferences = $state<NoticePreferences | null>(null)
  let global = $state<GlobalNoticeSettings | null>(null)
  let account = $state<NoticeSettings | null>(null)
  const accountId = $derived(status?.account ?? '')
  let agents = $state<{ agent_id: string; label: string }[]>([])
  let permission = $state<NoticePermission | null>(null)
  /** Other local accounts already receiving notifications on this device. */
  let others = $state<string[]>([])
  let linking = $state(false)
  let agentError = $state(''), platformError = $state(''), agentsLoading = $state(false)
  let saving = $state(false), pausing = $state(false), testing = $state(false)
  let observedAt = $state(Date.now())
  let justSaved = $state(false)
  let savedTimer: ReturnType<typeof setTimeout>
  let loading = $state(true), error = $state('')
  const busy = $derived(saving || pausing || testing)
  let alive = true, generation = 0, editRevision = 0
  const scope = $derived(`${status?.account ?? ''}:${status?.session ?? ''}`)
  const categoryLabels: Record<NoticeCategory, Label> = { security: 'security', action: 'approvals', message: 'noticeMessages', service: 'noticeServices' }
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone
  const zones = [...new Set([zone, ...Intl.supportedValuesOf('timeZone')])].sort().map((value) => ({ value, label: value }))
  const times = Array.from({ length: 96 }, (_, index) => ({ value: String(index * 15), label: `${String(Math.floor(index / 4)).padStart(2, '0')}:${String(index % 4 * 15).padStart(2, '0')}` }))
  const dirty = $derived(!!preferences && (JSON.stringify(global) !== JSON.stringify(preferences.global) || JSON.stringify(account) !== JSON.stringify(preferences.account)))
  const pending = $derived(!!preferences?.global_pending_sync || (!!account && account.revision > account.remote_revision))
  const conflict = $derived(!!preferences?.global_conflict || !!preferences?.account_conflict)
  const reportReminder = reminderVisibility(() => ({ status, ready: !!preferences && !loading && !error }), '[data-testid="notification-settings"]')
  $effect(() => { void status; void preferences; void loading; void error; reportReminder() })
  const valid = (turn: number) => alive && generation === turn
  function clone<T>(value: T): T { return JSON.parse(JSON.stringify(value)) as T }

  const invalidHours = $derived(!!global?.quiet_hours && global.quiet_hours.start === global.quiet_hours.end)
  const paused = $derived(!!global?.paused_until && global.paused_until > observedAt)
  $effect(() => {
    const remaining = (global?.paused_until ?? 0) - Date.now()
    observedAt = Date.now()
    if (remaining > 0) {
      const timer = setTimeout(() => { observedAt = Date.now() }, remaining + 1)
      return () => clearTimeout(timer)
    }
  })

  type Draft = { base: NoticePreferences; global: GlobalNoticeSettings; account: NoticeSettings | null }
  const draftKey = $derived('settings.notifications.' + scope)
  function currentDraft(): Draft | undefined {
    return dirty && preferences && global ? { base: preferences, global, account } : undefined
  }
  // Only edited fields follow the user across a refresh. A device or another window
  // can change a different category, pause, or revision while the form is open.
  function withEdits<T>(base: T, draft: T, latest: T): T {
    if (JSON.stringify(base) === JSON.stringify(draft)) return latest
    if (!base || !draft || !latest || typeof base !== 'object' || typeof draft !== 'object' || typeof latest !== 'object') return draft
    const result = { ...latest }
    for (const key of Object.keys(draft) as (keyof T)[]) result[key] = withEdits(base[key], draft[key], latest[key])
    return result
  }
  function accept(next: NoticePreferences, edits?: Draft) {
    global = clone(edits ? withEdits(edits.base.global, edits.global, next.global) : next.global)
    account = clone(edits?.base.account && edits.account && next.account
      ? withEdits(edits.base.account, edits.account, next.account) : next.account)
    preferences = next
  }
  $effect(() => {
    if (preferences) {
      const edits = currentDraft()
      rememberPlain(draftKey, edits ? clone(edits) : null)
    }
  })

  async function readPlatform() {
    const turn = generation
    try {
      const value = await bridge<NoticePermission>(() => window.dmsg.notificationPermission())
      if (valid(turn)) { permission = value; platformError = '' }
    } catch (cause) { if (valid(turn)) platformError = errorText(cause) }
  }
  async function readAgents() {
    if (status?.state !== 'unlocked') return
    const turn = generation
    agentsLoading = true
    try {
      const result = await call<{ agents: typeof agents }>('agent/ids')
      if (valid(turn)) { agents = result.agents; agentError = '' }
    } catch (cause) { if (valid(turn)) agentError = errorText(cause) }
    finally { if (valid(turn)) agentsLoading = false }
  }
  async function load() {
    const turn = ++generation, id = accountId
    loading = true
    error = ''
    void readPlatform()
    void readAgents()
    try {
      const next = await call<NoticePreferences>('notifications/settings', id ? { account: id } : {})
      if (valid(turn)) accept(next, currentDraft() ?? rememberedPlain<Draft>(draftKey))
    } catch (cause) { if (valid(turn)) error = errorText(cause) }
    finally { if (valid(turn)) loading = false }
  }

  $effect(() => {
    void scope
    untrack(() => {
      preferences = null; global = null; account = null; agents = []; permission = null
      others = []; linking = saving = testing = pausing = false
      platformError = agentError = ''
      void load()
    })
  })

  const refreshSaved = single(async () => {
    if (busy || loading || !alive) return
    const turn = generation, revision = editRevision, id = accountId
    try {
      const next = await call<NoticePreferences>('notifications/settings', id ? { account: id } : {})
      if (valid(turn) && revision === editRevision && !busy) accept(next, currentDraft())
    } catch { /* The form keeps its last read until an explicit refresh. */ }
  })
  onMount(() => {
    const off = window.dmsg.onEvent(event => {
      if (event.type === 'notifications/changed') void refreshSaved()
      if (event.type === 'notifications/platform') void readPlatform()
    })
    return () => { alive = false; generation++; clearTimeout(savedTimer); off() }
  })

  let receipts = $state(readReceipts())
  function changeReceipts(on: boolean) {
    setReadReceipts(on)
    receipts = readReceipts()
    if (receipts !== on) toast(() => t('localStorageUnavailable'))
  }
  function enable(value: boolean) {
    if (!global) return
    global.enabled = value
    // The first explicit opt-in selects only the current account.
    if (value && preferences?.global.revision === 0 && account?.revision === 0) account.enabled = true
  }

  async function save(linked = false) {
    if (!preferences || !global || busy || loading || invalidHours || (!dirty && !conflict)) return
    // Registering another account tells the notification service and the push
    // provider that these accounts share this device; that needs consent.
    const turn = generation, id = accountId, origin = scope
    saving = true
    justSaved = false
    editRevision++
    let written = false
    error = ''
    try {
      if (!linked && account?.enabled && !preferences.account?.enabled) {
        const counts = await call<NoticeStatus>('notifications/status')
        if (!valid(turn)) return
        others = counts.accounts.filter(entry => entry.enabled && entry.account !== id).map(entry => entry.account)
        if (others.length) { linking = true; return }
      }
      if (JSON.stringify(global) !== JSON.stringify(preferences.global) || preferences.global_conflict) {
        const result = await call<{ settings: GlobalNoticeSettings; pending_sync: boolean }>('notifications/settings_set', { scope: 'installation', expected_revision: preferences.global.revision, settings: clone(global) })
        if (!valid(turn)) return
        written = true
        preferences.global_pending_sync = result.pending_sync
        preferences.global = result.settings
        global = clone(result.settings)
      }
      if (account && preferences.account && (JSON.stringify(account) !== JSON.stringify(preferences.account) || preferences.account_conflict)) {
        const result = await call<{ settings: NoticeSettings }>('notifications/settings_set', { scope: 'account', account: id, expected_revision: preferences.account.revision, settings: clone(account) })
        if (!valid(turn)) return
        written = true
        preferences.account = result.settings
        account = clone(result.settings)
      }
      if (valid(turn)) {
        justSaved = true
        clearTimeout(savedTimer)
        savedTimer = setTimeout(() => { justSaved = false }, 3500)
        rememberPlain(draftKey, null)
      }
    } catch (cause) {
      if (!valid(turn)) return
      // Re-read the version after a conflict, retaining the person's draft. A
      // second explicit Save is required; no automatic overwrite/retry occurs.
      if (code(cause) === 'VERSION_CONFLICT') {
        await load()
        if (alive && scope === origin && !error) error = t('conflictItem')
      } else if (alive && scope === origin) error = errorText(cause)
      if (linked) throw cause
    } finally {
      if (alive && scope === origin) {
        saving = false
        if (written) void refreshSaved()
      }
    }
  }

  async function pause(until: number | null) {
    if (!preferences || !global || busy) return
    const turn = generation
    pausing = true
    editRevision++
    error = ''
    try {
      const result = await call<{ settings: GlobalNoticeSettings; pending_sync: boolean }>('notifications/settings_set', {
        scope: 'installation', expected_revision: preferences.global.revision,
        settings: { ...clone(preferences.global), paused_until: until }
      })
      if (!valid(turn)) return
      preferences.global = result.settings
      preferences.global_pending_sync = result.pending_sync
      // Leave unrelated unsaved fields alone.
      global = { ...global, revision: result.settings.revision, paused_until: until }
    } catch (cause) { if (valid(turn)) error = errorText(cause) }
    finally { if (valid(turn)) { pausing = false; void refreshSaved() } }
  }

  async function test() {
    if (busy) return
    const turn = generation
    testing = true
    try { await bridge(() => window.dmsg.notificationTest()); if (valid(turn)) { toast(() => t('noticeTestSent')); await readPlatform() } }
    catch (cause) { if (valid(turn)) platformError = errorText(cause) }
    finally { if (valid(turn)) testing = false }
  }
</script>

<div class="space-y-6" data-testid="notification-settings">
  {#if error && !preferences}<Notice failure={error} tone="danger" testid="notifications-error">{error}</Notice>{/if}
  {#if loading && !global}<p class="px-5 text-body text-muted" role="status">{t('loading')}</p>{/if}
  {#if global && preferences}
    <Section title={t('noticeSettings')} description={t('noticeScopeHint')} icon="bell">
      <div class="space-y-4">
        <Checkbox checked={global.enabled} onCheckedChange={enable} disabled={busy || loading} label={t('noticeEnable')} testid="notifications-enabled">{t('noticeEnable')}</Checkbox>
        <div class="grid gap-2 rounded-xl border border-line bg-surface p-3">
        <p class="text-callout font-medium">{t('settingsInstant')}</p>
        {#if platformError}<Notice tone="warn" testid="notifications-platform-error">{platformError}
          <button class={buttonClass('ghost','xs')} onclick={readPlatform}>{t('retry')}</button></Notice>{/if}
        {#if permission}<p class="text-callout text-muted">{t(permission.supported ? 'noticeManaged' : 'noticeUnsupported')}</p>{/if}
        <div class="flex flex-wrap gap-2">
          <button class={buttonClass('secondary', 'sm')} disabled={busy || !permission?.supported} onclick={test} data-testid="notifications-test">{t('noticeTest')}</button>
          <button class={buttonClass('ghost', 'sm')} disabled={!permission?.settings_available} onclick={async () => {
            try { await bridge(() => window.dmsg.notificationSettings()) }
            catch (cause) { if (alive) platformError = errorText(cause) }
          }}>{t('noticeSystem')}</button>
        </div>
        {#if permission?.last_result}<p class="text-callout text-muted" role="status">{permission.last_result === 'failed' ? stateLabel('failed') : t('noticeTestSent')}</p>{/if}
        <div class="flex flex-wrap gap-2">
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => pause(Date.now() + 900_000)}>{t('noticePause15')}</button>
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => pause(Date.now() + 3_600_000)}>{t('noticePauseHour')}</button>
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => pause(tomorrowMorning(Date.now(), zone))}>{t('noticePauseTomorrow')}</button>
          {#if paused}<button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => pause(null)}>{t('noticeResume')}</button>{/if}
        </div>
        {#if paused}<p class="text-callout text-muted">{t('noticeUntil')} {when(global.paused_until!)}</p>{/if}
        </div>
        <Checkbox class="flex" checked={global.quiet_hours !== null} disabled={busy || loading} onCheckedChange={(enabled) => { global!.quiet_hours = enabled ? { start: 1320, end: 480, timezone: zone } : null }} label={t('noticeQuiet')} testid="notifications-quiet">{t('noticeQuiet')}</Checkbox>
        {#if global.quiet_hours}
          <div class="flex flex-wrap gap-4">
            <Field label={t('noticeFrom')}><Select value={String(global.quiet_hours.start)} options={times} label={t('noticeFrom')} aria-invalid={invalidHours} aria-describedby={invalidHours ? 'settings-hours-error' : undefined} onChange={(value) => { global!.quiet_hours!.start = Number(value) }} disabled={busy} testid="notifications-quiet-from" /></Field>
            <Field label={t('noticeUntil')}><Select value={String(global.quiet_hours.end)} options={times} label={t('noticeUntil')} aria-invalid={invalidHours} aria-describedby={invalidHours ? 'settings-hours-error' : undefined} onChange={(value) => { global!.quiet_hours!.end = Number(value) }} disabled={busy} testid="notifications-quiet-until" /></Field>
            <Field label={t('noticeZone')} class="min-w-40 flex-1"><SearchSelect value={global.quiet_hours.timezone} options={zones} label={t('noticeZone')} onChange={(value) => { global!.quiet_hours!.timezone = value }} disabled={busy} /></Field>
          </div>
          {#if invalidHours}<p id="settings-hours-error" class="text-callout text-danger" role="alert">{t('settingsQuietRange')}</p>{/if}
        {/if}
        <Checkbox class="flex" bind:checked={global.security_exception} disabled={busy || loading} label={t('noticeSecurityException')}>{t('noticeSecurityException')}</Checkbox>
        <Field label={t('noticeBadge')}><Select value={global.badge} options={[{value:'all',label:t('noticeBadgeAll')},{value:'actions',label:t('noticeBadgeActions')},{value:'off',label:t('noticeBadgeOff')}]} label={t('noticeBadge')} onChange={(value) => { global!.badge = value as GlobalNoticeSettings['badge'] }} disabled={busy || loading} /></Field>
      </div>
    </Section>

    <Section title={t('channels')} description={t('settingsInstant')} icon="channels" testid="notifications-channels">
      <Switch checked={receipts} onCheckedChange={changeReceipts} label={t('sendReadReceipts')} testid="read-receipts">{t('sendReadReceipts')}</Switch>
    </Section>

    <Section title={t('account')} icon="account" testid="notifications-account-section">
      {#snippet actions()}{#if accountId}<Id id={accountId} testid="notifications-account-id" />{/if}{/snippet}
      <div class="space-y-4">
        {#if !account}<Notice>{t('loggedOutHint')}</Notice>{/if}
        {#if account}
          <Checkbox bind:checked={account.enabled} disabled={busy || loading} label={t('noticeAccount')} testid="notifications-account-enabled">{t('noticeAccount')}</Checkbox>
          <div class="divide-y divide-line rounded-xl border border-line bg-surface">
            {#each noticeCategories as category}
              <div class="flex flex-wrap items-center justify-between gap-3 p-3">
                <Checkbox bind:checked={account.categories[category].enabled} disabled={busy || loading} label={t(categoryLabels[category])}>{t(categoryLabels[category])}</Checkbox>
                <Checkbox bind:checked={account.categories[category].sound} disabled={busy || loading || !account.categories[category].enabled} label={`${t(categoryLabels[category])}: ${t('noticeSound')}`}>{t('noticeSound')}</Checkbox>
              </div>
            {/each}
          </div>
          <Checkbox bind:checked={account.merge_devices} disabled={busy || loading} label={t('noticeMerge')}>{t('noticeMerge')}</Checkbox>
          {#if status?.state === 'unlocked'}
            <fieldset class="min-w-0" data-testid="notifications-agents">
              <legend class="mb-2 text-callout font-medium text-muted">{t('noticeAgent')}</legend>
              {#if agentError}<Notice tone="warn" testid="notifications-agent-error">{agentError}
                <button class={buttonClass('ghost', 'xs')} onclick={readAgents}>{t('retry')}</button></Notice>{/if}
              {#if agents.length}
                <ul class="divide-y divide-line rounded-xl border border-line bg-surface">
                  {#each agents as agent (agent.agent_id)}
                    <li class="flex items-center gap-3 p-3">
                      <Checkbox class="flex min-w-0 flex-1 [&>span]:min-w-0 [&>span]:[overflow-wrap:anywhere]" checked={account.agent_mail[agent.agent_id] ?? false} onCheckedChange={(value) => { if (value || Object.hasOwn(preferences!.account?.agent_mail ?? {}, agent.agent_id)) account!.agent_mail[agent.agent_id] = value
                          else delete account!.agent_mail[agent.agent_id] }} disabled={busy || loading} label={agent.label || agent.agent_id} testid="notifications-agent"><bdi>{agent.label || agent.agent_id}</bdi></Checkbox>
                      <Id id={agent.agent_id} class="shrink-0" />
                    </li>
                  {/each}
                </ul>
              {:else if agentsLoading}<p class="text-callout text-muted" role="status">{t('loading')}</p>
              {:else if !loading && !error && !agentError}
                <p class="text-callout text-muted" data-testid="notifications-agents-empty">{t('noAgents')}</p>
              {/if}
            </fieldset>
          {/if}
        {/if}
      </div>
    </Section>
    {#if !preferences.remote_supported}<Notice icon="info">{t('noticeRemote')}</Notice>{/if}
    {#if pending}<Notice icon="sync">{t('noticePending')}<p class="mt-1 text-callout">{t('noticePendingHint')}</p></Notice>{/if}
    {#if conflict}<Notice tone="warn" icon="info">{t('conflictItem')} {t('retry')}</Notice>{/if}
    <div class={`grid gap-2 rounded-lg bg-paper px-3 py-2 sm:px-5 ${dirty || conflict || error || justSaved ? 'sticky bottom-0 z-10 border border-line shadow-sm' : ''}`} data-testid="notifications-actions">
      {#if error}<Notice tone="danger" testid="notifications-error">{error}</Notice>{/if}
      <div class="flex flex-wrap items-center gap-2">
        {#if dirty}<span role="status" class="me-auto text-callout font-medium">{t('settingsUnsaved')}</span>{:else if justSaved}<span role="status" class="me-auto text-callout text-ok" data-testid="notifications-saved">{t('savedValue')}</span>{/if}
        {#if dirty}<button class={buttonClass('ghost', 'sm')} disabled={busy || loading}
          onclick={() => { accept(preferences!); error = '' }} data-testid="notifications-discard">{t('discard')}</button>{/if}
        <button class={buttonClass('secondary', 'sm')} disabled={busy || loading} onclick={load} data-testid="notifications-refresh">{t('refresh')}</button>
        <button class={buttonClass('primary', 'sm')} disabled={(!dirty && !conflict) || invalidHours || busy || loading} aria-busy={saving}
          onclick={() => save()} data-testid="notifications-save">{saving ? t('loading') : conflict ? t('retry') : t('save')}</button>
      </div>
    </div>
  {:else if !loading}
    <button class={buttonClass('secondary', 'sm')} onclick={() => load()}>{t('retry')}</button>
  {/if}
</div>

<Confirm bind:open={linking} title={t('noticeLinkTitle')} description={t('noticeLinkBody')} confirm={t('noticeLinkConfirm')} icon="members" tone="warn" {busy} testid="notifications-link" onConfirm={() => save(true)} onReview={() => load()}>
  {#snippet body()}
    <p class="text-callout text-muted">{t('noticeLinkAccounts')}</p>
    <ul class="mt-2 flex flex-wrap gap-2" data-testid="notifications-link-accounts">
      {#each others as other (other)}<li><Id id={other} /></li>{/each}
    </ul>
  {/snippet}
</Confirm>
