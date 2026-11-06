<script lang="ts">
  import { Popover } from 'bits-ui'
  import { onMount, untrack } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Segmented from '../components/Segmented.svelte'
  import { buttonClass, cn, iconButtonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { ago, call, errorText, single } from '../rpc'
  import { category, icons, noNames, noticeNames, noticeTitle } from '../views/reminders/notice'
  import type { NoticeItem } from '../../shared/notifications'

  /** The bell in the title bar: what wants attention, without leaving the view. The full list stays a page. */
  let { status, count = 0, go }: { status: any; count?: number; go: (path: string) => void } = $props()

  let open = $state(false)
  let scope = $state('open')
  let items = $state<NoticeItem[]>([])
  let names = $state.raw(noNames())
  let loading = $state(false)
  let error = $state('')
  let run = 0

  const account = $derived<string>(status?.account ?? '')

  const load = single(async () => {
    const turn = ++run
    if (!account) { items = []; return }
    loading = true
    error = ''
    try {
      const [page, known] = await Promise.all([
        call<{ items: NoticeItem[] }>('notifications/list', { account, scope, cursor: null, limit: 20 }),
        noticeNames()
      ])
      if (turn !== run) return
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
    if (open) untrack(() => void load())
  })

  async function follow(item: NoticeItem) {
    open = false
    try {
      await window.dmsg.notificationOpen(item.account, item.id)
    } catch (e) {
      error = errorText(e)
      open = true
    }
  }

  onMount(() => window.dmsg.onEvent((event) => {
    if (open && event.type === 'notifications/changed') void load()
  }))
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
            <li>
              <button class="flex w-full items-start gap-2.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 hover:bg-wash focus-visible:ring-3 focus-visible:ring-focus/35" onclick={() => follow(item)} data-testid="reminder-row">
                <Icon name={icons[category(item)]} class={cn('mt-0.5 shrink-0', item.actionable ? 'text-forest' : 'text-faint')} />
                <span class="min-w-0 flex-1">
                  <span class="block truncate text-[13px] leading-snug font-medium">{noticeTitle(item, names)}</span>
                  <span class="block truncate text-[11.5px] leading-snug text-faint">{ago(item.updated_at)}{#if item.unread} · {item.unread.toLocaleString()} {t('noticeMessages')}{/if}</span>
                </span>
              </button>
            </li>
          {:else}
            <li class="px-3 py-6 text-center text-[12.5px] text-muted" role="status">{loading ? t('loading') : t('noticeEmpty')}</li>
          {/each}
        </ul>
      </div>
      <div class="border-t border-line p-1.5">
        <button class={buttonClass('ghost', 'sm', 'w-full')} onclick={() => { open = false; go('/reminders') }} data-testid="reminders-all">{t('noticeAll')}</button>
      </div>
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>
