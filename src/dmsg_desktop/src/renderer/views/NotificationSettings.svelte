<script lang="ts">
  import { onMount, untrack } from 'svelte'
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
  import { call, code, errorText, when } from '../rpc'
  import { t, stateLabel, type Label } from '../i18n/index.svelte'
  import { shortId } from '../shell/session'
  import { tomorrowMorning } from '../../shared/notification-time'
  import { noticeCategories, type GlobalNoticeSettings, type NoticeCategory, type NoticePermission, type NoticePreferences, type NoticeSettings, type NoticeStatus } from '../../shared/notifications'

  let { status }: { status: any } = $props()
  let preferences = $state<NoticePreferences | null>(null)
  let global = $state<GlobalNoticeSettings | null>(null)
  let account = $state<NoticeSettings | null>(null)
  let selected = $state('')
  let accounts = $state<{ account: string; bound: boolean }[]>([])
  let agents = $state<{ agent_id: string; label: string }[]>([])
  let permission = $state<NoticePermission | null>(null)
  /** Other local accounts already receiving notifications on this device. */
  let others = $state<string[]>([])
  let linking = $state(false)
  let loading = $state(true), busy = $state(false), error = $state('')
  let alive = true, generation = 0
  const scope = $derived(`${status?.account ?? ''}:${status?.session ?? ''}`)
  const categoryLabels: Record<NoticeCategory, Label> = { security: 'security', action: 'approvals', message: 'noticeMessages', service: 'noticeServices' }
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone
  const zones = [...new Set([zone, ...Intl.supportedValuesOf('timeZone')])].sort().map((value) => ({ value, label: value }))
  const times = Array.from({ length: 96 }, (_, index) => ({ value: String(index * 15), label: `${String(Math.floor(index / 4)).padStart(2, '0')}:${String(index % 4 * 15).padStart(2, '0')}` }))
  const dirty = $derived(!!preferences && (JSON.stringify(global) !== JSON.stringify(preferences.global) || JSON.stringify(account) !== JSON.stringify(preferences.account)))
  const pending = $derived(!!preferences?.global_pending_sync || (!!account && account.revision > account.remote_revision))
  const conflict = $derived(!!preferences?.global_conflict || !!preferences?.account_conflict)
  const valid = (turn: number) => alive && generation === turn
  function clone<T>(value: T): T { return JSON.parse(JSON.stringify(value)) as T }

  async function load(id = selected, replace = true) {
    const turn = ++generation
    loading = true
    error = ''
    try {
      const [next, local, platform, counts] = await Promise.all([
        call<NoticePreferences>('notifications/settings', id ? { account: id } : {}),
        call<{ account: string; bound: boolean }[]>('session/accounts'),
        window.dmsg.notificationPermission(),
        call<NoticeStatus>('notifications/status')
      ])
      if (!valid(turn)) return
      preferences = next
      accounts = local.filter((entry) => entry.bound)
      others = counts.accounts.filter((entry) => entry.enabled && entry.account !== id).map((entry) => entry.account)
      permission = platform
      if (replace || !global) global = clone(next.global)
      if (replace || !account) account = clone(next.account)
      agents = []
      if (status?.state === 'unlocked' && status.account === id) {
        const result = await call<{ agents: typeof agents }>('agent/ids')
        if (valid(turn)) agents = result.agents
      }
    } catch (cause) {
      if (valid(turn)) error = errorText(cause)
    } finally { if (valid(turn)) loading = false }
  }

  $effect(() => {
    void scope
    untrack(() => { selected = status?.account ?? ''; void load(selected) })
  })

  onMount(() => {
    const off = window.dmsg.onEvent((event) => {
      if (event.type === 'notifications/changed' && !busy && !loading) {
        const turn = generation, id = selected, keepDraft = dirty
        void call<NoticePreferences>('notifications/settings', id ? { account: id } : {}).then((next) => {
          if (!valid(turn) || busy) return
          const preserve = keepDraft || dirty
          preferences = next
          if (!preserve) { global = clone(next.global); account = clone(next.account) }
        }, () => {})
        return
      }
      if (event.type !== 'notifications/platform') return
      const turn = generation
      void window.dmsg.notificationPermission().then((value) => { if (valid(turn)) permission = value }, () => {})
    })
    return () => { alive = false; generation++; off() }
  })

  function enable(value: boolean) {
    if (!global) return
    global.enabled = value
    // The first explicit opt-in selects only the current account.
    if (value && preferences?.global.revision === 0 && account?.revision === 0 && selected === status?.account) account.enabled = true
  }

  async function save(linked = false) {
    if (!preferences || !global || busy) return
    // Registering another account tells the notification service and the push
    // provider that these accounts share this device; that needs consent.
    if (!linked && account?.enabled && !preferences.account?.enabled && others.length) { linking = true; return }
    const turn = generation, id = selected
    busy = true
    error = ''
    try {
      if (JSON.stringify(global) !== JSON.stringify(preferences.global) || preferences.global_conflict) {
        const result = await call<{ settings: GlobalNoticeSettings }>('notifications/settings_set', { scope: 'installation', expected_revision: preferences.global.revision, settings: clone(global) })
        if (!valid(turn)) return
        preferences.global = result.settings
        global = clone(result.settings)
      }
      if (account && preferences.account && (JSON.stringify(account) !== JSON.stringify(preferences.account) || preferences.account_conflict)) {
        const result = await call<{ settings: NoticeSettings }>('notifications/settings_set', { scope: 'account', account: id, expected_revision: preferences.account.revision, settings: clone(account) })
        if (!valid(turn)) return
        preferences.account = result.settings
        account = clone(result.settings)
      }
    } catch (cause) {
      if (!valid(turn)) return
      // Re-read the version after a conflict, retaining the person's draft. A
      // second explicit Save is required; no automatic overwrite/retry occurs.
      if (code(cause) === 'VERSION_CONFLICT') await load(id, false)
      if (alive) error = errorText(cause)
    } finally { if (alive) busy = false }
  }

  async function pause(until: number | null) {
    if (!preferences || !global || busy) return
    const turn = generation
    busy = true
    error = ''
    try {
      const result = await call<{ settings: GlobalNoticeSettings }>('notifications/settings_set', {
        scope: 'installation', expected_revision: preferences.global.revision,
        settings: { ...clone(preferences.global), paused_until: until }
      })
      if (!valid(turn)) return
      preferences.global = result.settings
      // Leave unrelated unsaved fields alone.
      global = { ...global, revision: result.settings.revision, paused_until: until }
    } catch (cause) { if (valid(turn)) error = errorText(cause) }
    finally { if (valid(turn)) busy = false }
  }

  async function test() {
    busy = true
    try { await window.dmsg.notificationTest(); if (alive) toast(() => t('noticeTestSent')) }
    catch (cause) { if (alive) error = errorText(cause) }
    finally { if (alive) busy = false }
  }
</script>

<div class="space-y-6" data-testid="notification-settings">
  {#if error}<Notice failure={error} tone="danger" testid="notifications-error">{error}</Notice>{/if}
  {#if loading && !global}<p class="px-5 text-sm text-muted" role="status">{t('loading')}</p>{/if}
  {#if global && preferences}
    <Section title={t('noticeSettings')} description={t('noticeScopeHint')} icon="time">
      <div class="space-y-4">
        <Checkbox checked={global.enabled} onCheckedChange={enable} disabled={busy || loading} label={t('noticeEnable')} testid="notifications-enabled">{t('noticeEnable')}</Checkbox>
        <p class="text-xs text-muted">{permission?.supported ? t('noticeManaged') : t('noticeUnsupported')}</p>
        <div class="flex flex-wrap gap-2">
          <button class={buttonClass('secondary', 'sm')} disabled={busy || !permission?.supported} onclick={test} data-testid="notifications-test">{t('noticeTest')}</button>
          <button class={buttonClass('ghost', 'sm')} disabled={!permission?.settings_available} onclick={() => window.dmsg.notificationSettings()}>{t('noticeSystem')}</button>
        </div>
        {#if permission?.last_result}<p class="text-xs text-muted" role="status">{permission.last_result === 'failed' ? stateLabel('failed') : t('noticeTestSent')}</p>{/if}
        <div class="flex flex-wrap gap-2">
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => pause(Date.now() + 900_000)}>{t('noticePause15')}</button>
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => pause(Date.now() + 3_600_000)}>{t('noticePauseHour')}</button>
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => pause(tomorrowMorning(Date.now(), zone))}>{t('noticePauseTomorrow')}</button>
          {#if global.paused_until && global.paused_until > Date.now()}<button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => pause(null)}>{t('noticeResume')}</button>{/if}
        </div>
        {#if global.paused_until && global.paused_until > Date.now()}<p class="text-xs text-muted">{t('noticeUntil')} {when(global.paused_until)}</p>{/if}
        <Checkbox class="flex" checked={global.quiet_hours !== null} disabled={busy || loading} onCheckedChange={(enabled) => { global!.quiet_hours = enabled ? { start: 1320, end: 480, timezone: zone } : null }} label={t('noticeQuiet')} testid="notifications-quiet">{t('noticeQuiet')}</Checkbox>
        {#if global.quiet_hours}
          <div class="flex flex-wrap gap-4">
            <Field label={t('noticeFrom')}><Select value={String(global.quiet_hours.start)} options={times} label={t('noticeFrom')} onChange={(value) => { global!.quiet_hours!.start = Number(value) }} disabled={busy} testid="notifications-quiet-from" /></Field>
            <Field label={t('noticeUntil')}><Select value={String(global.quiet_hours.end)} options={times} label={t('noticeUntil')} onChange={(value) => { global!.quiet_hours!.end = Number(value) }} disabled={busy} testid="notifications-quiet-until" /></Field>
            <Field label={t('noticeZone')} class="min-w-40 flex-1"><SearchSelect value={global.quiet_hours.timezone} options={zones} label={t('noticeZone')} onChange={(value) => { global!.quiet_hours!.timezone = value }} disabled={busy} /></Field>
          </div>
        {/if}
        <Checkbox class="flex" bind:checked={global.security_exception} disabled={busy || loading} label={t('noticeSecurityException')}>{t('noticeSecurityException')}</Checkbox>
        <Field label={t('noticeBadge')}><Select value={global.badge} options={[{value:'all',label:t('noticeBadgeAll')},{value:'actions',label:t('noticeBadgeActions')},{value:'off',label:t('noticeBadgeOff')}]} label={t('noticeBadge')} onChange={(value) => { global!.badge = value as GlobalNoticeSettings['badge'] }} disabled={busy || loading} /></Field>
      </div>
    </Section>

    <Section title={t('account')} icon="account">
      <div class="space-y-4">
        <Select value={selected} options={accounts.map((entry) => ({value:entry.account,label:shortId(entry.account)}))} label={t('account')} placeholder={t('account')} disabled={busy || loading || !accounts.length} onChange={(id) => { selected = id; void load(id) }} testid="notifications-account" />
        {#if !accounts.length}<Notice>{t('loggedOutHint')}</Notice>{/if}
        {#if account}
          <Checkbox bind:checked={account.enabled} disabled={busy || loading} label={t('noticeAccount')} testid="notifications-account-enabled">{t('noticeAccount')}</Checkbox>
          <div class="divide-y divide-line rounded-xl border border-line">
            {#each noticeCategories as category}
              <div class="flex flex-wrap items-center justify-between gap-3 p-3">
                <Checkbox bind:checked={account.categories[category].enabled} disabled={busy || loading} label={t(categoryLabels[category])}>{t(categoryLabels[category])}</Checkbox>
                <Checkbox bind:checked={account.categories[category].sound} disabled={busy || loading || !account.categories[category].enabled} label={`${t(categoryLabels[category])}: ${t('noticeSound')}`}>{t('noticeSound')}</Checkbox>
              </div>
            {/each}
          </div>
          <Checkbox bind:checked={account.merge_devices} disabled={busy || loading} label={t('noticeMerge')}>{t('noticeMerge')}</Checkbox>
          {#if agents.length}
            <p class="text-xs font-medium text-muted">{t('noticeAgent')}</p>
            {#each agents as agent (agent.agent_id)}
              <Checkbox checked={account.agent_mail[agent.agent_id] ?? false} onCheckedChange={(value) => { account!.agent_mail[agent.agent_id] = value }} disabled={busy || loading} label={agent.label}>{agent.label}</Checkbox>
            {/each}
          {/if}
        {/if}
      </div>
    </Section>
    {#if !preferences.remote_supported}<Notice icon="info">{t('noticeRemote')}</Notice>{/if}
    {#if pending}<Notice icon="sync">{t('noticePending')}<p class="mt-1 text-xs">{t('noticePendingHint')}</p></Notice>{/if}
    {#if conflict}<Notice tone="warn" icon="info">{t('conflictItem')} {t('retry')}</Notice>{/if}
    <div class="flex flex-wrap gap-2 px-5 pb-5">
      <button class={buttonClass('primary', 'sm')} disabled={(!dirty && !conflict) || busy || loading} onclick={() => save()} data-testid="notifications-save">{busy ? t('loading') : conflict ? t('retry') : t('save')}</button>
      <button class={buttonClass('secondary', 'sm')} disabled={busy || loading} onclick={() => load(selected)}>{t('refresh')}</button>
    </div>
  {:else if !loading}
    <button class={buttonClass('secondary', 'sm')} onclick={() => load()}>{t('retry')}</button>
  {/if}
</div>

<Confirm bind:open={linking} title={t('noticeLinkTitle')} description={t('noticeLinkBody')} confirm={t('noticeLinkConfirm')} icon="members" tone="warn" {busy} testid="notifications-link" onConfirm={() => save(true)}>
  {#snippet body()}
    <p class="text-[12.5px] text-muted">{t('noticeLinkAccounts')}</p>
    <ul class="mt-2 flex flex-wrap gap-2" data-testid="notifications-link-accounts">
      {#each others as other (other)}<li><Id id={other} /></li>{/each}
    </ul>
  {/snippet}
</Confirm>
