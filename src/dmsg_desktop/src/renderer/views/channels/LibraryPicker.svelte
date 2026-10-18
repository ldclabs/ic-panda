<script lang="ts">
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, size } from '../../rpc'

  /** Files of the library to send, found by name; several at once, each in a message of its own. */
  let { open = $bindable(false), onPick }: { open?: boolean; onPick: (files: { id: string; name: string }[]) => void } = $props()

  let query = $state('')
  let found = $state<any[]>([])
  let chosen = $state<Record<string, boolean>>({})

  $effect(() => {
    if (!open) {
      query = ''
      chosen = {}
      return
    }
    const asked = query
    const timer = setTimeout(async () => {
      const list = await call<any[]>('files/search', { query: asked }).catch(() => [])
      if (asked === query) found = list.filter((f) => f.status !== 'cloud_only' && f.status !== 'too_large').slice(0, 100)
    }, 150)
    return () => clearTimeout(timer)
  })

  function pick() {
    onPick(found.filter((f) => chosen[f.id]).map((f) => ({ id: f.id, name: f.name })))
    open = false
  }
</script>

<Dialog bind:open title={t('searchLibrary')} icon="folder" testid="library-picker">
  <label class="relative block">
    <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
    <input bind:value={query} type="search" class="ps-8" placeholder={t('searchFiles')} data-testid="library-query" />
  </label>
  <ul class="mt-2 max-h-80 overflow-y-auto" data-testid="library-results">
    {#each found as file (file.id)}
      <li class="flex items-center gap-2 rounded-lg px-1.5 py-1 hover:bg-wash">
        <Checkbox bind:checked={() => chosen[file.id] ?? false, (on) => (chosen[file.id] = on)} class="min-w-0 flex-1">
          <span class="min-w-0 truncate text-[13px]">{file.name}</span>
        </Checkbox>
        <span class="shrink-0 text-[11px] text-faint">{size(file.size)}</span>
      </li>
    {:else}
      <li class="px-2 py-4 text-center text-[12.5px] text-faint">{t('noFiles')}</li>
    {/each}
  </ul>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={!Object.values(chosen).some(Boolean)} onclick={pick} data-testid="library-pick">{t('attachFile')}</button>
  {/snippet}
</Dialog>
