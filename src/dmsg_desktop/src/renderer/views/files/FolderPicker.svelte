<script lang="ts">
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { code, errorText, planAction } from '../../rpc'
  import { foldersByParent, folderSubtree, ROOT } from './files'

  /** A folder of the library, chosen in a dialog: for moving, or for where a backup is fetched to. */
  let {
    open = $bindable(false),
    folders,
    title,
    description,
    action,
    exclude = [],
    unchanged,
    onPick
  }: {
    open?: boolean
    folders: any[]
    title: string
    description?: string
    action: string
    /** Folders that cannot be chosen, nor anything under them: those being moved. */
    exclude?: string[]
    unchanged?: string
    onPick: (folder: string) => unknown
  } = $props()

  let picked = $state(ROOT)
  let submitting = $state(false)
  let failure = $state.raw<unknown>(null)
  const error = $derived(failure ? code(failure) === 'IDEMPOTENCY_CONFLICT' ? t('nameTaken') : errorText(failure) : '')
  const actionAfterFailure = $derived(planAction(failure).action)

  $effect(() => {
    if (open) { picked = ROOT; failure = null }
  })

  const children = $derived(foldersByParent(folders, locale()))
  const excluded = $derived(folderSubtree(children, exclude))
  const canPick = $derived(!submitting && picked !== unchanged && !excluded.has(picked) && (picked === ROOT || folders.some(folder => folder.id === picked)))

  function selectFolder(id: string) {
    picked = id
    failure = null
  }

  async function confirm() {
    if (!canPick) return
    submitting = true
    failure = null
    try {
      await onPick(picked)
      open = false
    } catch (cause) {
      failure = cause
    } finally {
      submitting = false
    }
  }
</script>

{#snippet subtree(parent: string, depth: number)}
  {#each children.get(parent) ?? [] as folder (folder.id)}
    {@const off = excluded.has(folder.id)}
    <button
      class={cn(
        'flex h-8 w-full items-center gap-2 rounded-lg px-2 text-start text-[13px] outline-none focus-visible:ring-3 focus-visible:ring-focus/35',
        picked === folder.id ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash',
        off && 'cursor-not-allowed opacity-40 hover:bg-transparent'
      )}
      style:padding-inline-start={`${8 + depth * 16}px`}
      disabled={off || submitting}
      aria-pressed={picked === folder.id}
      onclick={() => selectFolder(folder.id)}
      data-folder={folder.id}
    >
      <Icon name="folder" class={cn('shrink-0', picked === folder.id ? 'text-forest' : 'text-faint')} />
      <span class="min-w-0 flex-1 truncate">{folder.name}</span>
    </button>
    {@render subtree(folder.id, depth + 1)}
  {/each}
{/snippet}

<Dialog bind:open {title} {description} dismissible={!submitting} icon="folder" testid="files-folder-dialog">
  <div class="grid gap-px rounded-xl border border-line bg-surface p-1.5" role="group" aria-label={t('folders')}>
    <button
      class={cn('flex h-8 w-full items-center gap-2 rounded-lg px-2 text-start text-[13px] outline-none focus-visible:ring-3 focus-visible:ring-focus/35', picked === ROOT ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash')}
      aria-pressed={picked === ROOT}
      onclick={() => selectFolder(ROOT)}
      disabled={submitting}
      data-folder={ROOT}
    >
      <Icon name="drive" class={cn('shrink-0', picked === ROOT ? 'text-forest' : 'text-faint')} />
      <span>{t('library')}</span>
    </button>
    {@render subtree(ROOT, 1)}
  </div>
  {#if error}
    <p role="alert" class="mt-2 text-sm text-danger">{error}</p>
    {#if actionAfterFailure}<button class={buttonClass('secondary', 'xs', 'mt-2')} onclick={actionAfterFailure.run}>{actionAfterFailure.label}</button>{/if}
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} disabled={submitting} onclick={() => (open = false)}>{t('cancel')}</button>
    <button
      class={buttonClass('primary')}
      disabled={!canPick}
      onclick={confirm}
      data-testid="files-folder-pick"
    >
      {submitting ? t('loading') : action}
    </button>
  {/snippet}
</Dialog>
