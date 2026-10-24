<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { ago, call, onChange, size, when } from '../../rpc'
  import { statusIcon, statusLabel, statusOf, statuses, statusTone, within } from './files'

  /**
   * One folder, selected: what lies under it (files, folders, their size
   * and where their current versions stand) and what can be done with it.
   * Counts come from the daemon's search within the folder.
   */
  let {
    folder,
    folders,
    unlocked,
    onOpen,
    onBackup,
    onGrant,
    onRename,
    onMove,
    onDiscard
  }: {
    folder: any
    folders: any[]
    unlocked: boolean
    onOpen: () => void
    onBackup: () => void
    onGrant: () => void
    onRename: () => void
    onMove: () => void
    onDiscard: () => void
  } = $props()

  let files = $state<any[] | null>(null)

  const subfolders = $derived(folders.filter((f) => f.id !== folder.id && within(folders, f.id, folder.id)).length)
  const bytes = $derived((files ?? []).reduce((n, f) => n + (f.size ?? 0), 0))
  const counts = $derived(statuses.map((status) => ({ status, n: (files ?? []).filter((f) => statusOf(f.status) === status).length })).filter((c) => c.n))

  const load = async () => {
    const id = folder.id
    const found = await call<any[]>('files/search', { query: '', folder: id }).catch(() => [])
    if (id === folder.id) files = found
  }

  $effect(() => {
    void folder.id
    files = null
    void load()
  })

  onMount(() => onChange(['files/changed'], load))
</script>

<section class="flex h-full min-w-0 flex-col" aria-label={folder.name} data-testid="files-folder-detail">
  <header class="flex flex-wrap items-start gap-x-3 gap-y-2 px-6 pt-5 pb-4">
    <div class="flex min-w-[min(100%,15rem)] flex-1 items-start gap-3">
      <span class="grid size-9 shrink-0 place-items-center text-forest [&_svg]:size-5"><Icon name="folder" /></span>
      <div class="min-w-0 flex-1">
        <h2 class="text-[17px] leading-snug font-semibold [overflow-wrap:anywhere]">{folder.name}</h2>
        <p class="mt-1 truncate text-[12px] text-faint" dir="ltr">/{[...(folder.path ?? []), folder.name].join('/')}</p>
      </div>
    </div>
    <div class="ms-auto flex shrink-0 flex-wrap items-center gap-1">
      <button class={buttonClass('secondary', 'md')} onclick={onOpen} data-testid="files-folder-open"><Icon name="folder-open" />{t('open')}</button>
      {#if unlocked}
        <button class={buttonClass('secondary', 'md')} disabled={!files?.length} onclick={onBackup}><Icon name="cloud-up" />{t('backUp')}</button>
      {/if}
    </div>
  </header>
  <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
    <dl class="grid gap-1" data-testid="files-folder-facts">
      <div class="grid grid-cols-[112px_1fr] items-center gap-x-4 py-1.5">
        <dt class="text-[12px] text-faint">{t('folderContents')}</dt>
        <dd>{files ? t('folderCounts', { files: files.length, folders: subfolders, bytes: size(bytes) }) : '…'}</dd>
      </div>
      <div class="grid grid-cols-[112px_1fr] items-center gap-x-4 py-1.5">
        <dt class="text-[12px] text-faint">{t('modified')}</dt>
        <dd title={when(folder.updated_at)}>{ago(folder.updated_at)}</dd>
      </div>
      {#if counts.length}
        <div class="grid grid-cols-[112px_1fr] items-start gap-x-4 py-2">
          <dt class="text-[12px] text-faint">{t('backupState')}</dt>
          <dd class="flex flex-wrap gap-1.5">
            {#each counts as count (count.status)}
              <Pill tone={statusTone[count.status]}><Icon name={statusIcon[count.status]} />{t(statusLabel[count.status])} · {count.n}</Pill>
            {/each}
          </dd>
        </div>
      {/if}
    </dl>
    {#if unlocked}
      <div class="mt-4 flex flex-wrap gap-2">
        <button class={buttonClass('secondary', 'sm')} onclick={onGrant} data-testid="files-folder-grant"><Icon name="agents" />{t('grantAgent')}</button>
        <button class={buttonClass('secondary', 'sm')} onclick={onRename}><Icon name="edit" />{t('renameFile')}</button>
        <button class={buttonClass('secondary', 'sm')} onclick={onMove}><Icon name="folder" />{t('moveTo')}</button>
        <button class={buttonClass('danger', 'sm')} onclick={onDiscard}><Icon name="trash" />{t('moveToTrash')}</button>
      </div>
    {/if}
  </div>
  <footer class="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-line px-6 py-2.5 text-[11.5px] text-faint">
    <span class="inline-flex items-center gap-1.5"><Icon name="folder" class="size-3.5" />{t('createdAt')} {ago(folder.created_at)}</span>
    <span title={folder.id}>ID <bdi dir="ltr" class="font-mono">{String(folder.id).slice(0, 10)}</bdi></span>
  </footer>
</section>
