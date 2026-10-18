<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import Locked from '../components/page/Locked.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Select from '../components/Select.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { locale, t } from '../i18n/index.svelte'
  import { copyText } from '../components/clipboard'
  import { bridge, call, closed, code, errorText, onChange, single, size } from '../rpc'
  import { added, backups, dismiss, failed, picked, progress, type Job } from './files/adding.svelte'
  import Cloud from './files/Cloud.svelte'
  import Detail from './files/Detail.svelte'
  import { matchesFile, ordered, pathOf, ROOT } from './files/files'
  import FolderPicker from './files/FolderPicker.svelte'
  import Jobs from './files/Jobs.svelte'
  import Row from './files/Row.svelte'
  import Trash from './files/Trash.svelte'
  import Tree from './files/Tree.svelte'

  /**
   * The file library: the folders and places on the left, the current
   * folder in the middle, the selected file on the right. Files come in by
   * the system dialog or a drop and are encrypted on this device; backing
   * up, sharing and deleting a cloud copy are explicit actions, each
   * confirmed, and no deletion on either side reaches the other. Each change
   * comes back through the daemon's `files/changed`.
   */
  let { status, mod, route, go }: { status: any; mod: string; route: string; go: (path: string) => void } = $props()

  /** A folder ID, or `starred`, `cloud`, `trash`. */
  let scope = $state<string>(ROOT)
  let folders = $state<any[]>([])
  let listing = $state<{ folder: any; folders: any[]; files: any[] } | null>(null)
  let found = $state<any[] | null>(null)
  let trashListing = $state<{ folders: any[]; files: any[] } | null>(null)
  let cloud = $state<any[]>([])
  let cloudLoaded = $state(false)
  let cloudUsage = $state<any>(null)
  let usage = $state<any>(null)
  let query = $state('')
  let selected = $state<string[]>([])
  let details = $state<any>(null)
  let error = $state('')
  let loaded = $state(false)
  let busy = $state(false)
  let dragging = $state(0)
  let search = $state<HTMLInputElement | null>(null)
  let list = $state<HTMLElement | null>(null)
  /** Dialogs: each holds what it acts on. */
  let naming = $state<{ parent: string } | null>(null)
  let renaming = $state<{ id: string; name: string } | null>(null)
  let moving = $state<string[] | null>(null)
  let sharing = $state<{ id: string; version: string | null; name: string; channel: string; text: string; channels: any[] } | null>(null)
  let backing = $state<{ ids: string[]; allVersions: boolean; count: number; bytes: number; usage: any } | null>(null)
  let deletingCloud = $state<string[] | null>(null)
  let changedCopy = $state<any | null>(null)
  /** `files/mount_status`: whether the library is a disk right now, and whether it may be. */
  let mount = $state<any>(null)
  let mounting = $state(false)
  /** `files/share_links`: the links this device made, for the detail's list. */
  let links = $state<any[]>([])
  let linking = $state<{ id: string; version: string | null; name: string; days: number; maxDownloads: number; password: string; result: any | null } | null>(null)
  let draft = $state('')

  const unlocked = $derived(status?.state === 'unlocked')
  const folderScope = $derived(scope !== 'starred' && scope !== 'cloud' && scope !== 'trash')
  const names = $derived(Object.fromEntries((listing?.files ?? []).concat(found ?? []).map((f: any) => [f.id, f.name])))
  const rows = $derived.by(() => {
    if (found) return found.map((f) => ({ ...f, kind: 'file' }))
    if (!listing) return []
    const files = listing.files.filter((f) => matchesFile(f, query)).map((f) => ({ ...f, kind: 'file' }))
    const subfolders = query.trim() ? [] : listing.folders.map((f) => ({ ...f, kind: 'folder' }))
    return ordered(subfolders, files, locale())
  })
  const current = $derived(selected.length === 1 ? rows.find((r) => r.id === selected[0] && r.kind === 'file') ?? null : null)
  /** The details follow the selected file's ID, not each new row object a reload or search makes. */
  const currentId = $derived<string | null>(current?.id ?? null)
  const path = $derived(pathOf(folders, folderScope ? scope : ROOT))

  const fail = (e: unknown) => toast(() => errorText(e))

  async function attempt(work: () => Promise<unknown>, done?: string) {
    busy = true
    try {
      await work()
      if (done) toast(() => done)
    } catch (e) {
      fail(e)
    } finally {
      busy = false
    }
  }

  // The route names the place; the place changes the route.
  $effect(() => {
    const next = route.startsWith('/files/cloud') ? 'cloud' : route.startsWith('/files/trash') ? 'trash' : null
    if (next && next !== scope) scope = next
    if (!next && (scope === 'cloud' || scope === 'trash')) scope = ROOT
  })

  function pick(next: string) {
    scope = next
    query = ''
    found = null
    selected = []
    if (next === 'cloud') go('/files/cloud')
    else if (next === 'trash') go('/files/trash')
    else if (route !== '/files') go('/files')
  }

  async function showDetails(id: string | null) {
    const next = id ? await call('files/show', { id }).catch(() => null) : null
    if (currentId === id) details = next
  }

  /** The place shown and the library around it, read together. */
  const reload = single(async () => {
    const place = scope
    try {
      const [tree, shown, trashed, cloudList, cloudRoom, local, disk, shares, jobs] = await Promise.all([
        call('files/tree'),
        folderScope ? call('files/list', { folder: place }) : place === 'starred' ? call<any[]>('files/search', { query: '' }) : null,
        place === 'trash' ? call('files/list', { trash: true }) : null,
        place === 'cloud' ? call('files/cloud_list') : null,
        place === 'cloud' ? call('files/cloud_usage').catch(() => null) : null,
        call('files/usage').catch(() => null),
        call('files/mount_status').catch(() => null),
        call('files/share_links').catch(() => []),
        call('files/backup_status').catch(() => [])
      ])
      folders = tree
      listing = Array.isArray(shown) ? { folder: null, folders: [], files: shown.filter((f) => f.starred) } : shown
      if (trashed) trashListing = trashed
      if (cloudList) {
        cloud = cloudList
        cloudUsage = cloudRoom ?? cloudUsage
        cloudLoaded = true
      }
      usage = local
      mount = disk
      links = shares
      backups(jobs)
      error = ''
    } catch (e) {
      listing = null
      folders = []
      error = closed(e, status) ? 'LOCKED' : errorText(e)
    }
    loaded = true
    selected = selected.filter((id) => rows.some((r) => r.id === id))
    await showDetails(currentId)
  })

  // Read untracked: the reload's own reads are not this effect's to follow.
  $effect(() => {
    void scope
    untrack(() => void reload())
  })

  // The folder's own matches first; a search across the library once the text leaves this folder's names.
  $effect(() => {
    const q = query.trim()
    if (!q || !folderScope || listing?.files.some((f) => matchesFile(f, q))) {
      found = null
      return
    }
    const timer = setTimeout(() => void call<any[]>('files/search', { query: q }).then((list) => q === query.trim() && (found = list)).catch(() => (found = [])), 150)
    return () => clearTimeout(timer)
  })

  $effect(() => void showDetails(currentId))

  function select(id: string, event?: MouseEvent) {
    if (event && (event.metaKey || event.ctrlKey)) selected = selected.includes(id) ? selected.filter((s) => s !== id) : [...selected, id]
    else if (event?.shiftKey && selected.length) {
      const ids = rows.map((r) => r.id)
      const [a, b] = [ids.indexOf(selected[0]), ids.indexOf(id)].sort((x, y) => x - y)
      selected = ids.slice(a, b + 1)
    } else selected = [id]
  }

  function onListKey(event: KeyboardEvent) {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
    event.preventDefault()
    const at = rows.findIndex((r) => r.id === selected[0])
    const next = rows[Math.max(0, Math.min(rows.length - 1, at + (event.key === 'ArrowDown' ? 1 : -1)))]
    if (next) {
      selected = [next.id]
      void tick().then(() => list?.querySelector<HTMLElement>(`[data-id="${next.id}"]`)?.focus())
    }
  }

  /** ⌘F focuses the search, ⌘N makes a folder (Ctrl elsewhere). */
  function onKey(event: KeyboardEvent) {
    const held = mod === '⌘' ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
    if (!held || event.altKey || event.shiftKey) return
    const key = event.key.toLowerCase()
    if (key === 'f') {
      event.preventDefault()
      search?.focus()
      search?.select()
    } else if (key === 'n' && unlocked && folderScope) {
      event.preventDefault()
      naming = { parent: scope }
      draft = ''
    }
  }

  /** Files reach the daemon through the main process: a dialog, or the paths of what was dropped. */
  async function add(dropped?: File[], directory = false) {
    const folder = folderScope ? scope : ROOT
    try {
      const result: any = await bridge(() => window.dmsg.filesAdd(folder, dropped, directory))
      if (result) {
        added(result.names, result.files, (n) => t('duplicateOf', { n }))
        // A single file opens once the list holds it.
        if (result.files.length === 1 && result.files[0].file) {
          await reload()
          selected = [result.files[0].file.id]
        }
      }
    } catch (e) {
      failed(code(e))
    }
  }

  const files = (event: DragEvent) => !!event.dataTransfer?.types.includes('Files')
  function onDragEnter(event: DragEvent) {
    if (!files(event) || !unlocked || !folderScope) return
    event.preventDefault()
    dragging++
  }
  function onDragOver(event: DragEvent) {
    if (!files(event) || !unlocked || !folderScope) return
    event.preventDefault()
    event.dataTransfer!.dropEffect = 'copy'
  }
  function onDrop(event: DragEvent) {
    if (!files(event)) return
    event.preventDefault()
    dragging = 0
    if (!unlocked || !folderScope) return
    const dropped = Array.from(event.dataTransfer?.files ?? [])
    if (dropped.length) {
      // Named here too: a drop the preload refuses never reaches the main process's `files/adding`.
      picked(dropped.map((f) => f.name))
      void add(dropped)
    }
  }

  const open = (id: string, version: string | null) => attempt(() => bridge(() => window.dmsg.filesOpen(id, version)))
  const exportFile = (id: string, version: string | null, name: string) => attempt(() => bridge(() => window.dmsg.filesExport(id, version, name)))
  const star = (file: any) => attempt(() => call('files/update', { id: file.id, starred: !file.starred }))
  const discard = (ids: string[]) => attempt(() => call(scope === 'trash' ? 'files/restore' : 'files/trash', { ids }), scope === 'trash' ? t('restored') : t('movedToTrash'))
  const purge = (ids: string[] | null) => attempt(() => call('files/purge', ids ? { ids } : {}))
  const move = (ids: string[], folder: string) => attempt(() => call('files/move', { ids, folder }), t('moved'))

  const createFolder = () =>
    attempt(async () => {
      const name = draft.trim()
      if (!name || !naming) return
      await call('files/mkdir', { parent: naming.parent, name })
      naming = null
    })
  const rename = () =>
    attempt(async () => {
      const name = draft.trim()
      if (!name || !renaming) return
      await call('files/rename', { id: renaming.id, name })
      renaming = null
    })

  /** Backing up shows what goes and the cloud's room first; nothing is uploaded before the confirmation. */
  async function askBackup(ids: string[], allVersions: boolean) {
    const chosen = rows.filter((r) => ids.includes(r.id))
    const count = chosen.filter((r) => r.kind === 'file').length || ids.length
    const bytes = chosen.filter((r) => r.kind === 'file').reduce((n, r) => n + (r.size ?? 0), 0)
    const usage = await call('files/cloud_usage').catch(() => null)
    backing = { ids, allVersions, count, bytes, usage }
  }
  const backup = () =>
    attempt(async () => {
      if (!backing) return
      const request: any = await call('files/backup', { ids: backing.ids, all_versions: backing.allVersions })
      backing = null
      if (request.skipped?.length) toast(() => t('backupSkipped', { n: request.skipped.length }))
    }, t('backupStarted'))

  async function askShare(id: string, version: string | null, name: string) {
    const channels: any[] = await call('channel/list').catch(() => [])
    const open = channels.filter((c) => c.ledger?.status === 'active')
    sharing = { id, version, name, channel: open[0]?.id ?? '', text: '', channels: open }
  }
  const share = () =>
    attempt(async () => {
      if (!sharing?.channel) return
      await call('files/share_channel', { channel: sharing.channel, id: sharing.id, version: sharing.version, text: sharing.text })
      sharing = null
    }, t('savedVerified'))

  const askLink = (id: string, version: string | null, name: string) => (linking = { id, version, name, days: 7, maxDownloads: 0, password: '', result: null })
  const createLink = () =>
    attempt(async () => {
      if (!linking) return
      linking.result = await call('files/share_link', {
        id: linking.id,
        version: linking.version,
        expires_in_ms: linking.days * 86_400_000,
        max_downloads: linking.maxDownloads,
        password: linking.password || null
      })
    })
  const revokeLink = (shareId: string) => attempt(() => call('files/share_revoke', { share_id: shareId }), t('linkRevoked'))
  const refreshLinks = () => attempt(async () => (links = await call('files/share_links', { refresh: true })))

  const mountDisk = () =>
    attempt(async () => {
      mount = await call('files/mount')
      mounting = false
    }, t('diskMounted'))
  const unmountDisk = () => attempt(async () => (mount = await call('files/unmount')), t('diskUnmounted'))
  const refreshCloud = () => attempt(() => call('files/cloud_refresh'))
  const fetchBackup = (upload: string, folder: string | null) => attempt(() => call('files/cloud_fetch', { upload_id: upload, folder }), t('fetched'))
  const verifyBackup = (upload: string) =>
    attempt(async () => {
      const check: any = await call('files/cloud_verify', { upload_id: upload })
      toast(() => (check.ok ? t('verified') : `${t('error')}: ${check.error}`))
    })
  const deleteCloud = () =>
    attempt(async () => {
      if (!deletingCloud) return
      await call('files/cloud_delete', { upload_ids: deletingCloud })
      deletingCloud = null
    }, t('cloudDeleted'))
  /** A cancelled backup leaves its card at once; the daemon keeps the version on this device. */
  const cancelBackup = (job: Job) =>
    attempt(async () => {
      await call('files/backup_cancel', { id: job.name, version: job.version })
      dismiss(job.key)
    })

  /** A copy another program changed: kept as a new version, or let go; the copy is removed either way. */
  const settleCopy = (save: boolean) =>
    attempt(async () => {
      if (!changedCopy) return
      await call('files/open_cleanup', { nonce: changedCopy.nonce, save })
      changedCopy = null
    })

  onMount(() => {
    const offChange = onChange(['files/changed', 'session/changed'], reload)
    const offWork = window.dmsg.onEvent((event) => {
      const data = (event.data ?? {}) as any
      if (event.type === 'files/adding') picked(Array.isArray(data.names) ? data.names : [])
      else if (event.type === 'files/progress') progress(data)
      else if (event.type === 'files/open_changed') changedCopy = data
      // A mount or unmount names only its method; the daemon's own announcement carries the status.
      else if (event.type === 'files/mount_changed') void call('files/mount_status').then((next) => (mount = next)).catch(() => {})
    })
    return () => {
      offChange()
      offWork()
    }
  })
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="@container relative flex h-full min-h-0"
  ondragenter={onDragEnter}
  ondragover={onDragOver}
  ondragleave={(event) => files(event) && (dragging = Math.max(0, dragging - 1))}
  ondrop={onDrop}
  data-testid="files"
>
  {#if error === 'LOCKED'}
    <Locked {status} testid="files-locked" />
  {:else}
    <aside class="flex w-[220px] shrink-0 flex-col border-e border-line">
      <Tree {folders} {scope} {unlocked} onPick={pick} onDropMove={move} onNewFolder={(parent) => ((naming = { parent }), (draft = ''))} />
      {#if usage}
        <p class="border-t border-line px-3.5 py-2 text-[11px] text-faint">{t('localUsage', { files: usage.files, versions: usage.versions, bytes: size(usage.blob_bytes) })}</p>
        {#if usage.last_check && usage.last_check.blobs_bad.length + usage.last_check.versions_bad.length > 0}
          <p class="flex items-start gap-1.5 px-3.5 pb-2 text-[11px] text-warn" data-testid="files-check-warning"><Icon name="warning" class="mt-px size-3.5 shrink-0" />{t('selfCheckDamage', { n: usage.last_check.blobs_bad.length + usage.last_check.versions_bad.length })}</p>
        {/if}
      {/if}
      {#if mount?.supported}
        <div class="border-t border-line px-3.5 py-2 text-[11px] text-faint" data-testid="files-mount">
          {#if mount.mounted}
            <p class="truncate" title={mount.mount_point ?? mount.url}>{t('mountedAt', { point: mount.mount_point ?? mount.url })}</p>
            <button class="mt-1 text-forest hover:underline disabled:opacity-50" disabled={busy} onclick={unmountDisk} data-testid="files-unmount">{t('unmountDisk')}</button>
          {:else if mount.enabled}
            <button class="flex items-center gap-1.5 text-forest hover:underline disabled:opacity-50" disabled={busy || !unlocked} onclick={() => (mounting = true)} data-testid="files-mount-open">
              <Icon name="drive" class="size-3.5" />{t('mountDisk')}
            </button>
          {:else}
            <p>{t('mountOff')}</p>
          {/if}
        </div>
      {/if}
    </aside>

    {#if scope === 'cloud'}
      <div class="min-w-0 flex-1">
        <Cloud entries={cloud} usage={cloudUsage} {unlocked} {busy} loaded={cloudLoaded} onRefresh={refreshCloud} onFetch={(upload) => fetchBackup(upload, null)} onVerify={verifyBackup} onDelete={(uploads) => (deletingCloud = uploads)} />
      </div>
    {:else if scope === 'trash'}
      <div class="min-w-0 flex-1">
        <Trash listing={trashListing} days={usage?.trash_days ?? 30} {unlocked} {busy} onRestore={(id) => discard([id])} onPurge={(id) => purge([id])} onEmpty={() => purge(null)} />
      </div>
    {:else}
      <section class="relative flex w-[280px] shrink-0 flex-col border-e border-line @min-[980px]:w-[320px]">
        <div class="grid gap-2 p-3 pb-2">
          <div class="flex items-center gap-1.5">
            <label class="relative min-w-0 flex-1">
              <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
              <input bind:this={search} bind:value={query} type="search" class="ps-8" placeholder={t('searchFiles')} aria-label={t('search')} data-testid="files-search" />
            </label>
            {#if unlocked && folderScope}
              <Tooltip content={t('addFiles')}>
                {#snippet children(props)}
                  <button {...props} class={buttonClass('primary', 'md', 'size-8 px-0')} aria-label={t('addFiles')} onclick={() => add()} data-testid="files-add">
                    <Icon name="add" />
                  </button>
                {/snippet}
              </Tooltip>
              <Tooltip content={t('addFolder')}>
                {#snippet children(props)}
                  <button {...props} class={buttonClass('secondary', 'md', 'size-8 px-0')} aria-label={t('addFolder')} onclick={() => add(undefined, true)} data-testid="files-add-folder">
                    <Icon name="folder-add" />
                  </button>
                {/snippet}
              </Tooltip>
            {/if}
          </div>
          <div class="flex min-w-0 items-center gap-1 px-0.5 text-[11.5px] text-faint">
            <button class="truncate hover:text-ink" onclick={() => pick(ROOT)}>{t('library')}</button>
            {#each path as name, i (i)}
              <span>/</span><span class={cn('truncate', i === path.length - 1 && 'text-ink')}>{name}</span>
            {/each}
            {#if scope === 'starred'}<span>/</span><span class="text-ink">{t('starred')}</span>{/if}
          </div>
        </div>

        {#if selected.length > 1 && unlocked}
          <div class="mx-3 mb-2 flex items-center gap-2 rounded-xl border border-line bg-surface-2 px-2.5 py-1.5 text-[12px] text-ink-2" data-testid="files-selection">
            <span class="min-w-0 flex-1">{t('selectedCount', { n: selected.length })}</span>
            <button class={buttonClass('secondary', 'xs')} onclick={() => askBackup(selected, false)}><Icon name="cloud-up" />{t('backUp')}</button>
            <button class={buttonClass('secondary', 'xs')} onclick={() => (moving = selected)}><Icon name="folder" />{t('moveTo')}</button>
            <button class={buttonClass('ghost', 'xs', 'text-danger')} onclick={() => discard(selected)}><Icon name="trash" /></button>
          </div>
        {/if}

        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <ul bind:this={list} class="min-h-0 flex-1 overflow-y-auto px-2 pb-24" aria-label={t('files')} onkeydown={onListKey} data-testid="files-list">
          {#each rows as row (row.id)}
            <Row
              entry={row}
              folder={row.kind === 'folder'}
              selected={selected.includes(row.id)}
              trash={false}
              {unlocked}
              onSelect={(event) => select(row.id, event)}
              onOpenFolder={() => pick(row.id)}
              onOpen={() => open(row.id, null)}
              onExport={() => exportFile(row.id, null, row.name)}
              onBackup={() => askBackup(selected.includes(row.id) && selected.length > 1 ? selected : [row.id], false)}
              onShare={() => askShare(row.id, null, row.name)}
              onMove={() => (moving = selected.includes(row.id) && selected.length > 1 ? selected : [row.id])}
              onRename={() => ((renaming = { id: row.id, name: row.name }), (draft = row.name))}
              onStar={() => star(row)}
              onDiscard={() => discard(selected.includes(row.id) && selected.length > 1 ? selected : [row.id])}
              onPurge={() => purge([row.id])}
              dragIds={() => (selected.includes(row.id) ? selected : [row.id])}
            />
          {:else}
            {#if loaded}
              <li class="grid justify-items-center gap-2 px-4 py-10 text-center" data-testid="files-empty">
                {#if query}
                  <p class="text-[13px] text-muted [overflow-wrap:anywhere]">{t('noFilesMatch', { query })}</p>
                {:else if scope === 'starred'}
                  <p class="text-[13px] text-muted">{t('noItems')}</p>
                {:else}
                  <span class="grid size-11 place-items-center rounded-xl border border-dashed border-edge-soft text-faint [&_svg]:size-5"><Icon name="folder-open" /></span>
                  <b class="text-[13.5px] font-semibold">{scope === ROOT && !folders.length ? t('emptyLibrary') : t('emptyFolder')}</b>
                  <p class="text-[12.5px] text-muted">{t('emptyFolderHint')}</p>
                {/if}
              </li>
            {/if}
          {/each}
        </ul>
        <Jobs {names} onCancel={cancelBackup} />
      </section>

      <div class="min-w-0 flex-1">
        {#if current && details?.file?.id === current.id}
          {#key current.id}
            <Detail
              file={details.file}
              versions={details.versions}
              trash={false}
              {unlocked}
              onOpen={(version) => open(current.id, version)}
              onExport={(version) => exportFile(current.id, version, current.name)}
              onBackup={(all) => askBackup([current.id], all)}
              onShare={(version) => askShare(current.id, version, current.name)}
              links={links.filter((l) => l.file === current.id)}
              onLink={(version) => askLink(current.id, version, current.name)}
              onRevokeLink={revokeLink}
              onRefreshLinks={refreshLinks}
              onMove={() => (moving = [current.id])}
              onRename={() => ((renaming = { id: current.id, name: current.name }), (draft = current.name))}
              onDiscard={() => discard([current.id])}
              onDeleteCloud={(uploads) => (deletingCloud = uploads)}
            />
          {/key}
        {:else if loaded}
          <div class="grid h-full place-items-center p-8 text-center text-[13px] text-faint">
            <p>{selected.length > 1 ? t('selectedCount', { n: selected.length }) : rows.length ? t('selectFile') : error || ''}</p>
          </div>
        {/if}
      </div>
    {/if}
  {/if}

  {#if dragging}
    <div class="pointer-events-none absolute inset-2 z-20 grid place-items-center rounded-2xl border-2 border-dashed border-forest/60 bg-pale/85 text-forest backdrop-blur-[1px] animate-in fade-in-0">
      <div class="grid justify-items-center gap-2">
        <Icon name="add" class="size-7" />
        <b class="text-[14px] font-semibold">{t('dropToAdd')}</b>
        <span class="text-[12px] text-muted">{t('dropFilesDetail')}</span>
      </div>
    </div>
  {/if}
</div>

<Dialog bind:open={() => !!naming, (next) => !next && (naming = null)} title={t('newFolder')} icon="folder-add" testid="files-new-folder-dialog">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div onkeydown={(event) => event.key === 'Enter' && createFolder()}>
    <label class="grid gap-1 text-[12px] text-faint">{t('folderName')}<input bind:value={draft} class="text-[13.5px] text-ink" data-testid="files-folder-name" /></label>
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (naming = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !draft.trim()} onclick={createFolder} data-testid="files-folder-create">{t('add')}</button>
  {/snippet}
</Dialog>

<Dialog bind:open={() => !!renaming, (next) => !next && (renaming = null)} title={t('renameFile')} description={renaming?.name} icon="edit" testid="files-rename-dialog">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div onkeydown={(event) => event.key === 'Enter' && rename()}>
    <label class="grid gap-1 text-[12px] text-faint">{t('newName')}<input bind:value={draft} class="text-[13.5px] text-ink" data-testid="files-rename-input" /></label>
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (renaming = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !draft.trim() || draft.trim() === renaming?.name} onclick={rename} data-testid="files-rename-save">{t('save')}</button>
  {/snippet}
</Dialog>

<FolderPicker bind:open={() => !!moving, (next) => !next && (moving = null)} {folders} title={t('moveTo')} action={t('moveHere')} exclude={moving ?? []} onPick={(folder) => moving && move(moving, folder)} />

<Dialog bind:open={() => !!backing, (next) => !next && (backing = null)} title={t('backupTitle')} icon="cloud-up" testid="files-backup-dialog">
  {#if backing}
    <p class="text-[13.5px] leading-relaxed">{t('backupHint', { n: backing.count, size: size(backing.bytes) })}</p>
    <Notice tone={backing.usage?.storage_limit && backing.usage.reserved + backing.usage.committed + backing.bytes > backing.usage.storage_limit ? 'warn' : 'info'} class="mt-3" icon="sync">
      {backing.usage?.storage_limit ? t('cloudRoom', { used: size(backing.usage.reserved + backing.usage.committed), limit: size(backing.usage.storage_limit) }) : t('cloudRoomUnknown')}
    </Notice>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (backing = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy} onclick={backup} data-testid="files-backup-confirm"><Icon name="cloud-up" />{t('backUp')}</button>
  {/snippet}
</Dialog>

<Dialog bind:open={() => !!linking, (next) => !next && (linking = null)} title={t('linkTitle')} description={linking?.name} icon="link" testid="files-link-dialog">
  {#if linking?.result}
    <div class="grid gap-3">
      <p class="text-[13.5px] leading-relaxed">{t('linkReady')}</p>
      <div class="flex items-center gap-2 rounded-xl border border-line bg-surface-2 px-3 py-2">
        <code class="min-w-0 flex-1 truncate font-mono text-[12px] text-ink" data-testid="files-link-url">{linking.result.url}</code>
        <button class={buttonClass('secondary', 'xs')} onclick={() => copyText(linking?.result.url)} data-testid="files-link-copy"><Icon name="copy" />{t('linkCopy')}</button>
      </div>
      <Notice tone="info">{linking.result.password ? t('linkPasswordSent') : t('linkSendHint')}</Notice>
    </div>
  {:else if linking}
    <div class="grid gap-3">
      <Notice tone="info">{t('linkHint')}</Notice>
      <div class="grid gap-1 text-[12px] text-faint">
        {t('linkDays')}
        <Select bind:value={() => String(linking?.days ?? 7), (next) => linking && (linking.days = Number(next))} options={[{ value: '1', label: t('days1') }, { value: '7', label: t('days7') }, { value: '30', label: t('days30') }]} label={t('linkDays')} testid="files-link-days" class="w-full" />
      </div>
      <label class="grid gap-1 text-[12px] text-faint">
        {t('linkMaxDownloads')}
        <input type="number" min="0" max="1000000" bind:value={linking.maxDownloads} class="text-[13.5px] text-ink" data-testid="files-link-max" />
      </label>
      <label class="grid gap-1 text-[12px] text-faint">
        {t('linkPassword')}
        <input type="password" bind:value={linking.password} class="text-[13.5px] text-ink" autocomplete="off" data-testid="files-link-password" />
        <span class="text-[11.5px]">{t('linkPasswordHint')}</span>
      </label>
    </div>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    {#if linking?.result}
      <button class={buttonClass('primary')} onclick={() => (linking = null)}>{t('done')}</button>
    {:else}
      <button class={buttonClass('ghost')} onclick={() => (linking = null)}>{t('cancel')}</button>
      <button class={buttonClass('primary')} disabled={busy} onclick={createLink} data-testid="files-link-create"><Icon name="link" />{t('createLink')}</button>
    {/if}
  {/snippet}
</Dialog>

<Dialog bind:open={mounting} title={t('mountTitle')} icon="drive" testid="files-mount-dialog">
  <p class="text-[13.5px] leading-relaxed">{t('mountHint')}</p>
  {#if mount?.os === 'macos' || mount?.os === 'windows' || mount?.os === 'linux'}
    <Notice tone="warn" class="mt-3">{t(mount.os === 'macos' ? 'mountNoteMacos' : mount.os === 'windows' ? 'mountNoteWindows' : 'mountNoteLinux')}</Notice>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (mounting = false)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy} onclick={mountDisk} data-testid="files-mount-confirm"><Icon name="drive" />{t('mountDisk')}</button>
  {/snippet}
</Dialog>

<Dialog bind:open={() => !!sharing, (next) => !next && (sharing = null)} title={t('shareToChannel')} description={sharing?.name} icon="send" testid="files-share-dialog">
  {#if sharing}
    <div class="grid gap-3">
      <Notice tone="info">{t('shareHint')}</Notice>
      <div class="grid gap-1 text-[12px] text-faint">
        {t('chooseChannel')}
        <Select bind:value={sharing.channel} options={sharing.channels.map((c) => ({ value: c.id, label: c.name }))} placeholder={t('pickChannel')} label={t('chooseChannel')} testid="files-share-channel" class="w-full" />
      </div>
      <label class="grid gap-1 text-[12px] text-faint">{t('body')}<textarea bind:value={sharing.text} rows="3" class="text-[13.5px] text-ink"></textarea></label>
    </div>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (sharing = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !sharing?.channel} onclick={share} data-testid="files-share-send"><Icon name="send" class="rtl:-scale-x-100" />{t('share')}</button>
  {/snippet}
</Dialog>

<Dialog bind:open={() => !!deletingCloud, (next) => !next && (deletingCloud = null)} title={t('deleteCloudTitle', { n: deletingCloud?.length ?? 0 })} icon="cloud-off" tone="danger" testid="files-cloud-delete-dialog">
  <p class="text-[13.5px] leading-relaxed">{t('deleteCloudHint')}</p>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (deletingCloud = null)}>{t('cancel')}</button>
    <button class={buttonClass('danger')} disabled={busy} onclick={deleteCloud} data-testid="files-cloud-delete-confirm"><Icon name="cloud-off" />{t('deleteForever')}</button>
  {/snippet}
</Dialog>

<Dialog bind:open={() => !!changedCopy, (next) => !next && (changedCopy = null)} title={t('copyChanged', { name: changedCopy?.name ?? '' })} description={t('copyChangedHint')} icon="edit" tone="warn" testid="files-copy-changed">
  <p class="truncate font-mono text-[12px] text-faint" dir="ltr">{changedCopy?.path}</p>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} disabled={busy} onclick={() => settleCopy(false)}>{t('discardChange')}</button>
    <button class={buttonClass('primary')} disabled={busy} onclick={() => settleCopy(true)} data-testid="files-copy-save"><Icon name="history" />{t('saveAsVersion')}</button>
  {/snippet}
</Dialog>
