<script lang="ts">
  import { ContextMenu } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import Icon from '../../components/Icon.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, size } from '../../rpc'
  import { fileIcon, statusIcon, statusLabel, statusOf, statusTone } from './files'

  /**
   * One row of a folder: a file with its status, or a folder. Click selects,
   * ⌘-click adds to the selection, double-click opens a folder; right-click
   * opens the actions. Rows drag into folders of the tree.
   */
  let {
    entry,
    folder = false,
    selected,
    trash,
    unlocked,
    onSelect,
    onOpenFolder,
    onOpen,
    onExport,
    onBackup,
    onShare,
    onMove,
    onRename,
    onStar,
    onDiscard,
    onPurge,
    dragIds
  }: {
    entry: any
    folder?: boolean
    selected: boolean
    trash: boolean
    unlocked: boolean
    onSelect: (event: MouseEvent) => void
    onOpenFolder: () => void
    onOpen: () => void
    onExport: () => void
    onBackup: () => void
    onShare: () => void
    onMove: () => void
    onRename: () => void
    onStar: () => void
    onDiscard: () => void
    onPurge: () => void
    dragIds: () => string[]
  } = $props()

  const status = $derived(statusOf(entry.status))
  const tones: Record<string, string> = { neutral: 'text-faint', forest: 'text-forest', gem: 'text-gem', warn: 'text-warn', danger: 'text-danger', ok: 'text-ok' }

  function onDragStart(event: DragEvent) {
    if (!unlocked || trash) return event.preventDefault()
    event.dataTransfer?.setData('application/x-dmsg-files', JSON.stringify(dragIds()))
    event.dataTransfer!.effectAllowed = 'move'
  }
</script>

<li>
  <ContextMenu.Root dir={localeDir(locale())}>
    <ContextMenu.Trigger onclick={onSelect} oncontextmenu={onSelect} ondblclick={folder ? onOpenFolder : onOpen}>
      {#snippet child({ props })}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <button
          {...props}
          class={cn(
            'group grid w-full grid-cols-[auto_1fr_auto] items-center gap-2.5 rounded-lg px-2 py-1.5 text-start outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
            selected ? 'bg-pale' : 'hover:bg-wash'
          )}
          draggable={unlocked && !trash}
          ondragstart={onDragStart}
          data-id={entry.id}
          data-kind={folder ? 'folder' : 'file'}
          aria-current={selected ? 'true' : undefined}
          tabindex={selected ? 0 : -1}
        >
          <span class={cn('grid size-8 place-items-center rounded-lg border bg-surface', selected ? 'border-forest/25 text-forest' : 'border-line text-faint')}>
            <Icon name={folder ? 'folder' : fileIcon(entry.mime)} />
          </span>
          <span class="min-w-0">
            <span class="block truncate text-[13.5px] leading-snug font-medium text-ink">{entry.name}</span>
            <span class="block truncate text-[12px] leading-snug text-faint">
              {#if folder}
                {ago(entry.updated_at)}
              {:else}
                {size(entry.size)}{entry.versions > 1 ? ` · ${t('versionCount', { n: entry.versions })}` : ''} · {ago(entry.updated_at)}
              {/if}
            </span>
          </span>
          <span class="flex items-center gap-1.5 text-faint">
            {#if entry.starred}<Icon name="starred" class="size-3 text-gem" />{/if}
            {#if !folder}
              <Tooltip content={t(statusLabel[status])}>
                {#snippet children(props)}
                  <span {...props} class={cn('grid size-6 place-items-center', tones[statusTone[status]])} data-status={status}>
                    <Icon name={statusIcon[status]} class={cn('size-4', status === 'backing_up' && 'animate-pulse')} />
                  </span>
                {/snippet}
              </Tooltip>
            {/if}
          </span>
        </button>
      {/snippet}
    </ContextMenu.Trigger>
    {#if unlocked}
      <ContextMenu.Portal>
        <ContextMenu.Content class={cn(menuContentClass, 'min-w-52')} data-testid="files-row-menu">
          {#if trash}
            <ContextMenu.Item class={menuItemClass} onSelect={onDiscard}>
              <span class="flex items-center gap-2"><Icon name="restore" class="text-faint" />{t('restore')}</span>
            </ContextMenu.Item>
            <ContextMenu.Separator class="my-1 h-px bg-line" />
            <ContextMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={onPurge}>
              <span class="flex items-center gap-2"><Icon name="trash" />{t('deleteForever')}</span>
            </ContextMenu.Item>
          {:else}
            {#if folder}
              <ContextMenu.Item class={menuItemClass} onSelect={onOpenFolder}>
                <span class="flex items-center gap-2"><Icon name="folder-open" class="text-faint" />{t('open')}</span>
              </ContextMenu.Item>
            {:else}
              <ContextMenu.Item class={menuItemClass} onSelect={onOpen} disabled={status === 'cloud_only'}>
                <span class="flex items-center gap-2"><Icon name="external" class="text-faint" />{t('openFile')}</span>
              </ContextMenu.Item>
              <ContextMenu.Item class={menuItemClass} onSelect={onExport} disabled={status === 'cloud_only'}>
                <span class="flex items-center gap-2"><Icon name="download" class="text-faint" />{t('exportFile')}</span>
              </ContextMenu.Item>
              <ContextMenu.Item class={menuItemClass} onSelect={onShare} disabled={status === 'cloud_only' || status === 'too_large'}>
                <span class="flex items-center gap-2"><Icon name="send" class="text-faint rtl:-scale-x-100" />{t('shareToChannel')}</span>
              </ContextMenu.Item>
              <ContextMenu.Item class={menuItemClass} onSelect={onStar}>
                <span class="flex items-center gap-2"><Icon name={entry.starred ? 'starred' : 'star'} class="text-faint" />{entry.starred ? t('unstar') : t('star')}</span>
              </ContextMenu.Item>
            {/if}
            <ContextMenu.Item class={menuItemClass} onSelect={onBackup} disabled={!folder && (status === 'backed_up' || status === 'backing_up' || status === 'cloud_only' || status === 'too_large')}>
              <span class="flex items-center gap-2"><Icon name="cloud-up" class="text-faint" />{t('backUp')}</span>
            </ContextMenu.Item>
            <ContextMenu.Separator class="my-1 h-px bg-line" />
            <ContextMenu.Item class={menuItemClass} onSelect={onRename}>
              <span class="flex items-center gap-2"><Icon name="edit" class="text-faint" />{t('renameFile')}</span>
            </ContextMenu.Item>
            <ContextMenu.Item class={menuItemClass} onSelect={onMove}>
              <span class="flex items-center gap-2"><Icon name="folder" class="text-faint" />{t('moveTo')}</span>
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
