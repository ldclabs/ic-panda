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
  let input = $state<HTMLInputElement | null>(null)
  let limit = $state(100)
  const search = searchLibrary(() => open ? query.trim() : null, 120)
  const found = $derived(search.result.kind === 'ready' ? search.result.files.filter(f => !taken.includes(f.id)) : [])

  $effect(() => {
    void query
    limit = 100
    if (!open) query = ''
  })

  function pick(id: string) {
    onPick(id)
    open = false
  }
</script>

<Dialog bind:open title={t('attachFile')} description={t('attachHint')} icon="attach" testid="file-picker" onOpenAutoFocus={(event) => { if (input) { event.preventDefault(); input.focus() } }}>
  <div class="grid gap-2">
    <label class="relative">
      <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
      <input bind:this={input} bind:value={query} type="search" class="ps-8" placeholder={t('searchFiles')} aria-label={t('searchFiles')} data-testid="file-picker-search" />
    </label>
    <ul class="grid max-h-80 gap-0.5 overflow-y-auto">
      {#each found.slice(0, limit) as file (file.id)}
        <li>
          <button class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-start text-[13px] hover:bg-wash" onclick={() => pick(file.id)} data-testid="file-picker-item">
            <Icon name="file" class="shrink-0 text-faint" />
            <span class="min-w-0 flex-1"><span class="block truncate">{file.name}</span>
              <bdi class="block truncate text-[12px] text-muted">{file.path?.join(' / ') || t('library')}</bdi></span>
            <span class="shrink-0 text-[11.5px] text-faint">{size(file.size)}</span>
          </button>
        </li>
      {:else}
        <li class="px-2 py-6"><SearchStatus {search} empty={t('noFilesFound')} /></li>
      {/each}
      {#if found.length > limit}
        <li><button class={buttonClass('ghost', 'sm')} onclick={() => limit += 100}>{t('moreRows', { n: found.length - limit })}</button></li>
      {/if}
    </ul>
  </div>
  {#snippet footer()}
    <span class="min-w-0 flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
  {/snippet}
</Dialog>
