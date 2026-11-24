<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Id from '../../components/page/Id.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { ago, onChange, size, when } from '../../rpc'
  import { foldersByParent, folderSubtree, statusIcon, statusLabel, statusOf, statuses, statusTone } from './files'
  import { searchLibrary } from './search.svelte'
  import SearchStatus from './SearchStatus.svelte'

  /**
   * One folder, selected: what lies under it (files, folders, their size
   * and where their current versions stand) and what can be done with it.
   * Counts come from the daemon's search within the folder.
   */
  let {
    standalone = false,
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
    standalone?: boolean
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

  const search = searchLibrary(() => '', 0, () => folder.id)
  const files = $derived(search.result.kind === 'ready' ? search.result.files : null)

  const children = $derived(foldersByParent(folders))
  const subfolders = $derived(folderSubtree(children, [folder.id]).size - 1)
  const bytes = $derived((files ?? []).reduce((n, f) => n + (f.size ?? 0), 0))
  const counts = $derived(statuses.map((status) => ({ status, n: (files ?? []).filter((f) => statusOf(f.status) === status).length })).filter((c) => c.n))

  onMount(() => onChange(['files/changed'], search.retry))
</script>

<section class="flex h-full min-w-0 flex-col" aria-label={folder.name} data-testid="files-folder-detail">
  <header class="flex flex-wrap items-start gap-x-3 gap-y-2 px-6 pt-5 pb-4">
    <div class="flex min-w-[min(100%,15rem)] flex-1 items-start gap-3">
      <span class="grid size-9 shrink-0 place-items-center text-forest [&_svg]:size-5"><Icon name="folder" /></span>
      <div class="min-w-0 flex-1">
        <h2 class="text-title-2 leading-snug font-semibold [overflow-wrap:anywhere]">{folder.name}</h2>
        <p class="mt-1 truncate text-callout text-muted" dir="ltr">/{[...(folder.path ?? []), folder.name].join('/')}</p>
      </div>
    </div>
    <div class="ms-auto flex shrink-0 flex-wrap items-center gap-1">
      <button class={buttonClass('ghost', 'md')} onclick={onOpen} data-testid="files-folder-open"><Icon name="folder-open" />{t('open')}</button>
      {#if unlocked}
        {#if !standalone}<button class={buttonClass('ghost', 'md')} disabled={!files?.length} onclick={onBackup}><Icon name="cloud-up" />{t('backUp')}</button>{/if}
      {/if}
    </div>
  </header>
  <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
    <dl class="grid gap-1" data-testid="files-folder-facts">
      <div class="grid grid-cols-[112px_1fr] items-center gap-x-4 py-1.5">
        <dt class="text-callout text-muted">{t('folderContents')}</dt>
        <dd class="min-w-0">{#if files}{t('folderCounts', { files: files.length, folders: subfolders, bytes: size(bytes) })}{:else}<SearchStatus {search} empty={t('loading')} />{/if}</dd>
      </div>
      <div class="grid grid-cols-[112px_1fr] items-center gap-x-4 py-1.5">
        <dt class="text-callout text-muted">{t('modified')}</dt>
        <dd title={when(folder.updated_at)}>{ago(folder.updated_at)}</dd>
      </div>
      {#if counts.length}
        <div class="grid grid-cols-[112px_1fr] items-start gap-x-4 py-2">
          <dt class="text-callout text-muted">{t('backupState')}</dt>
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
        {#if !standalone}<button class={buttonClass('secondary', 'sm')} onclick={onGrant} data-testid="files-folder-grant"><Icon name="agents" />{t('grantAgent')}</button>{/if}
        <button class={buttonClass('secondary', 'sm')} onclick={onRename}><Icon name="edit" />{t('renameFile')}</button>
        <button class={buttonClass('secondary', 'sm')} onclick={onMove}><Icon name="folder" />{t('moveTo')}</button>
        <button class={buttonClass('danger', 'sm')} onclick={onDiscard}><Icon name="trash" />{t('moveToTrash')}</button>
      </div>
    {/if}
  </div>
  <footer class="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-line px-6 py-2.5 text-caption text-faint">
    <span class="inline-flex items-center gap-1.5"><Icon name="folder" class="size-3.5" />{t('createdAt')} {ago(folder.created_at)}</span>
    <Id id={folder.id} />
  </footer>
</section>
