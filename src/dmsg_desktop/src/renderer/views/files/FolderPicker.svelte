<script lang="ts">
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { childrenOf, ROOT, within } from './files'

  /** A folder of the library, chosen in a dialog: for moving, or for where a backup is fetched to. */
  let {
    open = $bindable(false),
    folders,
    title,
    description,
    action,
    exclude = [],
    onPick
  }: {
    open?: boolean
    folders: any[]
    title: string
    description?: string
    action: string
    /** Folders that cannot be chosen, nor anything under them: those being moved. */
    exclude?: string[]
    onPick: (folder: string) => void
  } = $props()

  let picked = $state(ROOT)

  $effect(() => {
    if (open) picked = ROOT
  })

  const blocked = (id: string) => exclude.some((moved) => within(folders, id, moved))
</script>

{#snippet subtree(parent: string, depth: number)}
  {#each childrenOf(folders, parent, locale()) as folder (folder.id)}
    {@const off = blocked(folder.id)}
    <button
      class={cn(
        'flex h-8 w-full items-center gap-2 rounded-lg px-2 text-start text-[13px] outline-none focus-visible:ring-3 focus-visible:ring-focus/35',
        picked === folder.id ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash',
        off && 'cursor-not-allowed opacity-40 hover:bg-transparent'
      )}
      style:padding-inline-start={`${8 + depth * 16}px`}
      disabled={off}
      aria-pressed={picked === folder.id}
      onclick={() => (picked = folder.id)}
      data-folder={folder.id}
    >
      <Icon name="folder" class={cn('shrink-0', picked === folder.id ? 'text-forest' : 'text-faint')} />
      <span class="min-w-0 flex-1 truncate">{folder.name}</span>
    </button>
    {@render subtree(folder.id, depth + 1)}
  {/each}
{/snippet}

<Dialog bind:open {title} {description} icon="folder" testid="files-folder-dialog">
  <div class="grid gap-px rounded-xl border border-line bg-surface p-1.5" role="listbox" aria-label={t('folders')}>
    <button
      class={cn('flex h-8 w-full items-center gap-2 rounded-lg px-2 text-start text-[13px] outline-none focus-visible:ring-3 focus-visible:ring-focus/35', picked === ROOT ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash')}
      aria-pressed={picked === ROOT}
      onclick={() => (picked = ROOT)}
      data-folder={ROOT}
    >
      <Icon name="drive" class={cn('shrink-0', picked === ROOT ? 'text-forest' : 'text-faint')} />
      <span>{t('library')}</span>
    </button>
    {@render subtree(ROOT, 1)}
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    <button
      class={buttonClass('primary')}
      onclick={() => {
        // Before closing: closing lets go of what the dialog acts on.
        onPick(picked)
        open = false
      }}
      data-testid="files-folder-pick"
    >
      {action}
    </button>
  {/snippet}
</Dialog>
