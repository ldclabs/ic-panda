<script module lang="ts">
  /** The cloud view reads the relay once per window by itself; later reads are asked for. */
  let cloudRead = false
</script>

<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { onMount, tick, untrack } from 'svelte'
  import { localeDir } from '../../shared/locales'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import Locked from '../components/page/Locked.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Select from '../components/Select.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from '../components/ui'
  import { locale, t } from '../i18n/index.svelte'
  import { copyText } from '../components/clipboard'
  import { bridge, call, closed, code, errorText, onChange, planAction, single, size } from '../rpc'
  import { added, backups, dismiss, failed, forget, jobs, picked, progress, type Job } from './files/adding.svelte'
  import Cloud from './files/Cloud.svelte'
  import Detail from './files/Detail.svelte'
  import { ancestorsOf, isSort, matchesFile, ordered, ROOT, sortKeys, statusOf, type Sort, type SortKey } from './files/files'
  import FolderDetail from './files/FolderDetail.svelte'
  import { focus } from './files/focus.svelte'
  import FolderPicker from './files/FolderPicker.svelte'
  import GrantDialog from './files/GrantDialog.svelte'
  import Jobs from './files/Jobs.svelte'
  import OpenCopies from './files/OpenCopies.svelte'
  import Row from './files/Row.svelte'
  import Trash from './files/Trash.svelte'
  import Tree from './files/Tree.svelte'

  /**
   * The file library: the folders, tags and places on the left, the current
   * folder in the middle, the selected file or folder on the right. Files
   * come in by the system dialog or a drop and are encrypted on this device;
   * backing up, sharing and deleting a cloud copy are explicit actions, each
   * confirmed, and no deletion on either side reaches the other. Each change
   * comes back through the daemon's `files/changed`.
   */
  let { status, mod, route, go }: { status: any; mod: string; route: string; go: (path: string) => void } = $props()

  /** Rows rendered at first and added each time the list's end comes into view. */
  const PAGE = 300
  const SORT_KEY = 'dmsg.files.sort'

  /** A folder ID, or `starred`, `tag:<tag>`, `cloud`, `trash`. */
  let scope = $state<string>(ROOT)
  let folders = $state<any[]>([])
  let tags = $state<{ tag: string; files: number }[]>([])
  let listing = $state<{ folder: any; folders: any[]; files: any[] } | null>(null)
  let found = $state<any[] | null>(null)
  let trashListing = $state<{ folders: any[]; files: any[] } | null>(null)
  let cloud = $state<any[]>([])
  let cloudLoaded = $state(false)
  let cloudUsage = $state<any>(null)
  let usage = $state<any>(null)
  let query = $state('')
  let selected = $state<string[]>([])
  /** Where a shift-extended selection starts. */
  let anchor = $state<string | null>(null)
  let details = $state<any>(null)
  let error = $state('')
  let loaded = $state(false)
  let busy = $state(false)
  let dragging = $state(0)
  let limit = $state(PAGE)
  let sort = $state<Sort>(readSort())
  let search = $state<HTMLInputElement | null>(null)
  let list = $state<HTMLElement | null>(null)
  let sentinel = $state<HTMLElement | null>(null)
  /** Dialogs: each holds what it acts on. */
  let naming = $state<{ parent: string } | null>(null)
  let renaming = $state<{ id: string; name: string } | null>(null)
  let moving = $state<string[] | null>(null)
  let fetching = $state<string | null>(null)
  let granting = $state<any>(null)
  let purging = $state<string[] | 'all' | null>(null)
  let copiesOpen = $state(false)
  let sharing = $state<{ id: string; version: string | null; name: string; channel: string; text: string; channels: any[] } | null>(null)
  let backing = $state<{ ids: string[]; allVersions: boolean; count: number; bytes: number; usage: any } | null>(null)
  let deletingCloud = $state<string[] | null>(null)
  let changedCopy = $state<any | null>(null)
  /** The trashed file shown beside the trash, and its details. */
  let trashOpened = $state<string | null>(null)
  let trashDetails = $state<any>(null)
  /** `files/mount_status`: whether the library is a disk right now, and whether it may be. */
  let mount = $state<any>(null)
  let mounting = $state(false)
  /** `files/share_links`: the links this device made, for the detail's list. */
  let links = $state<any[]>([])
  let linking = $state<{ id: string; version: string | null; name: string; days: number; maxDownloads: number; password: string; result: any | null } | null>(null)
  let draft = $state('')
  /** The names of files a backup card names by ID, read once each. */
  let resolved = $state<Record<string, string>>({})
  let cancelling = false

  const unlocked = $derived(status?.state === 'unlocked')
  const tag = $derived(scope.startsWith('tag:') ? scope.slice(4) : null)
  const folderScope = $derived(scope !== 'starred' && scope !== 'cloud' && scope !== 'trash' && !tag)
  const names = $derived(Object.fromEntries((listing?.files ?? []).concat(found ?? []).map((f: any) => [f.id, f.name])))
  const cloudNames = $derived(Object.fromEntries(cloud.map((e) => [e.upload_id, e.name])))
  /** Rows from across the library show where they lie. */
  const spread = $derived(!!found || !folderScope)
  const rows = $derived.by(() => {
    if (found) return ordered([], found.map((f) => ({ ...f, kind: 'file' })), locale(), sort)
    if (!listing) return []
    const files = listing.files.filter((f) => matchesFile(f, query)).map((f) => ({ ...f, kind: 'file' }))
    const subfolders = query.trim() ? [] : listing.folders.map((f) => ({ ...f, kind: 'folder' }))
    return ordered(subfolders, files, locale(), sort)
  })
  const shown = $derived(rows.slice(0, limit))
  const currentRow = $derived(selected.length === 1 ? (rows.find((r) => r.id === selected[0]) ?? null) : null)
  const current = $derived(currentRow?.kind === 'file' ? currentRow : null)
  const currentFolder = $derived(currentRow?.kind === 'folder' ? (folders.find((f) => f.id === currentRow.id) ?? currentRow) : null)
  /** The details follow the selected file's ID, not each new row object a reload or search makes. */
  const currentId = $derived<string | null>(current?.id ?? null)
  const ancestors = $derived(ancestorsOf(folders, folderScope ? scope : ROOT))
  /** The selection as a set: a folder of thousands, all selected, stays cheap. */
  const chosen = $derived(new Set(selected))
  const chosenFiles = $derived(rows.filter((r) => chosen.has(r.id) && r.kind === 'file'))
  /** What an export can write: a file only in the cloud has no bytes here. */
  const exportable = $derived(chosenFiles.filter((f) => statusOf(f.status) !== 'cloud_only'))

  function readSort(): Sort {
    try {
      const stored = JSON.parse(localStorage.getItem(SORT_KEY) ?? 'null')
      if (isSort(stored)) return stored
    } catch {
      /* none kept */
    }
    return { key: 'name', descending: false }
  }

  function setSort(next: Sort) {
    sort = next
    try {
      localStorage.setItem(SORT_KEY, JSON.stringify(next))
    } catch {
      /* kept for this window only */
    }
  }

  /** A failure for people; a name taken in the folder says so. */
  const failure = (e: unknown) => (code(e) === 'IDEMPOTENCY_CONFLICT' ? t('nameTaken') : errorText(e))
  const fail = (e: unknown) => toast(() => failure(e), planAction(e))

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
    anchor = null
    trashOpened = null
    if (next === 'cloud') go('/files/cloud')
    else if (next === 'trash') go('/files/trash')
    else if (route !== '/files') go('/files')
  }

  // A vault item's attachment opens here: its folder, with the file selected.
  $effect(() => {
    const id = focus.id
    if (!id || !loaded) return
    focus.id = null
    void call('files/show', { id })
      .then((details: any) => {
        pick(details.file.folder)
        selected = [id]
        anchor = id
      })
      .catch((e) => toast(() => errorText(e)))
  })

  async function showDetails(id: string | null) {
    const next = id ? await call('files/show', { id }).catch(() => null) : null
    if (currentId === id) details = next
  }

  async function showTrashed(id: string | null) {
    const next = id ? await call('files/show', { id }).catch(() => null) : null
    if (trashOpened === id) trashDetails = next
  }

  /** The place shown and the library around it, read together. */
  const reload = single(async () => {
    const place = scope
    try {
      const [tree, shown, trashed, cloudList, cloudRoom, local, disk, shares, running, tagList] = await Promise.all([
        call('files/tree'),
        folderScope ? call('files/list', { folder: place }) : place === 'starred' ? call<any[]>('files/search', { query: '', starred: true }) : tag ? call<any[]>('files/search', { query: '', tag }) : null,
        place === 'trash' ? call('files/list', { trash: true }) : null,
        place === 'cloud' ? call('files/cloud_list') : null,
        place === 'cloud' ? call('files/cloud_usage').catch(() => null) : null,
        call('files/usage').catch(() => null),
        call('files/mount_status').catch(() => null),
        call('files/share_links').catch(() => []),
        call('files/backup_status').catch(() => []),
        call('files/tags').catch(() => [])
      ])
      folders = tree
      tags = tagList
      listing = Array.isArray(shown) ? { folder: null, folders: [], files: shown } : shown
      if (trashed) trashListing = trashed
      if (cloudList) {
        cloud = cloudList
        cloudUsage = cloudRoom ?? cloudUsage
        cloudLoaded = true
        if (!cloudRead) {
          cloudRead = true
          void call('files/cloud_refresh').catch(() => {})
        }
      }
      usage = local
      mount = disk
      links = shares
      backups(running)
      error = ''
    } catch (e) {
      // A folder trashed elsewhere is gone from here too.
      if (code(e) === 'NOT_FOUND' && folderScope && place !== ROOT) {
        loaded = true
        return pick(ROOT)
      }
      listing = null
      folders = []
      error = closed(e, status) ? 'LOCKED' : errorText(e)
    }
    loaded = true
    const present = new Set(rows.map((r) => r.id))
    selected = selected.filter((id) => present.has(id))
    if (trashOpened && !trashListing?.files.some((f) => f.id === trashOpened)) trashOpened = null
    await Promise.all([showDetails(currentId), showTrashed(trashOpened)])
  })

  // Read untracked: the reload's own reads are not this effect's to follow.
  $effect(() => {
    void scope
    untrack(() => void reload())
  })

  // A new place or search starts at the top of its rows.
  $effect(() => {
    void scope
    void query
    limit = PAGE
  })

  // More rows once the list's end comes into view.
  $effect(() => {
    if (!sentinel || !list) return
    const observer = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && (limit += PAGE), { root: list })
    observer.observe(sentinel)
    return () => observer.disconnect()
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
  $effect(() => void showTrashed(trashOpened))

  // A backup card names its file once the file's name is read.
  $effect(() => {
    for (const job of jobs) {
      if (job.kind !== 'backup' || names[job.name] || job.name in untrack(() => resolved)) continue
      const id = job.name
      resolved[id] = ''
      void call<any>('files/show', { id })
        .then((d) => (resolved[id] = d.file.name))
        .catch(() => {})
    }
  })

  const nameOf = (job: Job) =>
    job.kind === 'add' ? job.name : job.kind === 'backup' ? names[job.name] || resolved[job.name] || `${job.name.slice(0, 12)}…` : cloudNames[job.name] || `${job.name.slice(0, 12)}…`

  function select(id: string, event?: MouseEvent) {
    // A right-click inside the selection keeps it, for the row's menu to act on.
    if (event?.type === 'contextmenu') {
      if (!chosen.has(id)) {
        selected = [id]
        anchor = id
      }
      return
    }
    if (event && (event.metaKey || event.ctrlKey)) selected = chosen.has(id) ? selected.filter((s) => s !== id) : [...selected, id]
    else if (event?.shiftKey && anchor) selected = range(anchor, id)
    else selected = [id]
    if (!event?.shiftKey) anchor = id
  }

  function range(from: string, to: string): string[] {
    const ids = rows.map((r) => r.id)
    const [a, b] = [ids.indexOf(from), ids.indexOf(to)].sort((x, y) => x - y)
    return a < 0 ? [to] : ids.slice(a, b + 1)
  }

  /** Arrows move the selection, with Shift they extend it; the rows load as far as it goes. */
  function onListKey(event: KeyboardEvent) {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
    event.preventDefault()
    const lead = selected.at(-1)
    const at = rows.findIndex((r) => r.id === lead)
    const next = rows[Math.max(0, Math.min(rows.length - 1, at + (event.key === 'ArrowDown' ? 1 : -1)))]
    if (!next) return
    if (event.shiftKey && anchor) selected = [...range(anchor, next.id).filter((id) => id !== next.id), next.id]
    else {
      selected = [next.id]
      anchor = next.id
    }
    if (rows.indexOf(next) >= limit) limit = rows.indexOf(next) + PAGE
    void tick().then(() => list?.querySelector<HTMLElement>(`[data-id="${next.id}"]`)?.focus())
  }

  const parentOf = (id: string) => folders.find((f) => f.id === id)?.parent ?? ROOT

  /**
   * ⌘F focuses the search, ⌘N makes a folder, ⌘A selects every row, ⌘↑
   * goes up a folder and ⌘⌫ trashes the selection (Ctrl elsewhere). In the
   * list, Enter opens, F2 renames, Delete trashes and Escape clears the
   * selection; typing in a field or a dialog keeps its keys.
   */
  function onKey(event: KeyboardEvent) {
    const target = event.target instanceof HTMLElement ? event.target : null
    const typing = !!target && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))
    const overlay = !!document.querySelector('[role="dialog"], [role="alertdialog"], [role="menu"]')
    const inList = !target || target === document.body || !!target.closest('[data-testid="files-list"]')
    const held = mod === '⌘' ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key
    if (held && !event.altKey && !event.shiftKey) {
      if (key === 'f') {
        event.preventDefault()
        search?.focus()
        search?.select()
      } else if (key === 'n' && unlocked && folderScope && !overlay) {
        event.preventDefault()
        newFolderIn(scope)
      } else if (key === 'a' && inList && !typing && !overlay && rows.length) {
        event.preventDefault()
        selected = rows.map((r) => r.id)
      } else if (key === 'ArrowUp' && folderScope && scope !== ROOT && !typing && !overlay) {
        event.preventDefault()
        pick(parentOf(scope))
      } else if (key === 'Backspace' && inList && !typing && !overlay && selected.length && unlocked && scope !== 'trash' && scope !== 'cloud') {
        event.preventDefault()
        trash(selected)
      }
      return
    }
    if (typing || overlay || held || event.altKey || !inList || !unlocked || scope === 'cloud' || scope === 'trash') return
    if (key === 'Delete' && selected.length) {
      event.preventDefault()
      trash(selected)
    } else if (key === 'Enter' && currentRow) {
      event.preventDefault()
      if (currentRow.kind === 'folder') pick(currentRow.id)
      else if (statusOf(currentRow.status) !== 'cloud_only') open(currentRow.id, null)
    } else if (key === 'F2' && currentRow) {
      event.preventDefault()
      renameItem(currentRow)
    } else if (key === 'Escape' && selected.length) {
      selected = []
    }
  }

  /**
   * Files reach the daemon through the main process: a dialog, or the paths
   * of what was dropped, into the folder shown or the one dropped on. The
   * main process asks first when a name is taken there.
   */
  async function add(dropped?: File[], directory = false, target?: string) {
    const folder = target ?? (folderScope ? scope : ROOT)
    try {
      const result: any = await bridge(() => window.dmsg.filesAdd(folder, dropped, directory))
      if (!result && dropped) forget(dropped.map((f) => f.name))
      if (result) {
        added(result.names, result.files, { duplicate: (n) => t('duplicateOf', { n }), version: t('addedAsVersion'), files: (n) => t('filesAddedCount', { n }) })
        // A single file opens once the list holds it.
        if (!target && result.files.length === 1 && result.files[0].file && result.names.includes(result.files[0].file.name)) {
          await reload()
          selected = [result.files[0].file.id]
        }
      }
    } catch (e) {
      failed(code(e), cancelling)
    } finally {
      cancelling = false
    }
  }

  /** The import under way stops at its next chunk; what it added so far stays. */
  async function cancelImport() {
    cancelling = true
    for (const job of jobs) if (job.kind === 'add' && job.stage === 'running' && job.cancellable) job.cancelling = true
    await call('files/import_cancel').catch(fail)
  }

  const isFiles = (event: DragEvent) => !!event.dataTransfer?.types.includes('Files')
  function onDragEnter(event: DragEvent) {
    if (!isFiles(event) || !unlocked || !folderScope) return
    event.preventDefault()
    dragging++
  }
  function onDragOver(event: DragEvent) {
    if (!isFiles(event) || !unlocked || !folderScope) return
    event.preventDefault()
    event.dataTransfer!.dropEffect = 'copy'
  }
  function onDrop(event: DragEvent) {
    if (!isFiles(event)) return
    event.preventDefault()
    dragging = 0
    if (!unlocked || !folderScope) return
    dropFiles(Array.from(event.dataTransfer?.files ?? []))
  }
  /** Files dropped on the view, or on a folder of the list or the tree. */
  function dropFiles(dropped: File[], target?: string) {
    dragging = 0
    if (!dropped.length) return
    // Named here too: a drop the preload refuses never reaches the main process's `files/adding`.
    picked(dropped.map((f) => f.name))
    void add(dropped, false, target)
  }

  const open = (id: string, version: string | null) => attempt(() => bridge(() => window.dmsg.filesOpen(id, version)))
  const exportFile = (id: string, version: string | null, name: string) => attempt(() => bridge(() => window.dmsg.filesExport(id, version, name)))
  const exportMany = (items: any[]) =>
    attempt(async () => {
      const result: any = await bridge(() => window.dmsg.filesExportMany(items.map((f) => ({ id: f.id, name: f.name }))))
      if (result) toast(() => t('exported', { n: result.count, dir: result.dir }))
    })
  const star = (file: any) => attempt(() => call('files/update', { id: file.id, starred: !file.starred }))
  const starMany = (files: any[]) =>
    attempt(async () => {
      const starred = !files.every((f) => f.starred)
      for (const file of files) await call('files/update', { id: file.id, starred })
    })
  const restore = (ids: string[]) => attempt(() => call('files/restore', { ids }), t('restored'))
  /** To the trash, with a way back while the notice shows. */
  const trash = (ids: string[]) =>
    attempt(async () => {
      await call('files/trash', { ids })
      if (ids.includes(scope)) pick(parentOf(scope))
      toast(() => t('movedToTrash'), { ms: 6000, action: { label: t('undo'), run: () => restore(ids) }, testid: 'files-trashed' })
    })
  const purge = () =>
    attempt(async () => {
      if (!purging) return
      await call('files/purge', purging === 'all' ? {} : { ids: purging })
      purging = null
    })
  const move = (ids: string[], folder: string) => attempt(() => call('files/move', { ids, folder }), t('moved'))
  const reveal = (file: any) => {
    pick(file.folder)
    selected = [file.id]
    anchor = file.id
  }
  const newFolderIn = (parent: string) => {
    naming = { parent }
    draft = ''
  }
  const renameItem = (item: any) => {
    renaming = { id: item.id, name: item.name }
    draft = item.name
  }
  function folderAction(action: 'rename' | 'backup' | 'grant' | 'move' | 'trash', folder: any) {
    if (action === 'rename') renameItem(folder)
    else if (action === 'backup') void askBackup([folder.id], false)
    else if (action === 'grant') granting = folder
    else if (action === 'move') moving = [folder.id]
    else trash([folder.id])
  }

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

  /**
   * Backing up shows what goes and the cloud's room first; nothing is
   * uploaded before the confirmation. A folder counts the files under it
   * not yet in the cloud; `known` gives a single file's versions.
   */
  async function askBackup(ids: string[], allVersions: boolean, known?: { count: number; bytes: number }) {
    let count = known?.count ?? 0
    let bytes = known?.bytes ?? 0
    if (!known) {
      const wanted = new Set(ids)
      const inside = await Promise.all(folders.filter((f) => wanted.has(f.id)).map((f) => call<any[]>('files/search', { query: '', folder: f.id }).catch(() => [])))
      const all = new Map([...rows.filter((r) => wanted.has(r.id) && r.kind === 'file'), ...inside.flat()].map((f) => [f.id, f]))
      const pending = [...all.values()].filter((f) => ['local', 'failed'].includes(statusOf(f.status)))
      count = pending.length
      bytes = pending.reduce((n, f) => n + (f.size ?? 0), 0)
    }
    if (!count) return toast(() => t('nothingToBackUp'))
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
  /** The whole library: every chunk file, then every local version through its key. */
  const verifyAll = () =>
    attempt(async () => {
      const report: any = await call('files/verify')
      const bad = report.versions.filter((v: any) => !v.ok).length + report.blobs_bad.length
      const counts = { versions: report.versions.length, blobs: report.blobs_checked, n: bad }
      toast(() => t(bad ? 'verifyAllBad' : 'verifyAllDone', counts), { ms: 8000, testid: 'files-verified' })
    })
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
      if (event.type === 'files/adding') picked(Array.isArray(data.names) ? data.names : [], Array.isArray(data.folders) ? data.folders : [])
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

{#snippet sortMenu()}
  <DropdownMenu.Root dir={localeDir(locale())}>
    <Tooltip content={t('sortBy')}>
      {#snippet children(props)}
        <DropdownMenu.Trigger {...props} class={buttonClass('ghost', 'xs', 'ms-auto h-6 shrink-0 gap-1 px-1.5 text-faint')} aria-label={t('sortBy')} data-testid="files-sort">
          <Icon name="arrows" />{t(sort.key === 'name' ? 'name' : sort.key === 'modified' ? 'modified' : sort.key === 'size' ? 'sortSize' : 'fileType')}
        </DropdownMenu.Trigger>
      {/snippet}
    </Tooltip>
    <DropdownMenu.Portal>
      <DropdownMenu.Content align="end" sideOffset={6} class={cn(menuContentClass, 'min-w-44')} data-testid="files-sort-menu">
        {#each sortKeys as key (key)}
          <DropdownMenu.Item class={menuItemClass} onSelect={() => setSort({ key: key as SortKey, descending: sort.descending })}>
            <span>{t(key === 'name' ? 'name' : key === 'modified' ? 'modified' : key === 'size' ? 'sortSize' : 'fileType')}</span>
            {#if sort.key === key}<Icon name="check" class="size-3.5 text-forest" />{/if}
          </DropdownMenu.Item>
        {/each}
        <DropdownMenu.Separator class="my-1 h-px bg-line" />
        {#each [false, true] as descending (descending)}
          <DropdownMenu.Item class={menuItemClass} onSelect={() => setSort({ key: sort.key, descending })}>
            <span>{descending ? t('descending') : t('ascending')}</span>
            {#if sort.descending === descending}<Icon name="check" class="size-3.5 text-forest" />{/if}
          </DropdownMenu.Item>
        {/each}
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="@container relative flex h-full min-h-0"
  ondragenter={onDragEnter}
  ondragover={onDragOver}
  ondragleave={(event) => isFiles(event) && (dragging = Math.max(0, dragging - 1))}
  ondrop={onDrop}
  data-testid="files"
>
  {#if error === 'LOCKED'}
    <Locked {status} testid="files-locked" />
  {:else}
    <aside class="flex w-[220px] shrink-0 flex-col border-e border-line">
      <Tree {folders} {tags} {scope} {unlocked} onPick={pick} onDropMove={move} onDropFiles={dropFiles} onNewFolder={newFolderIn} onFolderAction={folderAction} />
      {#if usage}
        <div class="border-t border-line px-3.5 py-2 text-[11px] text-faint">
          <p>{t('localUsage', { files: usage.files, versions: usage.versions, bytes: size(usage.blob_bytes) })}</p>
          {#if unlocked}
            <button class="mt-0.5 text-forest hover:underline disabled:opacity-50" disabled={busy} onclick={verifyAll} data-testid="files-verify-all">{t('verifyAll')}</button>
          {/if}
        </div>
        {#if usage.last_check && usage.last_check.blobs_bad.length + usage.last_check.versions_bad.length > 0}
          <p class="flex items-start gap-1.5 px-3.5 pb-2 text-[11px] text-warn" data-testid="files-check-warning"><Icon name="warning" class="mt-px size-3.5 shrink-0" />{t('selfCheckDamage', { n: usage.last_check.blobs_bad.length + usage.last_check.versions_bad.length })}</p>
        {/if}
        {#if usage.open_copies > 0 && unlocked}
          <button class="flex items-center gap-1.5 border-t border-line px-3.5 py-2 text-start text-[11px] text-warn hover:underline" onclick={() => (copiesOpen = true)} data-testid="files-open-copies-open">
            <Icon name="external" class="size-3.5 shrink-0" />{t('openCopies', { n: usage.open_copies })}
          </button>
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
            <p>{t('mountOff')} <button class="text-forest hover:underline" onclick={() => go('/settings/files')} data-testid="files-mount-settings">{t('mountTurnOn')}</button></p>
          {/if}
        </div>
      {/if}
    </aside>

    {#if scope === 'cloud'}
      <div class="min-w-0 flex-1">
        <Cloud
          entries={cloud}
          usage={cloudUsage}
          {unlocked}
          {busy}
          loaded={cloudLoaded}
          onRefresh={refreshCloud}
          onFetch={(upload) => fetchBackup(upload, null)}
          onFetchTo={(upload) => (fetching = upload)}
          onVerify={verifyBackup}
          onDelete={(uploads) => (deletingCloud = uploads)}
        />
      </div>
    {:else if scope === 'trash'}
      <div class="min-w-0 flex-1">
        <Trash
          listing={trashListing}
          days={usage?.trash_days ?? 30}
          {unlocked}
          {busy}
          opened={trashOpened}
          onOpen={(id) => (trashOpened = id)}
          onRestore={restore}
          onPurge={(ids) => (purging = ids)}
          onEmpty={() => (purging = 'all')}
        />
      </div>
      {#if trashOpened && trashDetails?.file?.id === trashOpened}
        <div class="w-[380px] shrink-0 border-s border-line">
          {#key trashOpened}
            <Detail
              file={trashDetails.file}
              versions={trashDetails.versions}
              trash={true}
              {unlocked}
              device={status?.device_id ?? ''}
              onOpen={() => {}}
              onExport={() => {}}
              onBackup={() => {}}
              onShare={() => {}}
              links={[]}
              onLink={() => {}}
              onRevokeLink={() => {}}
              onRefreshLinks={() => {}}
              onMove={() => {}}
              onRename={() => {}}
              onReveal={() => {}}
              onDiscard={() => restore([trashDetails.file.id])}
              onDeleteCloud={() => {}}
            />
          {/key}
        </div>
      {/if}
    {:else}
      <section class="relative flex w-[280px] shrink-0 flex-col border-e border-line @min-[980px]:w-[320px]">
        <div class="grid gap-2 p-3 pb-2">
          <div class="flex items-center gap-1.5">
            <label class="relative min-w-0 flex-1">
              <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
              <input bind:this={search} bind:value={query} type="search" class="ps-8" placeholder={t('searchFiles')} aria-label={t('search')} data-testid="files-search" />
            </label>
            {#if unlocked && folderScope}
              <DropdownMenu.Root dir={localeDir(locale())}>
                <Tooltip content={t('addMenu')}>
                  {#snippet children(props)}
                    <DropdownMenu.Trigger {...props} class={buttonClass('primary', 'md', 'size-8 px-0')} aria-label={t('addMenu')} data-testid="files-add-menu">
                      <Icon name="add" />
                    </DropdownMenu.Trigger>
                  {/snippet}
                </Tooltip>
                <DropdownMenu.Portal>
                  <DropdownMenu.Content align="end" sideOffset={6} class={cn(menuContentClass, 'min-w-48')}>
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => add()} data-testid="files-add">
                      <span class="flex items-center gap-2"><Icon name="file" class="text-faint" />{t('addFiles')}</span>
                    </DropdownMenu.Item>
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => add(undefined, true)} data-testid="files-add-folder">
                      <span class="flex items-center gap-2"><Icon name="folder" class="text-faint" />{t('addFolder')}</span>
                    </DropdownMenu.Item>
                    <DropdownMenu.Separator class="my-1 h-px bg-line" />
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => newFolderIn(scope)} data-testid="files-new-folder-menu">
                      <span class="flex items-center gap-2"><Icon name="folder-add" class="text-faint" />{t('newFolder')}</span>
                      <kbd class="font-sans text-[11px] text-faint">{mod}N</kbd>
                    </DropdownMenu.Item>
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              </DropdownMenu.Root>
            {/if}
          </div>
          <div class="flex min-w-0 items-center gap-1 px-0.5 text-[11.5px] text-faint" data-testid="files-path">
            <button class="shrink-0 truncate hover:text-ink" onclick={() => pick(ROOT)}>{t('library')}</button>
            {#if folderScope}
              {#each ancestors as folder, i (folder.id)}
                <span>/</span>
                {#if i === ancestors.length - 1}
                  <span class="truncate text-ink">{folder.name}</span>
                {:else}
                  <button class="truncate hover:text-ink" onclick={() => pick(folder.id)}>{folder.name}</button>
                {/if}
              {/each}
            {:else if scope === 'starred'}
              <span>/</span><span class="truncate text-ink">{t('starred')}</span>
            {:else if tag}
              <span>/</span><span class="truncate text-ink">#{tag}</span>
            {/if}
            {@render sortMenu()}
          </div>
        </div>

        {#if selected.length > 1 && unlocked}
          <div class="mx-3 mb-2 flex flex-wrap items-center gap-1.5 rounded-xl border border-line bg-surface-2 px-2.5 py-1.5 text-[12px] text-ink-2" data-testid="files-selection">
            <span class="min-w-0 flex-1">{t('selectedCount', { n: selected.length })}</span>
            <Tooltip content={t('backUp')}>
              {#snippet children(props)}
                <button {...props} class={buttonClass('secondary', 'xs', 'size-6 px-0')} aria-label={t('backUp')} onclick={() => askBackup(selected, false)}><Icon name="cloud-up" /></button>
              {/snippet}
            </Tooltip>
            <Tooltip content={t('moveTo')}>
              {#snippet children(props)}
                <button {...props} class={buttonClass('secondary', 'xs', 'size-6 px-0')} aria-label={t('moveTo')} onclick={() => (moving = selected)}><Icon name="folder" /></button>
              {/snippet}
            </Tooltip>
            <Tooltip content={t('exportFile')}>
              {#snippet children(props)}
                <button {...props} class={buttonClass('secondary', 'xs', 'size-6 px-0')} aria-label={t('exportFile')} disabled={!exportable.length} onclick={() => exportMany(exportable)} data-testid="files-export-many"><Icon name="download" /></button>
              {/snippet}
            </Tooltip>
            <Tooltip content={chosenFiles.every((f) => f.starred) ? t('unstar') : t('star')}>
              {#snippet children(props)}
                <button {...props} class={buttonClass('secondary', 'xs', 'size-6 px-0')} aria-label={t('star')} disabled={!chosenFiles.length} onclick={() => starMany(chosenFiles)}><Icon name="star" /></button>
              {/snippet}
            </Tooltip>
            <Tooltip content={t('moveToTrash')}>
              {#snippet children(props)}
                <button {...props} class={buttonClass('ghost', 'xs', 'size-6 px-0 text-danger')} aria-label={t('moveToTrash')} onclick={() => trash(selected)}><Icon name="trash" /></button>
              {/snippet}
            </Tooltip>
            <Tooltip content={t('clearSelection')}>
              {#snippet children(props)}
                <button {...props} class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('clearSelection')} onclick={() => (selected = [])} data-testid="files-selection-clear"><Icon name="close" /></button>
              {/snippet}
            </Tooltip>
          </div>
        {/if}

        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <ul bind:this={list} class="min-h-0 flex-1 overflow-y-auto px-2 pb-24" aria-label={t('files')} onkeydown={onListKey} data-testid="files-list">
          {#each shown as row (row.id)}
            <Row
              entry={row}
              folder={row.kind === 'folder'}
              selected={chosen.has(row.id)}
              {unlocked}
              showPath={spread}
              onSelect={(event) => select(row.id, event)}
              onOpenFolder={() => pick(row.id)}
              onOpen={() => open(row.id, null)}
              onExport={() => exportFile(row.id, null, row.name)}
              onBackup={() => askBackup(chosen.has(row.id) && selected.length > 1 ? selected : [row.id], false)}
              onShare={() => askShare(row.id, null, row.name)}
              onMove={() => (moving = chosen.has(row.id) && selected.length > 1 ? selected : [row.id])}
              onRename={() => renameItem(row)}
              onStar={() => star(row)}
              onGrant={() => (granting = folders.find((f) => f.id === row.id) ?? row)}
              onNewFolder={() => newFolderIn(row.id)}
              onDiscard={() => trash(chosen.has(row.id) && selected.length > 1 ? selected : [row.id])}
              onDropMove={(ids) => move(ids, row.id)}
              onDropFiles={(dropped) => dropFiles(dropped, row.id)}
              dragIds={() => (chosen.has(row.id) ? selected : [row.id])}
            />
          {:else}
            {#if loaded}
              <li class="grid justify-items-center gap-2 px-4 py-10 text-center" data-testid="files-empty">
                {#if query}
                  <p class="text-[13px] text-muted [overflow-wrap:anywhere]">{t('noFilesMatch', { query })}</p>
                {:else if !folderScope}
                  <p class="text-[13px] text-muted">{t('noItems')}</p>
                {:else}
                  <span class="grid size-11 place-items-center rounded-xl border border-dashed border-edge-soft text-faint [&_svg]:size-5"><Icon name="folder-open" /></span>
                  <b class="text-[13.5px] font-semibold">{scope === ROOT && !folders.length ? t('emptyLibrary') : t('emptyFolder')}</b>
                  <p class="text-[12.5px] text-muted">{t('emptyFolderHint')}</p>
                {/if}
              </li>
            {:else}
              <!-- The daemon may answer late, busy with other work: never an empty library before it does. -->
              <li class="px-4 py-10 text-center" data-testid="files-loading"><p class="shimmer text-[13px] text-muted">{t('loading')}</p></li>
            {/if}
          {/each}
          {#if rows.length > shown.length}
            <li bind:this={sentinel} class="py-3 text-center text-[11.5px] text-faint" data-testid="files-more-rows">{t('moreRows', { n: rows.length - shown.length })}</li>
          {/if}
        </ul>
        <Jobs {nameOf} onCancel={cancelBackup} onCancelImport={cancelImport} />
      </section>

      <div class="min-w-0 flex-1">
        {#if current && details?.file?.id === current.id}
          {#key current.id}
            <Detail
              file={details.file}
              versions={details.versions}
              trash={false}
              {unlocked}
              device={status?.device_id ?? ''}
              onOpen={(version) => open(current.id, version)}
              onExport={(version) => exportFile(current.id, version, current.name)}
              onBackup={(all) => {
                const local = details.versions.filter((v: any) => v.status === 'local')
                void askBackup([current.id], all, all ? { count: local.length, bytes: local.reduce((n: number, v: any) => n + v.size, 0) } : undefined)
              }}
              onShare={(version) => askShare(current.id, version, current.name)}
              links={links.filter((l) => l.file === current.id)}
              onLink={(version) => askLink(current.id, version, current.name)}
              onRevokeLink={revokeLink}
              onRefreshLinks={refreshLinks}
              onMove={() => (moving = [current.id])}
              onRename={() => renameItem(current)}
              onReveal={() => reveal(details.file)}
              onDiscard={() => trash([current.id])}
              onDeleteCloud={(uploads) => (deletingCloud = uploads)}
            />
          {/key}
        {:else if currentFolder}
          <FolderDetail
            folder={currentFolder}
            {folders}
            {unlocked}
            onOpen={() => pick(currentFolder.id)}
            onBackup={() => askBackup([currentFolder.id], false)}
            onGrant={() => (granting = currentFolder)}
            onRename={() => renameItem(currentFolder)}
            onMove={() => (moving = [currentFolder.id])}
            onDiscard={() => trash([currentFolder.id])}
          />
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

<FolderPicker bind:open={() => !!fetching, (next) => !next && (fetching = null)} {folders} title={t('fetchTo')} action={t('fetchHere')} onPick={(folder) => fetching && fetchBackup(fetching, folder)} />

<GrantDialog bind:folder={granting} {go} />

<OpenCopies bind:open={copiesOpen} />

<Dialog
  bind:open={() => !!purging, (next) => !next && (purging = null)}
  title={purging === 'all' ? t('purgeAllTitle') : t('purgeTitle', { n: purging?.length ?? 0 })}
  icon="trash"
  tone="danger"
  testid="files-purge-dialog"
>
  <p class="text-[13.5px] leading-relaxed">{t('purgeHint')}</p>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (purging = null)}>{t('cancel')}</button>
    <button class={buttonClass('danger')} disabled={busy} onclick={purge} data-testid="files-purge-confirm"><Icon name="trash" />{t('deleteForever')}</button>
  {/snippet}
</Dialog>

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
