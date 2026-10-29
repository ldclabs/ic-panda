<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Page from '../components/page/Page.svelte'
  import Section from '../components/page/Section.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Icon from '../components/Icon.svelte'
  import Select from '../components/Select.svelte'
  import Segmented from '../components/Segmented.svelte'
  import Dialog from '../components/Dialog.svelte'
  import NoticeSetup from './NoticeSetup.svelte'
  import { buttonClass } from '../components/ui'
  import { call, errorText, single, when } from '../rpc'
  import { locale, t } from '../i18n/index.svelte'
  import { nextStep, shortId, switchAccount } from '../shell/session'
  import { notificationBody } from '../../shared/notification-text'
  import { tomorrowMorning } from '../../shared/notification-time'
  import type { NoticeCategory, NoticeItem, NoticeStatus } from '../../shared/notifications'
  import type { Glyph } from '../components/glyphs'

  let { status, route, go }: { status: any; route: string; go: (path: string) => void } = $props()
  let selected = $state(''), scope = $state('open')
  let accounts = $state<{ account: string; bound: boolean }[]>([])
  let items = $state<NoticeItem[]>([])
  let cursor = $state<string | null>(null)
  let summary = $state<NoticeStatus | null>(null)
  let loading = $state(true), busy = $state(false), error = $state('')
  let later = $state(false), laterItem = $state<NoticeItem | null>(null), laterNow = $state(0)
  let alive = true, generation = 0, opened = ''
  const context = $derived(`${status?.session ?? ''}:${route}`)
  const query = $derived(new URLSearchParams(route.split('?')[1] ?? ''))
  const wrongAccount = $derived(!!selected && status?.account !== selected)
  const step = $derived(nextStep(status))
  const category = (item: NoticeItem): NoticeCategory => ['recovery'].includes(item.target.kind) ? 'security'
    : ['approval', 'wallet', 'task'].includes(item.target.kind) ? 'action'
    : ['plan', 'service'].includes(item.target.kind) ? 'service'
    : item.target.kind === 'summary' ? item.target.id : 'message'
  const icons: Record<NoticeCategory, Glyph> = { security: 'security', action: 'approvals', message: 'channels', service: 'plan' }
  function title(item: NoticeItem): string {
    switch (item.target.kind) {
      case 'recovery': return t('pendingRecovery')
      case 'approval': return t('approvals')
      case 'wallet': return `${t('twNav')} · ${t('approvals')}`
      case 'task': return item.target.id.startsWith('conflicts:') ? t('conflict') : t('syncFailed')
      case 'service': return item.target.id.startsWith('network:') ? t('networkUnavailable') : t('noticeServiceBody')
      case 'plan': return t('membership')
      case 'inbox': return t('inbox')
      case 'channel': return t('channels')
      case 'agent_mail': return t('noticeAgent')
      default: return notificationBody(locale(), category(item), item.unread)
    }
  }
  const valid = (turn: number) => alive && generation === turn

  async function load(append = false) {
    const turn = ++generation, account = selected, filter = scope
    loading = true
    error = ''
    try {
      if (!account) { items = []; cursor = null; return }
      const [page, counts] = await Promise.all([
        call<{ items: NoticeItem[]; next_cursor: string | null }>('notifications/list', { account, scope: filter, cursor: append ? cursor : null, limit: 50 }),
        call<NoticeStatus>('notifications/status')
      ])
      if (!valid(turn) || selected !== account || scope !== filter) return
      const previous = append ? items : []
      items = [...new Map([...previous, ...page.items].map((item) => [item.id, item])).values()]
      cursor = page.next_cursor
      summary = counts
    } catch (cause) { if (valid(turn)) error = errorText(cause) }
    finally { if (valid(turn)) loading = false }
  }
  const reload = single(async () => load())

  $effect(() => {
    void context
    untrack(() => {
      selected = query.get('account') ?? status?.account ?? ''
      scope = query.get('scope') === 'recent' ? 'recent' : 'open'
      items = []
      cursor = null
      later = false
      void reload()
    })
  })
  $effect(() => { void selected; void scope; untrack(() => { items = []; cursor = null; void reload() }) })

  onMount(() => {
    void call<{ account: string; bound: boolean }[]>('session/accounts').then((value) => {
      if (!alive) return
      accounts = value.filter((account) => account.bound)
      if (!selected && accounts.length) selected = accounts[0].account
    }, (cause) => { if (alive) error = errorText(cause) })
    const off = window.dmsg.onEvent((event) => { if (event.type === 'notifications/changed') void reload() })
    return () => { alive = false; generation++; off() }
  })

  async function open(item: NoticeItem) {
    busy = true
    try { await window.dmsg.notificationOpen(item.account, item.id) }
    catch (cause) { if (alive) error = errorText(cause) }
    finally { if (alive) busy = false }
  }

  // A notification that needed an account switch/unlock keeps its original ID.
  // Reading this list itself never submits an observed/read receipt.
  $effect(() => {
    const id = query.get('item')
    if (!id || opened === id || query.has('ended') || wrongAccount || status?.state !== 'unlocked') return
    untrack(() => {
      opened = id
      void window.dmsg.notificationOpen(selected, id).catch((cause) => { if (alive) error = errorText(cause) })
    })
  })

  async function clear() {
    busy = true
    try { await call('notifications/clear_recent', { account: selected }); if (alive) await reload() }
    catch (cause) { if (alive) error = errorText(cause) }
    finally { if (alive) busy = false }
  }

  async function snooze(until: number) {
    if (!laterItem) return
    busy = true
    const item = laterItem
    try {
      await call('notifications/snooze', { account: item.account, id: item.id, version: item.version, until })
      if (alive) { later = false; await reload() }
    } catch (cause) { if (alive) error = errorText(cause) }
    finally { if (alive) busy = false }
  }
</script>

<Page testid="reminders-page">
  <Section title={t('noticeTitle')} description={t('noticeReadOnlyHint')} icon="time">
    {#snippet actions()}
      <button class={buttonClass('secondary', 'sm')} onclick={() => go('/settings/notifications')}>{t('noticeSettings')}</button>
      <button class={buttonClass('ghost', 'sm')} disabled={loading || busy} onclick={() => reload()}>{t('refresh')}</button>
    {/snippet}
    <div class="flex flex-wrap items-center gap-3">
      <Select bind:value={selected} options={accounts.map((account) => ({value:account.account,label:shortId(account.account)}))} label={t('account')} disabled={busy || !accounts.length} testid="reminders-account" />
      <Segmented bind:value={scope} options={[{value:'open',label:t('noticeNeedsAction')},{value:'recent',label:t('noticeRecent')}]} label={t('noticeTitle')} disabled={busy} testid="reminders-scope" />
    </div>
    <NoticeSetup {status} {go} class="mt-4" />
    {#if error}<div class="mt-4"><Notice failure={error} tone="danger">{error}</Notice></div>{/if}
    {#if query.has('ended')}<div class="mt-4"><Notice>{t('noticeEnded')}</Notice></div>{/if}
    {#if wrongAccount}
      <div class="mt-4"><Notice icon="account">{accounts.some((account) => account.account === selected) ? t('noticeSwitch') : t('noticeEnded')}{#if accounts.some((account) => account.account === selected)}<button class={buttonClass('secondary', 'sm', 'ms-3')} disabled={busy} onclick={() => switchAccount(selected, false)}>{shortId(selected)}</button>{/if}</Notice></div>
    {:else if step}
      <div class="mt-4"><button class={buttonClass('secondary', 'sm')} onclick={step.run}><Icon name={step.icon} />{t(step.label)}</button></div>
    {/if}
    {#if summary?.counts.unverified}<p class="mt-3 text-xs text-muted" role="status">{t('sync')} · {t('loading')}</p>{/if}
    <ul class="mt-5 divide-y divide-line overflow-hidden rounded-xl border border-line bg-surface" data-testid="reminders-list">
      {#each items as item (item.id)}
        <li class="flex items-start gap-3 p-4" data-testid="reminder-item">
          <Icon name={icons[category(item)]} class="mt-1 size-5 shrink-0 text-forest" />
          <div class="min-w-0 flex-1 space-y-1">
            <p class="text-[13.5px] font-medium [overflow-wrap:anywhere]">{title(item)}</p>
            <p class="text-xs text-muted">{when(item.updated_at)}{#if item.deadline} · {t('validUntil')} {when(item.deadline)}{/if}</p>
            {#if item.snoozed_until}<p class="text-xs text-muted">{t('noticeLater')} · {when(item.snoozed_until)}</p>{/if}
            {#if item.unread && category(item) === 'message'}<Pill>{item.unread.toLocaleString()} · {t('noticeMessages')}</Pill>{/if}
            <div class="flex flex-wrap gap-2 pt-2">
              <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => open(item)}>{t('open')}</button>
              {#if item.open && item.actionable}<button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => { laterItem = item; laterNow = Date.now(); later = true }}>{t('noticeLater')}</button>{/if}
            </div>
          </div>
        </li>
      {:else}
        <li class="px-5 py-10 text-center text-sm text-muted" role="status">{loading ? t('loading') : t('noticeEmpty')}</li>
      {/each}
    </ul>
    <div class="mt-3 flex flex-wrap gap-2">
      {#if cursor}<button class={buttonClass('secondary', 'sm')} disabled={loading || busy} onclick={() => load(true)}>{t('more')}</button>{/if}
      {#if scope === 'recent'}<button class={buttonClass('ghost', 'sm')} disabled={busy || !selected} onclick={clear}>{t('noticeClear')}</button>{/if}
    </div>
  </Section>
</Page>

<Dialog bind:open={later} title={t('noticeLater')} icon="time" dismissible={!busy} testid="reminder-snooze">
  {#if laterItem}
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    <div class="grid gap-3">
      {#each [{ label: t('noticePause15'), until: laterNow + 900_000 }, { label: t('noticePauseHour'), until: laterNow + 3_600_000 }, { label: t('noticePauseTomorrow'), until: tomorrowMorning(laterNow, Intl.DateTimeFormat().resolvedOptions().timeZone) }] as choice}
        {@const late = laterItem.deadline !== null && choice.until >= laterItem.deadline}
        <div>
          <button class={buttonClass('secondary', 'sm')} disabled={busy || late} onclick={() => snooze(choice.until)}>{choice.label}</button>
          {#if late}<p class="mt-1 text-xs text-muted">{t('noticeLate')}</p>{/if}
        </div>
      {/each}
    </div>
  {/if}
</Dialog>
