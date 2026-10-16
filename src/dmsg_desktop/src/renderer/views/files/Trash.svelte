<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, size } from '../../rpc'
  import { fileIcon, ordered } from './files'

  /** The trash: folders and files on their way out, restored or deleted for good; the cloud is not touched. */
  let {
    listing,
    days,
    unlocked,
    busy,
    onRestore,
    onPurge,
    onEmpty
  }: {
    listing: { folders: any[]; files: any[] } | null
    days: number
    unlocked: boolean
    busy: boolean
    onRestore: (id: string) => void
    onPurge: (id: string) => void
    onEmpty: () => void
  } = $props()

  const rows = $derived(listing ? ordered(listing.folders.map((f) => ({ ...f, kind: 'folder' })), listing.files.map((f) => ({ ...f, kind: 'file' })), locale()) : [])
</script>

<section class="flex h-full min-w-0 flex-col" data-testid="files-trash">
  <header class="flex items-center gap-2 border-b border-line px-5 py-3">
    <div class="min-w-0 flex-1">
      <h2 class="text-[15px] font-semibold">{t('trash')}</h2>
      <p class="text-[12px] text-faint">{t('trashHint', { days })}</p>
    </div>
    {#if rows.length && unlocked}
      <button class={buttonClass('danger', 'md')} disabled={busy} onclick={onEmpty} data-testid="files-trash-empty">
        <Icon name="trash" />{t('emptyTheTrash')}
      </button>
    {/if}
  </header>
  <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
    {#if !rows.length}
      {#if listing}
        <p class="py-14 text-center text-[13px] text-muted">{t('emptyTrash')}</p>
      {/if}
    {:else}
      <ul class="divide-y divide-line rounded-xl border border-line bg-surface shadow-xs">
        {#each rows as row (row.id)}
          <li class="flex items-center gap-3 px-3 py-2.5" data-id={row.id}>
            <span class="grid size-8 shrink-0 place-items-center rounded-lg border border-line bg-surface-2 text-faint"><Icon name={row.kind === 'folder' ? 'folder' : fileIcon(row.mime)} /></span>
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13.5px] font-medium">{row.name}</div>
              <div class="truncate text-[11.5px] text-faint">
                {row.kind === 'file' ? `${size(row.size)} · ` : ''}/{(row.path ?? []).join('/')} · {t('moveToTrash')} {ago(row.trashed_at)}
              </div>
            </div>
            {#if unlocked}
              <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => onRestore(row.id)} data-testid="files-trash-restore"><Icon name="restore" />{t('restore')}</button>
              <button class={buttonClass('danger', 'sm')} disabled={busy} onclick={() => onPurge(row.id)} data-testid="files-trash-purge"><Icon name="trash" />{t('deleteForever')}</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <Notice tone="info" class="mt-4">{t('deleteVersionHint')}</Notice>
  </div>
</section>
