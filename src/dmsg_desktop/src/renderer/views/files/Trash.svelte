<script lang="ts">
  import Checkbox from '../../components/Checkbox.svelte'
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, size } from '../../rpc'
  import { fileIcon, ordered } from './files'

  /**
   * The trash: folders and files on their way out, restored or deleted for
   * good, one by one or ticked together; a file opens beside the list. The
   * cloud is not touched.
   */
  let {
    listing,
    days,
    unlocked,
    busy,
    opened,
    onOpen,
    onRestore,
    onPurge,
    onEmpty
  }: {
    listing: { folders: any[]; files: any[] } | null
    days: number
    unlocked: boolean
    busy: boolean
    /** The file shown beside the list. */
    opened: string | null
    onOpen: (id: string | null) => void
    onRestore: (ids: string[]) => void
    onPurge: (ids: string[]) => void
    onEmpty: () => void
  } = $props()

  let ticked = $state<string[]>([])

  const rows = $derived(listing ? ordered(listing.folders.map((f) => ({ ...f, kind: 'folder' })), listing.files.map((f) => ({ ...f, kind: 'file' })), locale()) : [])
  /** The ticked rows still in the trash: what left it leaves the selection. */
  const picked = $derived(ticked.filter((id) => rows.some((r) => r.id === id)))

  const toggle = (id: string, on: boolean) => (ticked = on ? [...new Set([...picked, id])] : picked.filter((p) => p !== id))
</script>

<section class="flex h-full min-w-0 flex-col" data-testid="files-trash">
  <header class="flex flex-wrap items-center gap-2 border-b border-line px-5 py-3">
    <div class="min-w-0 flex-1">
      <h2 class="text-[15px] font-semibold">{t('trash')}</h2>
      <p class="text-[12px] text-faint">{t('trashHint', { days })}</p>
    </div>
    {#if unlocked && picked.length}
      <span class="text-[12px] text-faint">{t('selectedCount', { n: picked.length })}</span>
      <button class={buttonClass('secondary', 'md')} disabled={busy} onclick={() => onRestore(picked)} data-testid="files-trash-restore-picked"><Icon name="restore" />{t('restore')}</button>
      <button class={buttonClass('danger', 'md')} disabled={busy} onclick={() => onPurge(picked)} data-testid="files-trash-purge-picked"><Icon name="trash" />{t('deleteForever')}</button>
    {:else if rows.length && unlocked}
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
      <ul class="divide-y divide-line">
        {#each rows as row (row.id)}
          <li class={cn('flex items-center gap-3 px-3 py-2.5', opened === row.id && 'bg-pale')} data-id={row.id}>
            {#if unlocked}
              <Checkbox checked={picked.includes(row.id)} onCheckedChange={(on) => toggle(row.id, on)} testid="files-trash-pick" />
            {/if}
            <button
              class="flex min-w-0 flex-1 items-center gap-3 text-start disabled:cursor-default"
              disabled={row.kind !== 'file'}
              onclick={() => onOpen(opened === row.id ? null : row.id)}
              data-testid="files-trash-open"
            >
              <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-surface-2 text-faint"><Icon name={row.kind === 'folder' ? 'folder' : fileIcon(row.mime)} /></span>
              <span class="min-w-0 flex-1">
                <span class="block truncate text-[13.5px] font-medium">{row.name}</span>
                <span class="block truncate text-[11.5px] text-faint">
                  {row.kind === 'file' ? `${size(row.size)} · ` : ''}<span dir="ltr">/{(row.path ?? []).join('/')}</span> · {t('moveToTrash')} {ago(row.trashed_at)}
                </span>
              </span>
            </button>
            {#if unlocked}
              <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => onRestore([row.id])} data-testid="files-trash-restore"><Icon name="restore" />{t('restore')}</button>
              <button class={buttonClass('danger', 'sm')} disabled={busy} onclick={() => onPurge([row.id])} data-testid="files-trash-purge"><Icon name="trash" />{t('deleteForever')}</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <Notice tone="info" class="mt-4">{t('trashCloudHint')}</Notice>
  </div>
</section>
