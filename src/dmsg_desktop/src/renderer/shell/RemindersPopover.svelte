<script lang="ts">
  import { DropdownMenu, Popover } from 'bits-ui'
  import { onMount, untrack } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Segmented from '../components/Segmented.svelte'
  import ReminderRow from '../views/reminders/ReminderRow.svelte'
  import { localeDir } from '../../shared/locales'
  import { buttonClass, iconButtonClass, menuContentClass, menuItemClass } from '../components/ui'
  import { locale, t } from '../i18n/index.svelte'
  import { call, errorText, single } from '../rpc'
  import { noNames, noticeNames, noticeUnread } from '../views/reminders/notice'
  import type { NoticeItem } from '../../shared/notifications'

  /** The bell in the title bar: what wants attention, without leaving the view. The full list stays a page. */
  let { status, count = 0, go }: { status: any; count?: number; go: (path: string) => void } = $props()

  let open = $state(false)
  let scope = $state('open')
  let items = $state<NoticeItem[]>([])
  let names = $state.raw(noNames())
  let loading = $state(false)
  let error = $state('')
  let busy = $state(false)
  let run = 0, alive = true
  const context = $derived(`${status?.session}:${status?.account}:${status?.state}`)
  $effect(() => { void context; untrack(() => { run++; open = false; items = []; names = noNames(); error = ''; busy = false }) })

  const account = $derived<string>(status?.account ?? '')

  const load = single(async () => {
    const turn = ++run
    const selected = account, filter = scope
    if (!account) { items = []; return }
    loading = true
    error = ''
    try {
      const [page, known] = await Promise.all([
        call<{ items: NoticeItem[] }>('notifications/list', { account, scope, cursor: null, limit: 20 }),
        status?.state === 'unlocked' ? noticeNames() : Promise.resolve(noNames())
      ])
      if (turn !== run || selected !== account || filter !== scope) return
      items = page.items
      names = known
    } catch (e) {
      if (turn === run) error = errorText(e)
    } finally {
      if (turn === run) loading = false
    }
  })

  $effect(() => {
    void scope
    void account
    if (open) untrack(() => void load())
  })

  async function follow(item: NoticeItem) {
    const selected = context
    busy = true
    try {
      await window.dmsg.notificationOpen(item.account, item.id, item.version)
      if (alive && context === selected) open = false
    } catch (e) {
      if (alive && context === selected) { error = errorText(e); open = true }
    } finally { if (alive && context === selected) busy = false }
  }

  async function markRead(item: NoticeItem) {
    const selected = context
    busy = true; error = ''
    try {
      await call('notifications/mark_read', { account: item.account, id: item.id, version: item.version })
      if (alive && selected === context) await load()
    } catch (cause) { if (alive && selected === context) error = errorText(cause) }
    finally { if (alive && selected === context) busy = false }
  }

  onMount(() => {
    const off = window.dmsg.onEvent((event) => {
      if (open && event.type === 'notifications/changed') void load()
    })
    return () => { alive = false; run++; off() }
  })
</script>

<Popover.Root bind:open>
  <Popover.Trigger class={iconButtonClass('ghost', 'lg', 'relative shrink-0 [-webkit-app-region:no-drag]')} aria-label={`${t('noticeTitle')}${count ? ` (${count})` : ''}`} title={t('noticeTitle')} data-testid="reminders-open">
    <Icon name="bell" />
    {#if count > 0}<span class="absolute -end-1 -top-1 rounded-full bg-gem-pale px-1 text-[9px] leading-4 font-semibold text-gem" data-testid="reminders-count">{count > 99 ? '99+' : count}</span>{/if}
  </Popover.Trigger>
  <Popover.Portal>
    <Popover.Content align="end" sideOffset={6} class="z-50 grid max-h-[min(460px,80vh)] w-[min(360px,calc(100vw-24px))] grid-rows-[auto_1fr_auto] rounded-xl border border-line bg-surface shadow-pop outline-none" data-testid="reminders-popover">
      <div class="border-b border-line p-2.5">
        <Segmented bind:value={scope} options={[{ value: 'open', label: t('noticeNeedsAction') }, { value: 'recent', label: t('noticeRecent') }]} label={t('noticeTitle')} testid="reminders-popover-scope" />
      </div>
      <div class="min-h-24 overflow-y-auto p-1.5">
        {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
        <ul data-testid="reminders-popover-list">
          {#each items as item (item.id)}
            <li class="flex items-start gap-1">
              <button class="flex min-w-0 flex-1 items-start gap-2.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 hover:bg-wash focus-visible:ring-3 focus-visible:ring-focus/35" disabled={busy} onclick={() => follow(item)} data-testid="reminder-row">
                <ReminderRow {item} {names} compact />
              </button>
              <DropdownMenu.Root dir={localeDir(locale())}>
                <DropdownMenu.Trigger class={iconButtonClass('ghost','md','mt-1 shrink-0')} aria-label={t('moreActions')} title={t('moreActions')} data-testid="reminder-popover-menu"><Icon name="more" /></DropdownMenu.Trigger>
                <DropdownMenu.Portal><DropdownMenu.Content class={menuContentClass}>
                  <DropdownMenu.Item class={menuItemClass} onSelect={() => follow(item)}>{t('open')}</DropdownMenu.Item>
                  {#if noticeUnread(item)}<DropdownMenu.Item class={menuItemClass} disabled={busy} onSelect={() => markRead(item)} data-testid="reminder-popover-read">{t('markRead')}</DropdownMenu.Item>{/if}
                </DropdownMenu.Content></DropdownMenu.Portal>
              </DropdownMenu.Root>
            </li>
          {:else}
            <li class="px-3 py-6 text-center text-[12.5px] text-muted" role="status">{loading ? t('loading') : scope === 'open' ? t('reminderEmptyActions') : t('reminderEmpty')}</li>
          {/each}
        </ul>
      </div>
      <div class="border-t border-line p-1.5">
        <button class={buttonClass('ghost', 'sm', 'w-full')} onclick={() => { open = false; go('/reminders') }} data-testid="reminders-all">{t('noticeAll')}</button>
      </div>
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>
