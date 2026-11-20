<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { ago } from '../../rpc'
  import { isManaged, line, typeIcon, typeOf } from './vault'

  /** One item in the list. The list owns the clicks' meaning and the one right-click menu. */
  let {
    entry,
    selected,
    focusable,
    multi,
    conflict,
    flagged,
    trash,
    chosen,
    onClick
  }: {
    entry: any
    selected: boolean
    focusable: boolean
    multi: boolean
    conflict: boolean
    flagged: boolean
    trash: boolean
    chosen: boolean
    onClick: (event: MouseEvent) => void
  } = $props()

  const item = $derived(entry.item)
  const type = $derived(typeOf(item))
  const managed = $derived(isManaged(item))
  const subtitle = $derived(line(item, t))
</script>

<li>
  <button
    class={cn(
      'group grid w-full grid-cols-[auto_1fr_auto] items-center gap-2.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
      selected || chosen ? 'bg-pale' : 'hover:bg-wash'
    )}
    data-id={entry.id}
    data-chosen={chosen}
    aria-current={selected ? 'true' : undefined}
    aria-pressed={multi && !managed ? chosen : undefined}
    tabindex={focusable ? 0 : -1}
    onclick={onClick}
  >
    <span class={cn('grid size-8 place-items-center [&_svg]:size-5', selected || chosen ? 'text-forest' : 'text-faint')}>
      <Icon name={typeIcon[type]} />
    </span>
    <span class="min-w-0">
      <span class="block truncate text-[13.5px] leading-snug font-medium text-ink">{item.title}</span>
      <span class="block truncate text-[12px] leading-snug text-faint">{subtitle}</span>
    </span>
    <span class="flex flex-col items-end gap-1 text-[11px] text-faint">
      <span class="whitespace-nowrap">{ago(trash ? entry.at : item.updatedAt)}</span>
      <span class="flex items-center gap-1">
        {#if conflict}
          <span class="rounded bg-warn-pale px-1 text-[10.5px] font-medium text-warn">{t('conflict')}</span>
        {:else if !entry.synced && entry.save_state !== 'local_only'}
          <span class="size-1.5 rounded-full bg-warn" title={t('notSynced')}><span class="sr-only">{t('notSynced')}</span></span>
        {/if}
        {#if flagged}<Icon name="warning" class="size-3 text-warn" label={t('healthFlagged')} />{/if}
        {#if managed}<Icon name="lock" class="size-3" label={t('managedByDmsg')} />{/if}
        {#if item.favorite}<Icon name="starred" class="size-3 text-gem" label={t('starred')} />{/if}
      </span>
    </span>
  </button>
</li>
