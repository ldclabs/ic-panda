<script lang="ts">
  import { onDestroy } from 'svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { bridge, errorText } from '../../rpc'
  import { typeIcon, typeOf } from './vault'

  /**
   * Importing another password manager's export, read on this device: the
   * main process asks for the file, the daemon reads it and shows what it
   * holds without hidden values; items already here start unticked. The
   * same file is read again to add the ticked ones, if it has not changed.
   */
  let { open = $bindable(false) }: { open?: boolean } = $props()

  let preview = $state<any>(null)
  let ticked = $state<Set<number>>(new Set())
  let busy = $state(false)
  let error = $state('')
  let generation = 0

  const count = $derived(ticked.size)

  $effect(() => {
    if (!open) {
      generation += 1
      preview = null
      ticked = new Set()
      error = ''
      void window.dmsg.vaultImportCancel().catch(() => {})
    }
  })

  onDestroy(() => {
    generation += 1
    void window.dmsg.vaultImportCancel().catch(() => {})
  })

  async function choose() {
    if (busy) return
    const current = ++generation
    busy = true
    preview = null
    ticked = new Set()
    error = ''
    try {
      const read = await bridge<any>(() => window.dmsg.vaultImportPreview())
      if (!read || current !== generation || !open) return
      preview = read
      ticked = new Set(read.items.filter((i: any) => !i.duplicate).map((i: any) => i.index))
    } catch (e) {
      if (current === generation && open) error = errorText(e)
    } finally {
      busy = false
    }
  }

  function toggle(index: number, on: boolean) {
    const next = new Set(ticked)
    if (on) next.add(index)
    else next.delete(index)
    ticked = next
  }

  async function add() {
    if (busy || !preview) return
    const current = generation
    const session = preview.session
    busy = true
    error = ''
    try {
      const added = await bridge<{ ids: string[] }>(() => window.dmsg.vaultImport(session, [...ticked]))
      if (current === generation) open = false
      toast(() => t('imported', { n: added.ids.length }), { ms: 8000 })
    } catch (e) {
      if (current === generation && open) {
        preview = null
        ticked = new Set()
        error = errorText(e)
      }
    } finally {
      busy = false
    }
  }
</script>

<Dialog bind:open title={t('importTitle')} description={t('importHint')} icon="import" wide testid="vault-import">
  {#if !preview}
    <div class="grid justify-items-start gap-3 text-[13px] text-muted">
      <p>{t('importFormats')}</p>
      <button class={buttonClass('primary')} disabled={busy} onclick={choose} data-testid="vault-import-choose"><Icon name="import" />{t('chooseExport')}</button>
    </div>
  {:else}
    <div class="grid gap-2">
      <div class="flex flex-wrap items-center gap-2 text-[12.5px] text-muted">
        <span>{t('importRead', { n: preview.items.length, format: preview.format })}</span>
        {#if preview.skipped}<span class="text-warn">{t('importSkipped', { n: preview.skipped })}</span>{/if}
        <span class="min-w-0 flex-1"></span>
        <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={choose} data-testid="vault-import-rechoose">{t('chooseExport')}</button>
        <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => (ticked = new Set(preview.items.map((i: any) => i.index)))}>{t('selectAll')}</button>
        <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => (ticked = new Set())}>{t('selectNone')}</button>
      </div>
      <ul class="grid max-h-[50vh] gap-0.5 overflow-y-auto rounded-xl border border-line p-1" data-testid="vault-import-items">
        {#each preview.items as entry (entry.index)}
          <li class={cn('flex items-center gap-2 rounded-lg px-2 py-1.5 text-[12.5px]', ticked.has(entry.index) ? 'bg-pale' : '')}>
            <Checkbox checked={ticked.has(entry.index)} onCheckedChange={(on) => toggle(entry.index, on)} testid="vault-import-tick" />
            <Icon name={typeIcon[typeOf(entry)]} class="shrink-0 text-faint" />
            <span class="min-w-0 flex-1">
              <span class="block truncate font-medium text-ink">{entry.title}</span>
              <span class="block truncate text-[11.5px] text-faint">{[entry.username, entry.url].filter(Boolean).join(' · ') || t('fieldCount', { n: entry.fields })}</span>
            </span>
            {#if entry.duplicate}<span class="shrink-0 rounded bg-wash px-1.5 py-px text-[11px] text-muted">{t('alreadyHere')}</span>{/if}
          </li>
        {/each}
      </ul>
      {#if preview.incomplete}<p class="text-[12px] text-warn" data-testid="vault-import-incomplete">{t('importIncomplete', { n: preview.incomplete })}</p>{/if}
      <p class="flex items-start gap-1.5 text-[12px] text-warn"><Icon name="warning" class="mt-0.5 shrink-0" />{t('deleteExport')}</p>
    </div>
  {/if}
  {#if error}<p class="mt-2 text-[12.5px] text-danger">{error}</p>{/if}
  {#snippet footer()}
    <span class="min-w-0 flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    {#if preview}
      <button class={buttonClass('primary')} disabled={busy || !count} onclick={add} data-testid="vault-import-add">{t('importN', { n: count })}</button>
    {/if}
  {/snippet}
</Dialog>
