<script lang="ts">
  import { ContextMenu } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import Icon from '../../components/Icon.svelte'
  import { cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago } from '../../rpc'
  import { isManaged, line, typeIcon, typeOf } from './vault'

  /**
   * One item in the list; right-click selects it and opens its actions. In
   * select mode a click ticks it instead, for the actions on many.
   */
  let {
    entry,
    selected,
    conflict,
    flagged,
    trash,
    unlocked,
    choosing,
    chosen,
    onSelect,
    onChoose,
    onStar,
    onDuplicate,
    onDiscard
  }: {
    entry: any
    selected: boolean
    conflict: boolean
    flagged: boolean
    trash: boolean
    unlocked: boolean
    choosing: boolean
    chosen: boolean
    onSelect: () => void
    onChoose: () => void
    onStar: () => void
    onDuplicate: () => void
    onDiscard: () => void
  } = $props()

  const item = $derived(entry.item)
  const type = $derived(typeOf(item))
  const managed = $derived(isManaged(item))
  const subtitle = $derived(line(item, t))
</script>

<li>
  <ContextMenu.Root dir={localeDir(locale())}>
    <ContextMenu.Trigger onclick={choosing && !managed ? onChoose : onSelect} oncontextmenu={onSelect}>
      {#snippet child({ props })}
        <button
          {...props}
          class={cn(
            'group grid w-full grid-cols-[auto_1fr_auto] items-center gap-2.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
            selected || chosen ? 'bg-pale' : 'hover:bg-wash'
          )}
          data-id={entry.id}
          data-chosen={chosen}
          aria-current={selected ? 'true' : undefined}
          tabindex={selected ? 0 : -1}
        >
          {#if choosing}
            <span
              class={cn(
                'grid size-8 place-items-center rounded-lg border',
                managed ? 'border-line text-faint opacity-40' : chosen ? 'border-forest bg-forest text-on-forest' : 'border-edge bg-surface text-transparent'
              )}
              data-testid="vault-row-check"
            >
              <Icon name="check" />
            </span>
          {:else}
            <span class={cn('grid size-8 place-items-center [&_svg]:size-5', selected ? 'text-forest' : 'text-faint')}>
              <Icon name={typeIcon[type]} />
            </span>
          {/if}
          <span class="min-w-0">
            <span class="block truncate text-[13.5px] leading-snug font-medium text-ink">{item.title}</span>
            <span class="block truncate text-[12px] leading-snug text-faint">{subtitle}</span>
          </span>
          <span class="flex flex-col items-end gap-1 text-[11px] text-faint">
            <span class="whitespace-nowrap">{ago(trash ? entry.at : item.updatedAt)}</span>
            <span class="flex items-center gap-1">
              {#if conflict}
                <span class="rounded bg-warn-pale px-1 text-[10.5px] font-medium text-warn">{t('conflict')}</span>
              {:else if !entry.synced}
                <span class="size-1.5 rounded-full bg-warn" title={t('notSynced')}></span>
              {/if}
              {#if flagged}<Icon name="warning" class="size-3 text-warn" label={t('healthFlagged')} />{/if}
              {#if managed}<Icon name="lock" class="size-3" label={t('managedByDmsg')} />{/if}
              {#if item.favorite}<Icon name="starred" class="size-3 text-gem" />{/if}
            </span>
          </span>
        </button>
      {/snippet}
    </ContextMenu.Trigger>
    {#if unlocked && !managed}
      <ContextMenu.Portal>
        <ContextMenu.Content class={cn(menuContentClass, 'min-w-48')} data-testid="vault-row-menu">
          {#if trash}
            <ContextMenu.Item class={menuItemClass} onSelect={onDiscard}>
              <span class="flex items-center gap-2"><Icon name="restore" class="text-faint" />{t('restore')}</span>
            </ContextMenu.Item>
          {:else}
            <ContextMenu.Item class={menuItemClass} onSelect={onStar}>
              <span class="flex items-center gap-2"><Icon name={item.favorite ? 'starred' : 'star'} class="text-faint" />{item.favorite ? t('unstar') : t('star')}</span>
            </ContextMenu.Item>
            <ContextMenu.Item class={menuItemClass} onSelect={onDuplicate}>
              <span class="flex items-center gap-2"><Icon name="duplicate" class="text-faint" />{t('duplicate')}</span>
            </ContextMenu.Item>
            <ContextMenu.Separator class="my-1 h-px bg-line" />
            <ContextMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={onDiscard}>
              <span class="flex items-center gap-2"><Icon name="trash" />{t('moveToTrash')}</span>
            </ContextMenu.Item>
          {/if}
        </ContextMenu.Content>
      </ContextMenu.Portal>
    {/if}
  </ContextMenu.Root>
</li>
