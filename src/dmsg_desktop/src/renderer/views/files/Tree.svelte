<script lang="ts">
  import { ContextMenu } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import Icon from '../../components/Icon.svelte'
  import { cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { carries, childrenOf, DRAG_TYPE, draggedIds, ROOT } from './files'

  /**
   * The library's places: the root and its folders as a tree, the starred
   * files, the tags, the cloud backups and the trash. A folder takes rows
   * dragged onto it and files dropped from the system, and right-click
   * opens its actions.
   */
  let {
    folders,
    tags,
    scope,
    unlocked,
    onPick,
    onDropMove,
    onDropFiles,
    onNewFolder,
    onFolderAction
  }: {
    folders: any[]
    tags: { tag: string; files: number }[]
    scope: string
    unlocked: boolean
    onPick: (scope: string) => void
    onDropMove: (ids: string[], folder: string) => void
    onDropFiles: (files: File[], folder: string) => void
    onNewFolder: (parent: string) => void
    onFolderAction: (action: 'rename' | 'backup' | 'grant' | 'move' | 'trash', folder: any) => void
  } = $props()

  let collapsed = $state<Record<string, boolean>>({})
  let over = $state<string | null>(null)

  function onDragOver(event: DragEvent, folder: string) {
    if (!carries(event) || !unlocked) return
    event.preventDefault()
    event.stopPropagation()
    event.dataTransfer!.dropEffect = event.dataTransfer!.types.includes('Files') ? 'copy' : 'move'
    over = folder
  }

  /** Library rows move into the folder; system files are added to it. */
  function onDrop(event: DragEvent, folder: string) {
    if (!carries(event)) return
    event.preventDefault()
    event.stopPropagation()
    over = null
    if (!unlocked) return
    if (event.dataTransfer?.types.includes(DRAG_TYPE)) {
      const moved = draggedIds(event).filter((id) => id !== folder)
      if (moved.length) onDropMove(moved, folder)
    } else {
      const files = Array.from(event.dataTransfer?.files ?? [])
      if (files.length) onDropFiles(files, folder)
    }
  }
</script>

{#snippet place(value: string, icon: string, label: string, testid: string, count?: number)}
  {@const active = scope === value}
  <button
    class={cn(
      'flex h-8 w-full min-w-0 items-center gap-2 rounded-lg px-2 text-start text-[13px] outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
      active ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash hover:text-ink'
    )}
    aria-current={active ? 'true' : undefined}
    onclick={() => onPick(value)}
    data-testid={testid}
  >
    <Icon name={icon as any} class={cn('shrink-0', active ? 'text-forest' : 'text-faint')} />
    <span class="min-w-0 flex-1 truncate">{label}</span>
    {#if count != null}<span class="text-[11px] text-faint tabular-nums">{count}</span>{/if}
  </button>
{/snippet}

{#snippet target(folder: any, props: Record<string, unknown>)}
  {@const value = folder?.id ?? ROOT}
  {@const active = scope === value}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <button
    {...props}
    class={cn(
      'flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg px-2 text-start text-[13px] outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
      active ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash hover:text-ink',
      over === value && 'ring-2 ring-forest/60'
    )}
    aria-current={active ? 'true' : undefined}
    onclick={() => onPick(value)}
    ondragover={(event) => onDragOver(event, value)}
    ondragleave={() => (over = over === value ? null : over)}
    ondrop={(event) => onDrop(event, value)}
    data-testid={`files-scope-${folder ? 'folder' : 'root'}`}
    data-folder={value}
  >
    <Icon name={folder ? (active ? 'folder-open' : 'folder') : 'drive'} class={cn('shrink-0', active ? 'text-forest' : 'text-faint')} />
    <span class="min-w-0 flex-1 truncate">{folder ? folder.name : t('allFiles')}</span>
  </button>
{/snippet}

{#snippet entry(folder: any, depth: number, hasChildren: boolean)}
  {@const value = folder?.id ?? ROOT}
  <div class="group/entry flex items-center" style:padding-inline-start={`${depth * 14}px`}>
    {#if hasChildren}
      <button
        class="grid size-5 shrink-0 place-items-center rounded text-faint hover:text-ink"
        aria-label={collapsed[value] ? t('expandFolder') : t('collapseFolder')}
        aria-expanded={!collapsed[value]}
        onclick={() => (collapsed[value] = !collapsed[value])}
      >
        <Icon name="chevron-down" class={cn('size-3.5 transition-transform duration-150', collapsed[value] && 'ltr:-rotate-90 rtl:rotate-90')} />
      </button>
    {:else}
      <span class="size-5 shrink-0"></span>
    {/if}
    {#if folder && unlocked}
      <ContextMenu.Root dir={localeDir(locale())}>
        <ContextMenu.Trigger>
          {#snippet child({ props })}
            {@render target(folder, props)}
          {/snippet}
        </ContextMenu.Trigger>
        <ContextMenu.Portal>
          <ContextMenu.Content class={cn(menuContentClass, 'min-w-52')} data-testid="files-tree-menu">
            <ContextMenu.Item class={menuItemClass} onSelect={() => onNewFolder(folder.id)}>
              <span class="flex items-center gap-2"><Icon name="folder-add" class="text-faint" />{t('newSubfolder')}</span>
            </ContextMenu.Item>
            <ContextMenu.Item class={menuItemClass} onSelect={() => onFolderAction('backup', folder)}>
              <span class="flex items-center gap-2"><Icon name="cloud-up" class="text-faint" />{t('backUp')}</span>
            </ContextMenu.Item>
            <ContextMenu.Item class={menuItemClass} onSelect={() => onFolderAction('grant', folder)}>
              <span class="flex items-center gap-2"><Icon name="agents" class="text-faint" />{t('grantAgent')}</span>
            </ContextMenu.Item>
            <ContextMenu.Separator class="my-1 h-px bg-line" />
            <ContextMenu.Item class={menuItemClass} onSelect={() => onFolderAction('rename', folder)}>
              <span class="flex items-center gap-2"><Icon name="edit" class="text-faint" />{t('renameFile')}</span>
            </ContextMenu.Item>
            <ContextMenu.Item class={menuItemClass} onSelect={() => onFolderAction('move', folder)}>
              <span class="flex items-center gap-2"><Icon name="folder" class="text-faint" />{t('moveTo')}</span>
            </ContextMenu.Item>
            <ContextMenu.Separator class="my-1 h-px bg-line" />
            <ContextMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => onFolderAction('trash', folder)}>
              <span class="flex items-center gap-2"><Icon name="trash" />{t('moveToTrash')}</span>
            </ContextMenu.Item>
          </ContextMenu.Content>
        </ContextMenu.Portal>
      </ContextMenu.Root>
    {:else}
      {@render target(folder, {})}
    {/if}
  </div>
{/snippet}

{#snippet subtree(parent: string, depth: number)}
  {#each childrenOf(folders, parent, locale()) as folder (folder.id)}
    {@const kids = childrenOf(folders, folder.id, locale())}
    {@render entry(folder, depth, kids.length > 0)}
    {#if kids.length && !collapsed[folder.id]}
      {@render subtree(folder.id, depth + 1)}
    {/if}
  {/each}
{/snippet}

<nav class="flex min-h-0 flex-1 flex-col gap-px overflow-x-hidden overflow-y-auto p-2.5" aria-label={t('folders')}>
  {@render entry(null, 0, false)}
  <div class="flex items-center justify-between px-2 pt-3 pb-1">
    <span class="text-[11px] font-medium text-faint">{t('folders')}</span>
    {#if unlocked}
      <button class="grid size-5 place-items-center rounded text-faint hover:text-ink" aria-label={t('newFolder')} onclick={() => onNewFolder(scope.length === 64 ? scope : ROOT)} data-testid="files-new-folder">
        <Icon name="folder-add" class="size-3.5" />
      </button>
    {/if}
  </div>
  {@render subtree(ROOT, 0)}
  <div class="pt-3">
    {@render place('starred', 'star', t('starred'), 'files-scope-starred')}
  </div>
  {#if tags.length}
    <span class="px-2 pt-3 pb-1 text-[11px] font-medium text-faint">{t('tags')}</span>
    {#each tags as item (item.tag)}
      {@render place(`tag:${item.tag}`, 'tag', item.tag, 'files-scope-tag', item.files)}
    {/each}
  {/if}
  <div class="mt-auto grid gap-px pt-3">
    {@render place('storage', 'drive', t('storageTitle'), 'files-scope-storage')}
    {@render place('links', 'link', t('shareLinks'), 'files-scope-links')}
    {@render place('cloud', 'cloud-down', t('cloudBackups'), 'files-scope-cloud')}
    {@render place('trash', 'trash', t('trash'), 'files-scope-trash')}
  </div>
</nav>
