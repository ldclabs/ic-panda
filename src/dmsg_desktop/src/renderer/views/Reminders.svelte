<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { onMount, untrack } from 'svelte'
  import { nameOf } from './channels/names.svelte'
  import Page from '../components/page/Page.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Icon from '../components/Icon.svelte'
  import Id from '../components/page/Id.svelte'
  import Segmented from '../components/Segmented.svelte'
  import Dialog from '../components/Dialog.svelte'
  import NoticeSetup from './NoticeSetup.svelte'
  import ReminderRow from './reminders/ReminderRow.svelte'
  import { localeDir } from '../../shared/locales'
  import { buttonClass, iconButtonClass, menuContentClass, menuItemClass } from '../components/ui'
  import { call, errorText, single } from '../rpc'
  import { locale, t } from '../i18n/index.svelte'
  import { nextStep, openAccounts, unlocking } from '../shell/session'
  import { tomorrowMorning } from '../../shared/notification-time'
  import type { NoticeItem, NoticeStatus } from '../../shared/notifications'
  import { noNames, noticeNames, noticeUnread } from './reminders/notice'

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
  let opening = $state(false), openingError = $state('')
  const context = $derived(`${status?.session ?? ''}:${selected}:${status?.state ?? ''}:${route}`)
  const query = $derived(new URLSearchParams(route.split('?')[1] ?? ''))
  const requestedAccount = $derived(query.get('account'))
  const wrongAccount = $derived(!!requestedAccount && requestedAccount !== selected)
  const step = $derived(nextStep(status))
  const incoming = $derived(query.has('item') || query.has('ended') || wrongAccount)
  const endedTitle = $derived(query.get('reason') === 'EXPIRED' ? t('reminderExpired') : query.get('reason') === 'FORBIDDEN' ? t('reminderForbidden') : t('reminderUnavailable'))
  const valid = (turn: number) => alive && generation === turn

  async function load(append = false) {
    const turn = ++generation, account = selected, filter = scope
    loading = true
    error = ''
    try {
      if (!account || incoming) { items = []; cursor = null; return }
      const [page, counts, known] = await Promise.all([
        call<{ items: NoticeItem[]; next_cursor: string | null }>('notifications/list', { account, scope: filter, cursor: append ? cursor : null, limit: 50 }),
        call<NoticeStatus>('notifications/status'),
        status?.state === 'unlocked' ? noticeNames() : Promise.resolve(noNames())
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
      names = noNames()
      openingError = ''
      opening = false
      busy = false
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
    const current = context
    busy = true; error = ''
    try { await window.dmsg.notificationOpen(item.account, item.id, item.version) }
    catch (cause) { if (alive && context === current) error = errorText(cause) }
    finally { if (alive && context === current) busy = false }
  }

  async function continueOpening() {
    const id = query.get('item'), current = context
    if (!id || wrongAccount || status?.state !== 'unlocked') return
    opened = current; opening = true; openingError = ''
    try { await window.dmsg.notificationOpen(selected, id, query.get('version') ?? undefined) }
    catch (cause) { if (alive && context === current) openingError = errorText(cause) }
    finally { if (alive && context === current) opening = false }
  }

  // Keep the original identity and displayed version across explicit account/unlock actions.
  $effect(() => {
    if (!query.has('item') || opened === context || query.has('ended') || wrongAccount || status?.state !== 'unlocked') return
    untrack(() => void continueOpening())
  })

  async function clear() {
    busy = true
    try { await call('notifications/clear_recent', { account: selected }); if (alive) await reload() }
    catch (cause) { if (alive) error = errorText(cause) }
    finally { if (alive) busy = false }
  }

  async function markRead(item?: NoticeItem) {
    busy = true; error = ''
    try {
      if (item) await call('notifications/mark_read', { account: item.account, id: item.id, version: item.version })
      else await call('notifications/mark_all_read')
      if (alive) await reload()
    } catch (cause) { if (alive) error = errorText(cause) }
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
  {#if incoming}
    <section class="grid max-w-xl gap-5 py-4" aria-labelledby="reminder-state-title" data-testid="reminder-state">
      <Icon name={query.has('ended') ? 'info' : wrongAccount ? 'account' : step ? 'lock' : 'bell'} class="size-6 text-muted" />
      <h2 id="reminder-state-title" class="text-[17px] font-semibold text-ink [overflow-wrap:anywhere]">{query.has('ended') ? endedTitle : wrongAccount ? t('reminderOtherAccount') : step ? t('reminderUnlockTitle') : openingError ? t('reminderOpenFailed') : t('reminderOpening')}</h2>
      <p class="text-[13.5px] leading-relaxed text-muted">{query.has('ended') ? t('reminderUnavailableHint') : wrongAccount ? t('reminderAccountHint') : step ? t('reminderUnlockHint') : ''}</p>
      {#if wrongAccount && !query.has('ended')}
        <div class="flex flex-wrap items-center gap-2" data-testid="reminders-original-account">{#if nameOf(requestedAccount!, false)}<span class="font-medium">{nameOf(requestedAccount!, false)}</span>{/if}<Id id={requestedAccount!} /></div>
        <div><button class={buttonClass('primary')} disabled={$unlocking} onclick={openAccounts} data-testid="reminders-choose-account"><Icon name="account" />{t('localAccounts')}</button></div>
      {:else if step && !query.has('ended')}
        <div><button class={buttonClass('primary')} disabled={$unlocking} onclick={step.run} data-testid="reminders-unlock"><Icon name={step.icon} />{t(step.label)}</button></div>
      {:else if !query.has('ended')}
        {#if openingError}<Notice tone="danger">{openingError}</Notice>{/if}
        {#if opening}<p role="status" class="text-muted">{t('loading')}</p>
        {:else}<div><button class={buttonClass('primary')} onclick={continueOpening} data-testid="reminders-retry">{t('retry')}</button></div>{/if}
      {/if}
      <div><button class={buttonClass('ghost')} onclick={() => go('/reminders')} data-testid="reminders-back">{t('reminderBack')}</button></div>
    </section>
  {:else}
    <div class="grid gap-4">
      <div class="flex flex-wrap items-center gap-2">
        <Segmented bind:value={scope} options={[{value:'open',label:t('noticeNeedsAction')},{value:'recent',label:t('noticeRecent')}]} label={t('noticeTitle')} disabled={busy} testid="reminders-scope" />
        <span class="min-w-0 flex-1"></span>
        <button class={iconButtonClass()} title={t('refresh')} aria-label={t('refresh')} disabled={loading || busy} onclick={() => reload()}><Icon name="sync-now" /></button>
        <DropdownMenu.Root dir={localeDir(locale())}>
          <DropdownMenu.Trigger class={iconButtonClass()} aria-label={t('moreActions')} title={t('moreActions')} data-testid="reminders-menu"><Icon name="more" /></DropdownMenu.Trigger>
          <DropdownMenu.Portal><DropdownMenu.Content class={`${menuContentClass} max-w-80`}>
            <DropdownMenu.Item class={`${menuItemClass} whitespace-normal`} disabled={loading || busy || !(items.some(noticeUnread) || summary?.counts.actionable || summary?.counts.unread)} onSelect={() => markRead()} data-testid="reminders-read-all">{t('reminderReadAll')}</DropdownMenu.Item>
            <p class="px-3 py-2 text-xs text-muted">{t('noticeMarkReadHint')}</p>
            <DropdownMenu.Separator class="my-1 h-px bg-line" />
            <DropdownMenu.Item class={menuItemClass} onSelect={() => go('/settings/notifications')}>{t('noticeSettings')}</DropdownMenu.Item>
            {#if scope === 'recent'}<DropdownMenu.Item class={menuItemClass} disabled={busy || !selected} onSelect={clear}>{t('noticeClear')}</DropdownMenu.Item>{/if}
          </DropdownMenu.Content></DropdownMenu.Portal>
        </DropdownMenu.Root>
      </div>
      {#if error}<Notice failure={error} tone="danger">{error}</Notice><div><button class={buttonClass('secondary','sm')} disabled={loading || busy} onclick={() => reload()}>{t('retry')}</button></div>{/if}
      {#if step}<Notice icon="lock">{t('reminderUnlockHint')}{#snippet action()}<button class={buttonClass('secondary','sm')} disabled={$unlocking} onclick={step.run}>{t(step.label)}</button>{/snippet}</Notice>{/if}
      {#if counts?.unverified}<div class="flex flex-wrap items-center gap-2 text-xs text-muted" role="status" data-testid="reminders-unverified"><span>{t('noticeVerifyHint')}</span>{#if scope === 'open'}<button class={buttonClass('ghost','xs')} onclick={() => scope = 'recent'}>{t('noticeRecent')}</button>{/if}</div>{/if}
      <ul class="divide-y divide-line" data-testid="reminders-list">
        {#each items as item (item.id)}
          <li class="flex items-start gap-1 rounded-lg pe-1 transition-colors duration-100 hover:bg-wash" data-testid="reminder-item">
            <button class="flex min-w-0 flex-1 items-start gap-3 rounded-lg p-3 text-start outline-none focus-visible:ring-3 focus-visible:ring-inset focus-visible:ring-focus/35" disabled={busy} onclick={() => open(item)} data-testid="reminder-open"><ReminderRow {item} {names} /></button>
            <DropdownMenu.Root dir={localeDir(locale())}>
              <DropdownMenu.Trigger class={iconButtonClass('ghost','md','mt-2 shrink-0')} aria-label={t('moreActions')} title={t('moreActions')} data-testid="reminder-menu"><Icon name="more" /></DropdownMenu.Trigger>
              <DropdownMenu.Portal><DropdownMenu.Content class={menuContentClass}>
                {#if noticeUnread(item)}<DropdownMenu.Item class={menuItemClass} disabled={busy} onSelect={() => markRead(item)} data-testid="reminder-read">{t('markRead')}</DropdownMenu.Item>{/if}
                <DropdownMenu.Item class={menuItemClass} disabled={busy} onSelect={() => open(item)}>{t('open')}</DropdownMenu.Item>
                {#if item.open && item.actionable}<DropdownMenu.Item class={menuItemClass} disabled={busy} onSelect={() => { laterItem = item; laterNow = Date.now(); later = true }}>{t('noticeLater')}</DropdownMenu.Item>{/if}
              </DropdownMenu.Content></DropdownMenu.Portal>
            </DropdownMenu.Root>
          </li>
        {:else}
          {#if !error}<li class="grid justify-items-center gap-2 px-5 py-12 text-center" role="status"><Icon name="bell" class="size-6 text-faint" /><span class="text-[14px] font-medium">{loading ? t('loading') : counts?.unverified ? t('noticeVerifyHint') : scope === 'open' ? t('reminderEmptyActions') : t('reminderEmpty')}</span>{#if !loading && !counts?.unverified && scope === 'open'}<span class="text-[12.5px] text-muted">{t('reminderEmptyHint')}</span>{/if}</li>{/if}
        {/each}
      </ul>
      {#if cursor}<div><button class={buttonClass('secondary','sm')} disabled={loading || busy} onclick={() => load(true)}>{t('more')}</button></div>{/if}
      <NoticeSetup {status} {go} />
    </div>
  {/if}
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
