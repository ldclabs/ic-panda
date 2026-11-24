<script lang="ts">
  import { visibleName } from '../channels/names.svelte'
  import Icon from '../../components/Icon.svelte'
  import { cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { ago, when } from '../../rpc'
  import type { NoticeItem } from '../../../shared/notifications'
  import { category, icons, noticeBody, noticeTitle, noticeUnread, type NoticeNames } from './notice'

  let { item, names, compact = false }: { item: NoticeItem; names: NoticeNames; compact?: boolean } = $props()
</script>

<Icon name={item.target.kind === 'service' ? 'bell' : icons[category(item)]} class="mt-0.5 shrink-0 text-muted" />
<span class="grid min-w-0 flex-1 gap-1">
  <span class={cn('flex items-start gap-2 font-medium text-ink [overflow-wrap:anywhere]', compact ? 'text-body' : 'text-body')}>
    <span {@attach visibleName(item.target.kind === 'inbox' && item.target.id ? names.letters.get(item.target.id) : undefined)} class="min-w-0 flex-1">{noticeTitle(item, names)}</span>
    {#if noticeUnread(item)}<span class="mt-1.5 size-1.5 shrink-0 rounded-full bg-forest" role="img" aria-label={t('reminderUnread')} data-testid="reminder-unread"></span>{/if}
  </span>
  <span class="text-callout leading-normal text-muted [overflow-wrap:anywhere]">{noticeBody(item)}</span>
  <span class="flex flex-wrap items-center gap-x-2 gap-y-1 text-callout text-muted">
    <time datetime={new Date(item.updated_at).toISOString()} title={when(item.updated_at)}>{ago(item.updated_at)}</time>
    {#if item.open && item.actionable}<span>{t('noticeNeedsAction')}</span>{/if}
    {#if !noticeUnread(item)}<span class="sr-only" data-testid="reminder-read-state">{t('noticeRead')}</span>{/if}
  </span>
  {#if item.open && item.deadline}<span class="text-callout text-muted">{t('validUntil')} {when(item.deadline)}</span>{/if}
  {#if item.snoozed_until}<span class="text-callout text-muted">{t('noticeLater')} · {when(item.snoozed_until)}</span>{/if}
</span>
