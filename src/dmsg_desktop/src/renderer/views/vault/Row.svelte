<script lang="ts">
  import { ContextMenu } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import Icon from '../../components/Icon.svelte'
  import { cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, size } from '../../rpc'
  import { typeIcon, typeLabel, typeOf } from './vault'

  /** One item in the list; right-click selects it and opens its actions. */
  let {
    entry,
    selected,
    conflict,
    trash,
    unlocked,
    onSelect,
    onStar,
    onDiscard,
    onExport
  }: {
    entry: any
    selected: boolean
    conflict: boolean
    trash: boolean
    unlocked: boolean
    onSelect: () => void
    onStar: () => void
    onDiscard: () => void
    onExport: () => void
  } = $props()

  const item = $derived(entry.item)
  const type = $derived(typeOf(item))
  /** A line on what the item is: the user name, the address, the file's size, or the body's first line. */
  const line = $derived(
    item.file ? size(item.file.size) : item.username || item.url.replace(/^https?:\/\//, '') || item.body.split('\n')[0] || t(typeLabel[type])
  )
</script>

<li>
  <ContextMenu.Root dir={localeDir(locale())}>
    <ContextMenu.Trigger onclick={onSelect} oncontextmenu={onSelect}>
      {#snippet child({ props })}
        <button
          {...props}
          class={cn(
            'group grid w-full grid-cols-[auto_1fr_auto] items-center gap-2.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
            selected ? 'bg-pale' : 'hover:bg-wash'
          )}
          data-id={entry.id}
          aria-current={selected ? 'true' : undefined}
          tabindex={selected ? 0 : -1}
        >
          <span class={cn('grid size-8 place-items-center rounded-lg border bg-surface', selected ? 'border-forest/25 text-forest' : 'border-line text-faint')}>
            <Icon name={typeIcon[type]} />
          </span>
          <span class="min-w-0">
            <span class="block truncate text-[13.5px] leading-snug font-medium text-ink">{item.title}</span>
            <span class="block truncate text-[12px] leading-snug text-faint">{line}</span>
          </span>
          <span class="flex flex-col items-end gap-1 text-[11px] text-faint">
            <span class="whitespace-nowrap">{ago(item.updatedAt)}</span>
            <span class="flex items-center gap-1">
              {#if conflict}
                <span class="rounded bg-warn-pale px-1 text-[10.5px] font-medium text-warn">{t('conflict')}</span>
              {:else if !entry.synced}
                <span class="size-1.5 rounded-full bg-warn" title={t('notSynced')}></span>
              {/if}
              {#if item.favorite}<Icon name="starred" class="size-3 text-gem" />{/if}
            </span>
          </span>
        </button>
      {/snippet}
    </ContextMenu.Trigger>
    {#if unlocked}
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
            {#if item.file}
              <ContextMenu.Item class={menuItemClass} onSelect={onExport}>
                <span class="flex items-center gap-2"><Icon name="download" class="text-faint" />{t('exportFile')}</span>
              </ContextMenu.Item>
            {/if}
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
