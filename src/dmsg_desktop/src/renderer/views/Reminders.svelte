<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Page from '../components/page/Page.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Icon from '../components/Icon.svelte'
  import Id from '../components/page/Id.svelte'
  import Segmented from '../components/Segmented.svelte'
  import Dialog from '../components/Dialog.svelte'
  import NoticeSetup from './NoticeSetup.svelte'
  import { buttonClass } from '../components/ui'
  import { call, errorText, single, when } from '../rpc'
  import { t } from '../i18n/index.svelte'
  import { nextStep } from '../shell/session'
  import { tomorrowMorning } from '../../shared/notification-time'
  import type { NoticeItem, NoticeStatus } from '../../shared/notifications'
  import { category, icons, noNames, noticeNames, noticeTitle } from './reminders/notice'

  let { status, route, go }: { status: any; route: string; go: (path: string) => void } = $props()
  const selected = $derived<string>(status?.account ?? '')
  let scope = $state('open')
  let items = $state<NoticeItem[]>([])
  let cursor = $state<string | null>(null)
  let summary = $state<NoticeStatus | null>(null)
  const counts = $derived(summary?.accounts.find((entry) => entry.account === selected)?.counts)
  let names = $state.raw(noNames())
  let loading = $state(true), busy = $state(false), error = $state('')
  let later = $state(false), laterItem = $state<NoticeItem | null>(null), laterNow = $state(0)
  let alive = true, generation = 0, opened = ''
  const context = $derived(`${status?.session ?? ''}:${selected}:${status?.state ?? ''}:${route}`)
  const query = $derived(new URLSearchParams(route.split('?')[1] ?? ''))
  const requestedAccount = $derived(query.get('account'))
  const wrongAccount = $derived(!!requestedAccount && requestedAccount !== selected)
  const step = $derived(nextStep(status))
  const valid = (turn: number) => alive && generation === turn

  async function load(append = false) {
    const turn = ++generation, account = selected, filter = scope
    loading = true
    error = ''
    try {
      if (!account) { items = []; cursor = null; return }
      const [page, counts, known] = await Promise.all([
        call<{ items: NoticeItem[]; next_cursor: string | null }>('notifications/list', { account, scope: filter, cursor: append ? cursor : null, limit: 50 }),
        call<NoticeStatus>('notifications/status'),
        noticeNames()
      ])
      if (!valid(turn) || selected !== account || scope !== filter) return
      const previous = append ? items : []
      items = [...new Map([...previous, ...page.items].map((item) => [item.id, item])).values()]
      cursor = page.next_cursor
      summary = counts
      names = known
    } catch (cause) { if (valid(turn)) error = errorText(cause) }
    finally { if (valid(turn)) loading = false }
  }
  const reload = single(async () => load())

  $effect(() => {
    void context
    untrack(() => {
      generation++
      scope = query.get('scope') === 'recent' ? 'recent' : 'open'
      items = []
      cursor = null
      summary = null
      later = false
      laterItem = null
      void reload()
    })
  })
  $effect(() => { void selected; void scope; untrack(() => { items = []; cursor = null; void reload() }) })

  onMount(() => {
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
    if (!id || opened === id || query.has('ended') || requestedAccount !== selected || status?.state !== 'unlocked') return
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
  <div class="grid gap-4">
    <div class="flex flex-wrap items-center gap-2">
      <Segmented bind:value={scope} options={[{value:'open',label:t('noticeNeedsAction')},{value:'recent',label:t('noticeRecent')}]} label={t('noticeTitle')} disabled={busy} testid="reminders-scope" />
      <span class="min-w-0 flex-1"></span>
      <button class={buttonClass('ghost', 'sm')} disabled={loading || busy} onclick={() => reload()}>{t('refresh')}</button>
      <button class={buttonClass('secondary', 'sm')} onclick={() => go('/settings/notifications')}>{t('noticeSettings')}</button>
    </div>
    <p class="text-[12.5px] text-muted">{t('noticeReadOnlyHint')}</p>
    <NoticeSetup {status} {go} />
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    {#if query.has('ended')}<Notice>{t('noticeEnded')}</Notice>{/if}
    {#if wrongAccount && !query.has('ended')}
      <div data-testid="reminders-original-account"><Notice icon="account">{t('noticeSwitch')} <Id id={requestedAccount!} /></Notice></div>
    {/if}
    {#if step}
      <div><button class={buttonClass('secondary', 'sm')} onclick={step.run}><Icon name={step.icon} />{t(step.label)}</button></div>
    {/if}
    {#if counts?.unverified}
      <div class="flex flex-wrap items-center gap-2 text-xs text-muted" role="status" data-testid="reminders-unverified">
        <span>{t('noticeVerifyHint')}</span>
        {#if scope === 'open'}<button class={buttonClass('ghost', 'xs')} onclick={() => { scope = 'recent' }}>{t('noticeRecent')}</button>{/if}
      </div>
    {/if}
    <ul class="divide-y divide-line overflow-hidden rounded-xl border border-line bg-surface" data-testid="reminders-list">
      {#each items as item (item.id)}
        <li class="flex items-start gap-2 pe-3 transition-colors duration-100 hover:bg-wash" data-testid="reminder-item">
          <button class="flex min-w-0 flex-1 items-start gap-3 p-3 text-start outline-none focus-visible:ring-3 focus-visible:ring-inset focus-visible:ring-focus/35" disabled={busy} onclick={() => open(item)} data-testid="reminder-open">
            <Icon name={icons[category(item)]} class="mt-0.5 size-5 shrink-0 text-forest" />
            <span class="grid min-w-0 flex-1 gap-1">
              <span class="text-[13.5px] font-medium [overflow-wrap:anywhere]">{noticeTitle(item, names)}</span>
              <span class="text-xs text-muted">{when(item.updated_at)}{#if item.deadline} · {t('validUntil')} {when(item.deadline)}{/if}</span>
              {#if item.snoozed_until}<span class="text-xs text-muted">{t('noticeLater')} · {when(item.snoozed_until)}</span>{/if}
              {#if item.unread && category(item) === 'message'}<span><Pill>{item.unread.toLocaleString()} · {t('noticeMessages')}</Pill></span>{/if}
            </span>
          </button>
          {#if item.open && item.actionable}<button class={buttonClass('ghost', 'xs', 'mt-3 shrink-0')} disabled={busy} onclick={() => { laterItem = item; laterNow = Date.now(); later = true }}>{t('noticeLater')}</button>{/if}
        </li>
      {:else}
        <li class="px-5 py-10 text-center text-sm text-muted" role="status">{loading ? t('loading') : t('noticeEmpty')}</li>
      {/each}
    </ul>
    <div class="flex flex-wrap gap-2 empty:hidden">
      {#if cursor}<button class={buttonClass('secondary', 'sm')} disabled={loading || busy} onclick={() => load(true)}>{t('more')}</button>{/if}
      {#if scope === 'recent'}<button class={buttonClass('ghost', 'sm')} disabled={busy || !selected} onclick={clear}>{t('noticeClear')}</button>{/if}
    </div>
  </div>
</Page>

<Dialog bind:open={later} title={t('noticeLater')} icon="bell" dismissible={!busy} testid="reminder-snooze">
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
