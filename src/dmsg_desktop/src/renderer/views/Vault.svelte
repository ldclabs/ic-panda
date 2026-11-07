<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import { ContextMenu } from 'bits-ui'
  import { copyText } from '../components/clipboard'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import Locked from '../components/page/Locked.svelte'
  import Page from '../components/page/Page.svelte'
  import PaneHandle from '../components/PaneHandle.svelte'
  import Select from '../components/Select.svelte'
  import TagInput from '../components/TagInput.svelte'
  import TargetMenu from '../components/TargetMenu.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { iconButtonClass, buttonClass, cn, menuItemClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, closed, errorText, onChange, single } from '../rpc'
  import { remember, remembered, rememberedPlain, rememberPlain } from '../view-memory'
  import Workflows from './secrets/Workflows.svelte'
  import { workflowText } from './secrets/workflow-text'
  import Conflict from './vault/Conflict.svelte'
  import AgentAccess from './agents/AgentAccess.svelte'
  import Filters from './vault/Filters.svelte'
  import { focus } from './vault/focus.svelte'
  import Import from './vault/Import.svelte'
  import Inspector from './vault/Inspector.svelte'
  import NewItem from './vault/NewItem.svelte'
  import Row from './vault/Row.svelte'
  import {
    facets as count,
    findingsByItem,
    healthKey,
    inScope,
    isManaged,
    matches,
    otpField,
    sorted,
    sorts,
    typeLabel,
    types,
    valueOf,
    workflows,
    type Scope,
    type Sort,
    type Templates
  } from './vault/vault'

  /**
   * The vault: filters, the list and the open item side by side, or the part
   * of what secrets are used for that the route names. New items, imports and
   * conflict copies open in dialogs; ⌘-click and Shift-click choose many items
   * to act on together. Files have their own library and items only name them.
   * Each change reaches the list through the daemon's `vault/changed`.
   */
  let { status, mod, route = '/vault', go }: { status: any; mod: string; route?: string; go: (path: string) => void } = $props()

  const SORT_KEY = 'dmsg.vault.sort'
  /** Rows rendered at first and added each time the list's end comes into view. */
  const PAGE = 200

  let entries = $state<any[]>([])
  let trashed = $state<any[]>([])
  let conflicts = $state<any[]>([])
  let findings = $state<any[]>([])
  let templates = $state<Templates | null>(null)
  let scope = $state<Scope>(rememberedPlain<Scope>('vault.tag') ?? remembered<Scope>('vault.scope') ?? 'all')
  /** The tag whose Agent access is open. */
  let accessTag = $state<string | null>(null)
  let query = $state('')
  let sort = $state<Sort>(readSort())
  let selected = $state<string | null>(remembered('vault.selected') ?? null)
  let editing = $state(false)
  let dirty = $state(false)
  let leaving = $state<string | null>(null)
  let error = $state('')
  let loaded = $state(false)
  let trashLoaded = $state(false)
  let healthStatus = $state<'loading' | 'ready' | 'error'>('loading')
  let creating = $state(false)
  let importing = $state(false)
  let comparing = $state(false)
  let compareFrom = $state(0)
  let chosen = $state<Set<string>>(new Set())
  let limit = $state(PAGE)
  let sentinel = $state<HTMLElement | null>(null)
  /** Where Shift+arrow has extended the choice to. */
  let cursor: string | null = null
  let tagging = $state<'add' | 'remove' | null>(null)
  let batchTags = $state<string[]>([])
  let generation = 0
  let alive = true
  let contentKey = ''
  let checkedKey = ''
  let search = $state<HTMLInputElement | null>(null)
  let list = $state<HTMLElement | null>(null)

  const unlocked = $derived(status?.state === 'unlocked')
  const sessionKey = $derived(`${status?.account ?? ''}:${status?.state ?? ''}`)
  const trash = $derived(scope === 'trash')
  const facets = $derived(count(entries))
  const conflicted = $derived(new Set(conflicts.map((c) => c.copy.id)))
  const issues = $derived(findingsByItem(findings))
  const flagged = $derived(new Set(issues.keys()))
  const shown = $derived(
    sorted(
      (trash ? trashed : entries).filter(
        (entry) => (editing && entry.id === selected) || (inScope(entry, scope, flagged) && matches(entry.item, query))
      ),
      sort,
      trash
    )
  )
  const listed = $derived(shown.slice(0, limit))
  const current = $derived(shown.find((entry) => entry.id === selected) ?? null)
  const workflow = $derived.by(() => {
    const part = route.split('?')[0].split('/')[2]
    return workflows.includes(part) ? part : null
  })
  /** What the actions on many reach: dMsg's own items stay out of them. */
  const picked = $derived(entries.filter((e) => chosen.has(e.id) && !isManaged(e.item)).map((e) => e.id))
  const scopes = $derived([
    { value: 'all', label: t('allItems') },
    { value: 'starred', label: t('starred') },
    { value: 'health', label: t('securityCheck') },
    ...types.filter((type) => facets.types[type]).map((type) => ({ value: `type:${type}`, label: t(typeLabel[type]) })),
    ...facets.tags.map(([tag]) => ({ value: `tag:${tag}`, label: `#${tag}` })),
    ...workflows.map((name) => ({ value: `workflow:${name}`, label: workflowText(name) })),
    { value: 'trash', label: t('trash') }
  ])

  function readSort(): Sort {
    try {
      const kept = localStorage.getItem(SORT_KEY)
      if (kept && sorts.some((s) => s.value === kept)) return kept as Sort
    } catch {
      /* none kept */
    }
    return 'updated'
  }

  function setSort(next: Sort) {
    sort = next
    try {
      localStorage.setItem(SORT_KEY, next)
    } catch {
      /* kept for this window only */
    }
  }

  const valid = (at: number) => alive && at === generation && unlocked

  // Security checks depend on content and the day, not sync ACKs. They
  // never delay the list or the selection of a newly saved item.
  const checkHealth = single(async () => {
    const at = generation
    const key = contentKey
    if (!valid(at) || !key || key === checkedKey) return
    healthStatus = 'loading'
    try {
      const next = await call('vault/health')
      if (valid(at) && key === contentKey) {
        findings = next
        checkedKey = key
        healthStatus = 'ready'
      }
    } catch (e) {
      if (valid(at) && key === contentKey) {
        healthStatus = 'error'
        if (!closed(e, status)) toast(() => errorText(e))
      }
    }
  })

  const loadConflicts = single(async () => {
    const at = generation
    if (!valid(at)) return
    try {
      const next = await call('vault/conflicts')
      if (valid(at)) conflicts = next
    } catch (e) {
      if (valid(at) && !closed(e, status)) toast(() => errorText(e))
    }
  })

  const loadTrash = single(async () => {
    const at = generation
    if (!valid(at) || !trash) return
    try {
      const next = await call('vault/list', { trash: true })
      if (valid(at)) {
        trashed = next
        trashLoaded = true
      }
    } catch (e) {
      if (valid(at) && !closed(e, status)) toast(() => errorText(e))
    }
  })

  const reload = single(async () => {
    const at = generation
    if (!valid(at)) return
    try {
      const next = await call('vault/list')
      if (!valid(at)) return
      entries = next
      loaded = true
      error = ''
      contentKey = healthKey(entries)
      void checkHealth()
      void loadConflicts()
      void loadTrash()
    } catch (e) {
      if (!valid(at)) return
      error = closed(e, status) ? 'LOCKED' : errorText(e)
      if (error === 'LOCKED') entries = trashed = conflicts = findings = []
      loaded = true
    }
  })

  // Neither an old read nor a saved selection crosses a lock/account switch.
  $effect(() => {
    void sessionKey
    untrack(() => {
      generation += 1
      entries = trashed = conflicts = findings = []
      contentKey = checkedKey = ''
      editing = dirty = creating = importing = comparing = false
      chosen = new Set()
      leaving = tagging = accessTag = null
      loaded = false
      trashLoaded = false
      healthStatus = 'loading'
      error = unlocked ? '' : 'LOCKED'
      if (unlocked) void reload()
    })
  })

  // A filter that no longer has items falls back to everything.
  $effect(() => {
    if (loaded && (scope.startsWith('tag:') || scope.startsWith('type:')))
      if (!scopes.some((option) => option.value === scope)) scope = 'all'
  })

  // The trash is read when it is opened.
  $effect(() => {
    if (trash) void loadTrash()
  })

  $effect(() => {
    const tagged = scope.startsWith('tag:')
    rememberPlain('vault.tag', tagged ? scope : null)
    if (!tagged) remember('vault.scope', scope)
    remember('vault.selected', selected)
  })

  // Another filter or search starts over, at the top of its rows and with nothing chosen.
  $effect(() => {
    void scope
    void query
    void sort
    limit = PAGE
    chosen = new Set()
    cursor = null
  })

  // The open item is always among the rows, however far down it lies.
  $effect(() => {
    const at = selected ? shown.findIndex((entry) => entry.id === selected) : -1
    if (at >= limit) limit = at + PAGE
  })

  // More rows once the list's end comes into view.
  $effect(() => {
    if (!sentinel || !list) return
    const observer = new IntersectionObserver((seen) => seen.some((e) => e.isIntersecting) && (limit += PAGE), { root: list })
    observer.observe(sentinel)
    return () => observer.disconnect()
  })

  // Keep an item open: the first one when the open one left the list.
  $effect(() => {
    if (!current && shown.length && !editing) selected = shown[0].id
  })

  // An item that changed under an edit's form, or a lock, ends nothing: the save names its version.
  $effect(() => {
    if (!unlocked) editing = false
  })

  // The command palette opens an item here.
  $effect(() => {
    const id = focus.id
    if (!id || !loaded) return
    focus.id = null
    scope = 'all'
    query = ''
    void select(id)
  })

  /** Open an item; with an edit that has unsaved changes, ask first. */
  async function select(id: string, move = false) {
    if (id === selected) return
    if (editing && dirty) {
      leaving = id
      return
    }
    editing = false
    selected = id
    if (!move) return
    await tick()
    list?.querySelector<HTMLElement>(`[data-id="${id}"]`)?.focus()
  }

  function leave() {
    const id = leaving
    leaving = null
    editing = false
    dirty = false
    if (id) void select(id)
  }

  function range(from: string, to: string): string[] {
    const ids = shown.map((entry) => entry.id)
    const [a, b] = [ids.indexOf(from), ids.indexOf(to)].sort((x, y) => x - y)
    return a < 0 ? [to] : ids.slice(a, b + 1)
  }

  /** A click opens the item; ⌘ adds it to what is chosen and Shift chooses up to it. */
  function onRowClick(entry: any, event: MouseEvent) {
    if (!trash && unlocked && selected && (event.metaKey || event.ctrlKey)) {
      const next = new Set(chosen.size ? chosen : [selected])
      if (next.has(entry.id)) next.delete(entry.id)
      else next.add(entry.id)
      chosen = next
    } else if (!trash && unlocked && selected && event.shiftKey) {
      chosen = new Set(range(selected, entry.id))
    } else {
      chosen = new Set()
      cursor = null
      void select(entry.id)
    }
  }

  /** The row under a right-click: one outside what is chosen becomes the open item. */
  function rowAt(element: Element) {
    const id = element.closest('[data-id]')?.getAttribute('data-id')
    const entry = id ? shown.find((e) => e.id === id) : null
    if (!entry) return null
    if (!chosen.has(entry.id)) {
      chosen = new Set()
      void select(entry.id)
    }
    return unlocked && !isManaged(entry.item) ? entry : null
  }

  function onListKey(event: KeyboardEvent) {
    const held = mod === '⌘' ? event.metaKey : event.ctrlKey
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const step = event.key === 'ArrowDown' ? 1 : -1
      const from = event.shiftKey ? (cursor ?? selected) : selected
      const at = shown.findIndex((entry) => entry.id === from)
      const next = shown[Math.max(0, Math.min(shown.length - 1, at + step))]
      if (!next) return
      if (event.shiftKey && selected && !trash && unlocked) {
        if (shown.indexOf(next) >= limit) limit = shown.indexOf(next) + PAGE
        cursor = next.id
        chosen = new Set(range(selected, next.id))
        void tick().then(() => list?.querySelector<HTMLElement>(`[data-id="${next.id}"]`)?.focus())
      } else {
        chosen = new Set()
        cursor = null
        void select(next.id, true)
      }
    } else if (trash || !unlocked) {
      return
    } else if (event.key === 'a' && held) {
      event.preventDefault()
      chosen = new Set(shown.filter((e) => !isManaged(e.item)).map((e) => e.id))
    } else if (event.key === 'Delete' || (event.key === 'Backspace' && held)) {
      event.preventDefault()
      if (chosen.size) {
        if (picked.length) void trashChosen()
      } else if (current && !isManaged(current.item)) void discard(current)
    } else if (event.key === 'Enter' && current) {
      event.preventDefault()
      document.querySelector<HTMLElement>('[data-testid="vault-inspector"] button')?.focus()
    } else if (event.key === 'Escape' && chosen.size) {
      event.stopPropagation()
      chosen = new Set()
      cursor = null
    }
  }

  function pickScope(next: string) {
    if (next.startsWith('workflow:')) return go(`/vault/${next.slice('workflow:'.length)}`)
    scope = next as Scope
    if (workflow) go('/vault')
  }

  const openApproval = (id: string) => bridge(() => window.dmsg.openApproval(id)).catch((e) => toast(() => errorText(e)))

  function onSearchKey(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' && shown.length) {
      event.preventDefault()
      void select(selected ?? shown[0].id, true)
    } else if (event.key === 'Escape' && query) {
      event.stopPropagation()
      query = ''
    }
  }

  async function copyField(field: string | undefined) {
    if (!current || !field || !valueOf(current.item, field)) return
    try {
      const { clear_after_ms } = await bridge<{ clear_after_ms: number }>(() => window.dmsg.copySecret(current.id, field, null))
      toast((seconds) => t('copied', { seconds }), { ms: clear_after_ms, countdown: true, testid: 'secret-copied' })
    } catch (e) {
      toast(() => errorText(e))
    }
  }

  /**
   * ⌘N opens a new item, ⌘F the search; ⌘⇧C copies the open item's
   * secret, ⌘⇧U its user name and ⌘⇧O its one-time code (Ctrl elsewhere).
   */
  function onKey(event: KeyboardEvent) {
    const held = mod === '⌘' ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
    if (!held || event.altKey || creating || comparing || importing) return
    const key = event.key.toLowerCase()
    if (event.shiftKey) {
      if (!unlocked || !current || trash || editing || workflow) return
      if (key === 'c') {
        event.preventDefault()
        const primary = templates?.[current.item.type]?.secret
        const secret = primary && valueOf(current.item, primary) ? primary : current.item.fields.find((f: any) => f.type === 'concealed' && f.value)?.id
        void copyField(secret)
      } else if (key === 'u') {
        event.preventDefault()
        const name = valueOf(current.item, 'username')
        if (name) void copyText(name)
      } else if (key === 'o') {
        event.preventDefault()
        void copyField(otpField(current.item)?.id)
      }
      return
    }
    if (key === 'n' && unlocked) {
      event.preventDefault()
      if (trash) scope = 'all'
      creating = true
    } else if (key === 'f') {
      event.preventDefault()
      search?.focus()
      search?.select()
    }
  }

  async function act(work: () => Promise<unknown>, done?: string) {
    try {
      await work()
      if (done) toast(() => done)
    } catch (e) {
      toast(() => errorText(e))
    }
  }

  const star = (entry: any) => act(() => call('vault/edit', { id: entry.id, base: entry.revision, changes: { favorite: !entry.item.favorite } }))
  const discard = (entry: any) =>
    act(() => call(trash ? 'vault/restore' : 'vault/delete', { id: entry.id }), trash ? t('restored') : t('movedToTrash'))
  const duplicate = (entry: any) =>
    act(async () => {
      const copy: any = await call('vault/duplicate', { id: entry.id, title: t('copyTitle', { title: entry.item.title }) })
      await saved(copy.id)
    }, t('duplicated'))

  const trashChosen = () =>
    act(async () => {
      await call('vault/delete', { ids: picked })
      chosen = new Set()
    }, t('movedToTrash'))
  const retag = () =>
    act(async () => {
      await call('vault/tags', { ids: picked, [tagging === 'add' ? 'add' : 'remove']: batchTags })
      tagging = null
      batchTags = []
    }, t('saved'))

  /** A new item opens once the list holds it. */
  async function saved(id: string) {
    const at = generation
    await reload()
    if (!valid(at)) return
    editing = false
    selected = id
    if (workflow) go('/vault')
  }

  function compare(id?: string) {
    compareFrom = Math.max(0, id ? conflicts.findIndex((c) => c.copy.id === id) : 0)
    comparing = true
  }

  onMount(() => {
    void call('vault/templates').then((answer: any) => {
      if (alive) templates = answer.templates
    }).catch((e) => {
      if (alive) toast(() => errorText(e))
    })
    const off = onChange(['vault/changed', 'session/changed', 'sync/changed'], reload)
    return () => {
      alive = false
      generation += 1
      off()
    }
  })
</script>

<svelte:window onkeydown={onKey} />

{#snippet rowMenu(entry: any)}
  {#if trash}
    <ContextMenu.Item class={menuItemClass} onSelect={() => discard(entry)}>
      <span class="flex items-center gap-2"><Icon name="restore" class="text-faint" />{t('restore')}</span>
    </ContextMenu.Item>
  {:else if picked.length > 1 && chosen.has(entry.id)}
    <ContextMenu.Item class={menuItemClass} onSelect={() => (tagging = 'add')}>
      <span class="flex items-center gap-2"><Icon name="tag" class="text-faint" />{t('addTags')}</span>
    </ContextMenu.Item>
    <ContextMenu.Item class={menuItemClass} onSelect={() => (tagging = 'remove')}>
      <span class="flex items-center gap-2"><Icon name="tag" class="text-faint" />{t('removeTags')}</span>
    </ContextMenu.Item>
    <ContextMenu.Separator class="my-1 h-px bg-line" />
    <ContextMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={trashChosen}>
      <span class="flex items-center gap-2"><Icon name="trash" />{t('moveToTrash')}</span>
    </ContextMenu.Item>
  {:else}
    <ContextMenu.Item class={menuItemClass} onSelect={() => star(entry)}>
      <span class="flex items-center gap-2"><Icon name={entry.item.favorite ? 'starred' : 'star'} class="text-faint" />{entry.item.favorite ? t('unstar') : t('star')}</span>
    </ContextMenu.Item>
    <ContextMenu.Item class={menuItemClass} onSelect={() => duplicate(entry)}>
      <span class="flex items-center gap-2"><Icon name="duplicate" class="text-faint" />{t('duplicate')}</span>
    </ContextMenu.Item>
    <ContextMenu.Separator class="my-1 h-px bg-line" />
    <ContextMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => discard(entry)}>
      <span class="flex items-center gap-2"><Icon name="trash" />{t('moveToTrash')}</span>
    </ContextMenu.Item>
  {/if}
{/snippet}

<div class="@container relative flex h-full min-h-0" data-testid="vault">
  {#if error === 'LOCKED'}
    <Locked {status} testid="vault-locked" />
  {:else}
    <aside class="hidden w-[184px] shrink-0 flex-col border-e border-line @min-[860px]:flex">
      <Filters {scope} {workflow} onScope={pickScope} onWorkflow={(name) => go(`/vault/${name}`)} {facets} flagged={flagged.size} onAccess={(tag) => (accessTag = tag)} />
    </aside>

    {#if workflow}
      <div class="flex min-w-0 flex-1 flex-col">
        <div class="border-b border-line p-3 @min-[860px]:hidden">
          <Select bind:value={() => `workflow:${workflow}`, pickScope} options={scopes} label={t('vaultFilters')} testid="vault-scope" />
        </div>
        <div class="min-h-0 flex-1">
          <Page><Workflows open={openApproval} tab={workflow} {go} /></Page>
        </div>
      </div>
    {:else}
    <section class="relative flex w-[var(--pane,264px)] shrink-0 flex-col border-e border-line @min-[860px]:w-[var(--pane,296px)]">
      <div class="grid min-w-0 grid-cols-1 gap-2 p-3 pb-2">
        <div class="flex items-center gap-1.5">
          <label class="relative min-w-0 flex-1">
            <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
            <input
              bind:this={search}
              bind:value={query}
              type="search"
              class="ps-8"
              placeholder={t('searchItems', { n: trash ? trashed.length : facets.all })}
              aria-label={t('search')}
              onkeydown={onSearchKey}
              data-testid="vault-search"
            />
          </label>
          {#if unlocked && !trash}
            <Tooltip content={`${t('add')} · ${mod}N`}>
              {#snippet children(props)}
                <button {...props} class={iconButtonClass('primary', 'md')} aria-label={t('add')} onclick={() => (creating = true)} data-testid="vault-new">
                  <Icon name="add" />
                </button>
              {/snippet}
            </Tooltip>
          {/if}
        </div>
        <div class="flex items-center gap-1.5">
          <Select bind:value={() => scope, pickScope} options={scopes} label={t('vaultFilters')} class="min-w-0 flex-1 @min-[860px]:hidden" testid="vault-scope" />
          {#if !trash}
            <Select
              bind:value={() => sort, (next) => setSort(next as Sort)}
              options={sorts.map((s) => ({ value: s.value, label: t(s.label) }))}
              label={t('sortBy')}
              class="min-w-0 flex-1"
              testid="vault-sort"
            />
          {/if}
          {#if unlocked && !trash}
            <Tooltip content={t('importTitle')}>
              {#snippet children(props)}
                <button {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('importTitle')} onclick={() => (importing = true)} data-testid="vault-import-open">
                  <Icon name="import" />
                </button>
              {/snippet}
            </Tooltip>
          {/if}
        </div>
      </div>

      {#if conflicts.length && !trash}
        <div class="mx-3 mb-2 flex items-center gap-2 rounded-xl border border-warn/35 bg-warn-pale px-2.5 py-2 text-[12px] text-ink-2" data-testid="vault-conflicts">
          <Icon name="conflict" class="shrink-0 text-warn" />
          <span class="min-w-0 flex-1">{t('conflictBanner', { n: conflicts.length })}</span>
          <button class={buttonClass('secondary', 'xs')} onclick={() => compare()} data-testid="vault-conflicts-review">{t('review')}</button>
        </div>
      {/if}

      <TargetMenu find={rowAt} testid="vault-row-menu">
        {#snippet menu(entry)}{@render rowMenu(entry)}{/snippet}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul bind:this={list} class="min-h-0 flex-1 overflow-y-auto px-2 pb-24" aria-label={t('vault')} onkeydown={onListKey} data-testid="vault-list">
        {#each listed as entry (entry.id)}
          <Row
            {entry}
            selected={entry.id === selected}
            conflict={conflicted.has(entry.id)}
            flagged={flagged.has(entry.id)}
            {trash}
            chosen={chosen.has(entry.id)}
            onClick={(event) => onRowClick(entry, event)}
          />
        {:else}
          {#if loaded && (!trash || trashLoaded)}
            <li class="grid justify-items-center gap-2 px-4 py-10 text-center" data-testid="vault-empty">
              {#if status?.sync_initialized === false && unlocked}
                <Icon name="sync" class="text-faint" />
                <b class="text-[13.5px] font-semibold" data-testid="vault-initial-sync">{t(status?.syncing ? 'syncInitial' : 'syncInitialWaiting')}</b>
                <p class="text-[12.5px] text-muted">{t('syncInitialHint')}</p>
              {:else if query}
                <p class="text-[13px] text-muted [overflow-wrap:anywhere]">{t('noMatch', { query })}</p>
              {:else if trash}
                <p class="text-[13px] text-muted">{t('emptyTrash')}</p>
              {:else if scope === 'health'}
                <p class="text-[13px] text-muted">{t(healthStatus === 'ready' ? 'healthClear' : healthStatus === 'error' ? 'error' : 'loading')}</p>
              {:else if scope === 'all'}
                <span class="grid size-11 place-items-center text-faint [&_svg]:size-6"><Icon name="vault" /></span>
                <b class="text-[13.5px] font-semibold">{t('emptyVault')}</b>
                <p class="text-[12.5px] text-muted">{t('emptyVaultHint')}</p>
                {#if unlocked}
                  <button class={buttonClass('secondary', 'sm')} onclick={() => (importing = true)}><Icon name="import" />{t('importTitle')}</button>
                {/if}
              {:else}
                <p class="text-[13px] text-muted">{t('noItems')}</p>
              {/if}
            </li>
          {/if}
        {/each}
        {#if listed.length < shown.length}<li bind:this={sentinel} class="h-px" aria-hidden="true"></li>{/if}
      </ul>
      </TargetMenu>

      {#if picked.length > 1}
        <div class="absolute inset-x-2 bottom-2 grid gap-1.5 rounded-xl border border-line bg-surface p-2 shadow-pop" data-testid="vault-batch">
          <div class="flex items-center gap-1.5 text-[12.5px]">
            <span class="min-w-0 flex-1 text-muted">{t('nSelected', { n: picked.length })}</span>
            <button class={buttonClass('ghost', 'xs')} onclick={() => (chosen = new Set(shown.filter((e) => !isManaged(e.item)).map((e) => e.id)))}>{t('selectAll')}</button>
            <button class={buttonClass('ghost', 'xs')} onclick={() => (chosen = new Set())} data-testid="vault-batch-done">{t('done')}</button>
          </div>
          <div class="flex flex-wrap gap-1">
            <button class={buttonClass('secondary', 'xs')} onclick={() => (tagging = 'add')} data-testid="vault-batch-tag"><Icon name="tag" />{t('addTags')}</button>
            <button class={buttonClass('secondary', 'xs')} onclick={() => (tagging = 'remove')}><Icon name="tag" />{t('removeTags')}</button>
            <button class={buttonClass('danger', 'xs')} onclick={trashChosen} data-testid="vault-batch-trash"><Icon name="trash" />{t('moveToTrash')}</button>
          </div>
        </div>
      {/if}
    </section>
    <PaneHandle name="vault" />

    <div class="min-w-0 flex-1">
      {#if current}
        {#key current.id}
          <Inspector
            entry={current}
            conflict={conflicted.has(current.id)}
            {trash}
            {unlocked}
            {mod}
            {go}
            device={status?.device_id ?? null}
            issues={issues.get(current.id)}
            bind:editing
            bind:dirty
            onCompare={() => compare(current.id)}
            onDuplicated={saved}
          />
        {/key}
      {:else if loaded}
        <div class="grid h-full place-items-center p-8 text-center text-[13px] text-faint">
          <p>{shown.length ? t('selectItem') : error || ''}</p>
        </div>
      {/if}
    </div>
    {/if}
  {/if}
</div>

<NewItem bind:open={creating} {mod} {templates} onSaved={saved} />
<Import bind:open={importing} />
<Conflict bind:open={comparing} {conflicts} start={compareFrom} />

<Dialog bind:open={() => leaving !== null, (next) => !next && (leaving = null)} title={t('unsavedTitle')} description={t('unsavedHint')} icon="edit" tone="warn" testid="vault-unsaved">
  <p class="text-[13px] text-muted">{t('unsavedBody')}</p>
  {#snippet footer()}
    <span class="min-w-0 flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (leaving = null)}>{t('keepEditing')}</button>
    <button class={buttonClass('danger')} onclick={leave} data-testid="vault-unsaved-discard">{t('discard')}</button>
  {/snippet}
</Dialog>

<Dialog
  bind:open={() => tagging !== null, (next) => !next && ((tagging = null), (batchTags = []))}
  title={tagging === 'remove' ? t('removeTags') : t('addTags')}
  description={t('nSelected', { n: picked.length })}
  icon="tag"
  testid="vault-batch-tags"
>
  <TagInput bind:tags={batchTags} placeholder={t('tagsAdd')} maxLength={40} testid="vault-batch-tags-input" />
  {#snippet footer()}
    <span class="min-w-0 flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (tagging = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={!batchTags.length} onclick={retag} data-testid="vault-batch-tags-save">{t('save')}</button>
  {/snippet}
</Dialog>

<AgentAccess
  bind:open={() => !!accessTag, (open) => !open && (accessTag = null)}
  title={t('vaultAgentAccess', { tag: accessTag ?? '' })}
  description={t('vaultAgentAccessHint')}
  testid="vault-access"
  choices={[
    { capability: `vault.read:${accessTag}`, label: t('canRead') },
    { capability: `vault.write:${accessTag}`, label: t('canWrite') }
  ]}
/>
