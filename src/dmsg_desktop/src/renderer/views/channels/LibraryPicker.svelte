<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity'
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { size } from '../../rpc'
  import SearchStatus from '../files/SearchStatus.svelte'
  import { searchLibrary } from '../files/search.svelte'

  /** Files of the library to send, found by name; several at once, each in a message of its own. */
  type Attachment = { id: string; name: string }
  let { open = $bindable(false), onPick }: { open?: boolean; onPick: (files: Attachment[]) => void } = $props()

  let query = $state('')
  const search = searchLibrary(() => open ? query : null)
  const found = $derived(search.result.kind === 'ready'
    ? search.result.files.filter((f) => f.status !== 'cloud_only' && f.size <= 100 * 1024 * 1024).slice(0, 100) : [])
  const chosen = new SvelteMap<string, Attachment>()

  $effect(() => {
    if (!open) {
      query = ''
      chosen.clear()
    }
  })

  function pick() {
    onPick([...chosen.values()])
    open = false
  }

  function select({ id, name }: Attachment, checked: boolean) {
    if (checked) chosen.set(id, { id, name })
    else chosen.delete(id)
  }
</script>

<Dialog bind:open title={t('searchLibrary')} icon="folder" testid="library-picker">
  <label class="relative block">
    <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
    <input bind:value={query} type="search" class="ps-8" aria-label={t('searchFiles')} placeholder={t('searchFiles')} data-testid="library-query" />
  </label>
  <ul class="mt-2 max-h-80 overflow-y-auto" data-testid="library-results">
    {#each found as file (file.id)}
      <li class="flex items-center gap-2 rounded-lg px-1.5 py-1 hover:bg-wash">
        <Checkbox bind:checked={() => chosen.has(file.id), (on) => select(file, on)} class="min-w-0 flex-1 [&>span]:min-w-0">
          <bdi class="block truncate text-[13px]" title={file.name}>{file.name}</bdi>
        </Checkbox>
        <span class="shrink-0 text-[11px] text-faint">{size(file.size)}</span>
      </li>
    {:else}
      <li class="px-2 py-4"><SearchStatus {search} empty={t('noFiles')} /></li>
    {/each}
  </ul>
  {#snippet footer()}
    <span class="me-auto text-[12px] text-muted" role="status">{chosen.size ? t('selectedCount', { n: chosen.size }) : ''}</span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={!chosen.size} onclick={pick} data-testid="library-pick">{t('attachFile')}</button>
  {/snippet}
</Dialog>
