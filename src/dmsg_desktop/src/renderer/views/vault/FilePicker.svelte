<script lang="ts">
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { size } from '../../rpc'
  import SearchStatus from '../files/SearchStatus.svelte'
  import { searchLibrary } from '../files/search.svelte'

  /**
   * Pick a file of the library to attach to an item: the item names it,
   * the file stays in the library with its own versions and backups.
   */
  let { open = $bindable(false), taken, onPick }: { open?: boolean; taken: string[]; onPick: (id: string) => void } = $props()

  let query = $state('')
  const search = searchLibrary(() => open ? query.trim() : null, 120)
  const found = $derived(search.result.kind === 'ready' ? search.result.files.slice(0, 100) : [])

  $effect(() => {
    if (!open) query = ''
  })

  function pick(id: string) {
    onPick(id)
    open = false
  }
</script>

<Dialog bind:open title={t('attachFile')} description={t('attachHint')} icon="attach" testid="file-picker">
  <div class="grid gap-2">
    <label class="relative">
      <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
      <input bind:value={query} type="search" class="ps-8" placeholder={t('searchFiles')} aria-label={t('searchFiles')} data-testid="file-picker-search" />
    </label>
    <ul class="grid max-h-80 gap-0.5 overflow-y-auto">
      {#each found.filter((f) => !taken.includes(f.id)) as file (file.id)}
        <li>
          <button class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-start text-[13px] hover:bg-wash" onclick={() => pick(file.id)} data-testid="file-picker-item">
            <Icon name="file" class="shrink-0 text-faint" />
            <span class="min-w-0 flex-1 truncate">{file.name}</span>
            <span class="shrink-0 text-[11.5px] text-faint">{size(file.size)}</span>
          </button>
        </li>
      {:else}
        <li class="px-2 py-6"><SearchStatus {search} empty={t('noFilesFound')} /></li>
      {/each}
    </ul>
  </div>
  {#snippet footer()}
    <span class="min-w-0 flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
  {/snippet}
</Dialog>
