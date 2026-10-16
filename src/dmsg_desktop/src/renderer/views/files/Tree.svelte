<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { cn } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { childrenOf, ROOT } from './files'

  /**
   * The library's places: the root and its folders as a tree, the starred
   * files, the cloud backups and the trash. A folder takes files dragged
   * onto it.
   */
  let {
    folders,
    scope,
    unlocked,
    onPick,
    onDropMove,
    onNewFolder
  }: {
    folders: any[]
    scope: string
    unlocked: boolean
    onPick: (scope: string) => void
    onDropMove: (ids: string[], folder: string) => void
    onNewFolder: (parent: string) => void
  } = $props()

  let collapsed = $state<Record<string, boolean>>({})
  let over = $state<string | null>(null)

  const ids = (event: DragEvent): string[] => {
    try {
      const text = event.dataTransfer?.getData('application/x-dmsg-files')
      return text ? (JSON.parse(text) as string[]) : []
    } catch {
      return []
    }
  }
  const carries = (event: DragEvent) => !!event.dataTransfer?.types.includes('application/x-dmsg-files')

  function onDragOver(event: DragEvent, folder: string) {
    if (!carries(event) || !unlocked) return
    event.preventDefault()
    event.dataTransfer!.dropEffect = 'move'
    over = folder
  }

  function onDrop(event: DragEvent, folder: string) {
    if (!carries(event)) return
    event.preventDefault()
    over = null
    const moved = ids(event).filter((id) => id !== folder)
    if (moved.length && unlocked) onDropMove(moved, folder)
  }
</script>

{#snippet entry(value: string, icon: string, label: string, depth: number, hasChildren: boolean, folder: boolean)}
  {@const active = scope === value}
  <div class="group/entry flex items-center" style:padding-inline-start={`${depth * 14}px`}>
    {#if hasChildren}
      <button
        class="grid size-5 shrink-0 place-items-center rounded text-faint hover:text-ink"
        aria-label={collapsed[value] ? t('expandSidebar') : t('collapseSidebar')}
        aria-expanded={!collapsed[value]}
        onclick={() => (collapsed[value] = !collapsed[value])}
      >
        <Icon name="chevron-down" class={cn('size-3.5 transition-transform duration-150', collapsed[value] && 'ltr:-rotate-90 rtl:rotate-90')} />
      </button>
    {:else}
      <span class="size-5 shrink-0"></span>
    {/if}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <button
      class={cn(
        'flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg px-2 text-start text-[13px] outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
        active ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash hover:text-ink',
        over === value && 'ring-2 ring-forest/60'
      )}
      aria-current={active ? 'true' : undefined}
      onclick={() => onPick(value)}
      ondragover={folder ? (event) => onDragOver(event, value) : undefined}
      ondragleave={folder ? () => (over = over === value ? null : over) : undefined}
      ondrop={folder ? (event) => onDrop(event, value) : undefined}
      data-testid={`files-scope-${value === ROOT ? 'root' : value.length === 64 ? 'folder' : value}`}
      data-folder={folder ? value : undefined}
    >
      <Icon name={icon as any} class={cn('shrink-0', active ? 'text-forest' : 'text-faint')} />
      <span class="min-w-0 flex-1 truncate">{label}</span>
    </button>
  </div>
{/snippet}

{#snippet subtree(parent: string, depth: number)}
  {#each childrenOf(folders, parent, locale()) as folder (folder.id)}
    {@const kids = childrenOf(folders, folder.id, locale())}
    {@render entry(folder.id, scope === folder.id ? 'folder-open' : 'folder', folder.name, depth, kids.length > 0, true)}
    {#if kids.length && !collapsed[folder.id]}
      {@render subtree(folder.id, depth + 1)}
    {/if}
  {/each}
{/snippet}

<nav class="flex min-h-0 flex-1 flex-col gap-px overflow-x-hidden overflow-y-auto p-2.5" aria-label={t('folders')}>
  {@render entry(ROOT, 'drive', t('allFiles'), 0, false, true)}
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
    {@render entry('starred', 'star', t('starred'), 0, false, false)}
  </div>
  <div class="mt-auto grid gap-px pt-3">
    {@render entry('cloud', 'cloud-down', t('cloudBackups'), 0, false, false)}
    {@render entry('trash', 'trash', t('trash'), 0, false, false)}
  </div>
</nav>
