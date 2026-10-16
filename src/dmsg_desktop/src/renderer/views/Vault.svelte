<script lang="ts">
  import { onMount, tick } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import Select from '../components/Select.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code } from '../rpc'
  import { unlock } from '../shell/session'
  import Conflict from './vault/Conflict.svelte'
  import Filters from './vault/Filters.svelte'
  import { focus } from './vault/focus.svelte'
  import Imports from './vault/Imports.svelte'
  import { importFile, picked, settle } from './vault/importing.svelte'
  import Inspector from './vault/Inspector.svelte'
  import NewItem from './vault/NewItem.svelte'
  import Row from './vault/Row.svelte'
  import { facets as count, inScope, matches, typeLabel, types, type Scope } from './vault/vault'

  /**
   * The vault: filters, the list and the open item side by side. New items
   * and conflict copies open in dialogs; files dropped on the list are
   * imported through the main process.
   */
  let { status, mod }: { status: any; mod: string } = $props()

  let entries = $state<any[]>([])
  let trashed = $state<any[]>([])
  let conflicts = $state<any[]>([])
  let scope = $state<Scope>('all')
  let query = $state('')
  let selected = $state<string | null>(null)
  let error = $state('')
  let loaded = $state(false)
  let creating = $state(false)
  let comparing = $state(false)
  let compareFrom = $state(0)
  let dragging = $state(0)
  let search = $state<HTMLInputElement | null>(null)
  let list = $state<HTMLElement | null>(null)

  const unlocked = $derived(status?.state === 'unlocked')
  const trash = $derived(scope === 'trash')
  const facets = $derived(count(entries))
  const conflicted = $derived(new Set(conflicts.map((copy) => copy.id)))
  const shown = $derived(
    (trash ? trashed : entries)
      .filter((entry) => inScope(entry, scope) && matches(entry.item, query))
      .sort((a, b) => (b.item.updatedAt ?? 0) - (a.item.updatedAt ?? 0))
  )
  const current = $derived(shown.find((entry) => entry.id === selected) ?? null)
  const scopes = $derived([
    { value: 'all', label: t('allItems') },
    { value: 'starred', label: t('starred') },
    ...types.filter((type) => facets.types[type]).map((type) => ({ value: `type:${type}`, label: t(typeLabel[type]) })),
    ...facets.tags.map(([tag]) => ({ value: `tag:${tag}`, label: `#${tag}` })),
    { value: 'trash', label: t('trash') }
  ])

  async function load() {
    try {
      entries = await call('vault/list')
      trashed = trash ? await call('vault/list', { trash: true }) : []
      conflicts = await call('vault/conflicts')
      error = ''
      settle(entries)
    } catch (e) {
      entries = trashed = conflicts = []
      error = code(e) === 'LOCKED' ? 'LOCKED' : `${t('error')}: ${code(e)}`
    }
    loaded = true
  }

  // A filter that no longer has items falls back to everything.
  $effect(() => {
    if (scope.startsWith('tag:') || scope.startsWith('type:') || scope === 'starred')
      if (!scopes.some((option) => option.value === scope)) scope = 'all'
  })

  // The trash is read when it is opened.
  $effect(() => {
    if (trash) void load()
  })

  // Keep an item open: the first one when the open one left the list.
  $effect(() => {
    if (!current && shown.length) selected = shown[0].id
  })

  // The command palette opens an item here.
  $effect(() => {
    const id = focus.id
    if (!id || !loaded) return
    focus.id = null
    scope = 'all'
    query = ''
    selected = id
  })

  async function select(id: string, move = false) {
    selected = id
    if (!move) return
    await tick()
    list?.querySelector<HTMLElement>(`[data-id="${id}"]`)?.focus()
  }

  function onListKey(event: KeyboardEvent) {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
    event.preventDefault()
    const at = shown.findIndex((entry) => entry.id === selected)
    const next = shown[Math.max(0, Math.min(shown.length - 1, at + (event.key === 'ArrowDown' ? 1 : -1)))]
    if (next) void select(next.id, true)
  }

  function onSearchKey(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' && shown.length) {
      event.preventDefault()
      void select(selected ?? shown[0].id, true)
    } else if (event.key === 'Escape' && query) {
      event.stopPropagation()
      query = ''
    }
  }

  /** ⌘N opens a new item, ⌘F the search (Ctrl elsewhere). */
  function onKey(event: KeyboardEvent) {
    const held = mod === '⌘' ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
    if (!held || event.altKey || event.shiftKey || creating || comparing) return
    const key = event.key.toLowerCase()
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
      toast(() => (code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`))
    }
    await load()
  }

  const star = (entry: any) => act(() => call('vault/edit', { id: entry.id, base: entry.revision, changes: { favorite: !entry.item.favorite } }))
  const discard = (entry: any) =>
    act(() => call(trash ? 'vault/restore' : 'vault/delete', { id: entry.id }), trash ? t('restored') : t('movedToTrash'))
  const exportFile = (entry: any) => act(() => bridge(() => window.dmsg.exportFile(entry.id, entry.item.file.name)))

  async function add(dropped?: File) {
    const id = await importFile(dropped, load)
    if (id) {
      if (trash) scope = 'all'
      selected = id
    }
  }

  async function saved(id: string) {
    await load()
    selected = id
  }

  function compare(id?: string) {
    compareFrom = Math.max(0, id ? conflicts.findIndex((copy) => copy.id === id) : 0)
    comparing = true
  }

  const files = (event: DragEvent) => !!event.dataTransfer?.types.includes('Files')

  function onDragEnter(event: DragEvent) {
    if (!files(event) || !unlocked) return
    event.preventDefault()
    dragging++
  }

  function onDragOver(event: DragEvent) {
    if (!files(event) || !unlocked) return
    event.preventDefault()
    event.dataTransfer!.dropEffect = 'copy'
  }

  function onDrop(event: DragEvent) {
    if (!files(event)) return
    event.preventDefault()
    dragging = 0
    if (!unlocked) return
    for (const file of Array.from(event.dataTransfer?.files ?? [])) void add(file)
  }

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (event.type === 'file/importing') picked(String((event.data as any)?.name ?? ''))
      if (['vault/changed', 'session/changed', 'sync/changed'].includes(event.type)) void load()
    })
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
  data-testid="vault"
>
  {#if error === 'LOCKED'}
    <div class="grid flex-1 place-items-center p-8" data-testid="vault-locked">
      <div class="grid max-w-sm justify-items-center gap-3 text-center">
        <span class="grid size-12 place-items-center rounded-2xl border border-line bg-surface text-faint shadow-xs [&_svg]:size-6"><Icon name="lock" /></span>
        <p class="text-[13.5px] text-muted">{t('locked')}</p>
        {#if status?.state === 'locked'}
          <button class={buttonClass('primary')} onclick={() => unlock(status)}>{status?.fast_unlock ? t('fastUnlock') : t('login')}</button>
        {/if}
      </div>
    </div>
  {:else}
    <aside class="hidden w-[184px] shrink-0 flex-col border-e border-line @min-[860px]:flex">
      <Filters bind:scope {facets} />
    </aside>

    <section class="relative flex w-[264px] shrink-0 flex-col border-e border-line @min-[860px]:w-[296px]">
      <div class="grid gap-2 p-3 pb-2">
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
                <button {...props} class={buttonClass('primary', 'md', 'size-8 px-0')} aria-label={t('add')} onclick={() => (creating = true)} data-testid="vault-new">
                  <Icon name="add" />
                </button>
              {/snippet}
            </Tooltip>
            <Tooltip content={t('importFile')}>
              {#snippet children(props)}
                <button {...props} class={buttonClass('secondary', 'md', 'size-8 px-0')} aria-label={t('importFile')} onclick={() => add()} data-testid="import-file">
                  <Icon name="upload" />
                </button>
              {/snippet}
            </Tooltip>
          {/if}
        </div>
        <Select bind:value={() => scope, (next) => (scope = next as Scope)} options={scopes} label={t('vaultFilters')} class="w-full @min-[860px]:hidden" testid="vault-scope" />
      </div>

      {#if conflicts.length && !trash}
        <div class="mx-3 mb-2 flex items-center gap-2 rounded-xl border border-warn/35 bg-warn-pale px-2.5 py-2 text-[12px] text-ink-2" data-testid="vault-conflicts">
          <Icon name="conflict" class="shrink-0 text-warn" />
          <span class="min-w-0 flex-1">{t('conflictBanner', { n: conflicts.length })}</span>
          <button class={buttonClass('secondary', 'xs')} onclick={() => compare()} data-testid="vault-conflicts-review">{t('review')}</button>
        </div>
      {/if}

      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul bind:this={list} class="min-h-0 flex-1 overflow-y-auto px-2 pb-24" aria-label={t('vault')} onkeydown={onListKey} data-testid="vault-list">
        {#each shown as entry (entry.id)}
          <Row
            {entry}
            selected={entry.id === selected}
            conflict={conflicted.has(entry.id)}
            {trash}
            {unlocked}
            onSelect={() => select(entry.id)}
            onStar={() => star(entry)}
            onDiscard={() => discard(entry)}
            onExport={() => exportFile(entry)}
          />
        {:else}
          {#if loaded}
            <li class="grid justify-items-center gap-2 px-4 py-10 text-center" data-testid="vault-empty">
              {#if query}
                <p class="text-[13px] text-muted [overflow-wrap:anywhere]">{t('noMatch', { query })}</p>
              {:else if trash}
                <p class="text-[13px] text-muted">{t('emptyTrash')}</p>
              {:else if scope === 'all'}
                <span class="grid size-11 place-items-center rounded-xl border border-dashed border-edge-soft text-faint [&_svg]:size-5"><Icon name="vault" /></span>
                <b class="text-[13.5px] font-semibold">{t('emptyVault')}</b>
                <p class="text-[12.5px] text-muted">{t('emptyVaultHint')}</p>
              {:else}
                <p class="text-[13px] text-muted">{t('noItems')}</p>
              {/if}
            </li>
          {/if}
        {/each}
      </ul>
      <Imports />
    </section>

    <div class="min-w-0 flex-1">
      {#if current}
        {#key current.id}
          <Inspector
            entry={current}
            conflict={conflicted.has(current.id)}
            {trash}
            {unlocked}
            {mod}
            reload={load}
            onCompare={() => compare(current.id)}
          />
        {/key}
      {:else if loaded}
        <div class="grid h-full place-items-center p-8 text-center text-[13px] text-faint">
          <p>{shown.length ? t('selectItem') : error || ''}</p>
        </div>
      {/if}
    </div>
  {/if}

  {#if dragging}
    <div class="pointer-events-none absolute inset-2 z-20 grid place-items-center rounded-2xl border-2 border-dashed border-forest/60 bg-pale/85 text-forest backdrop-blur-[1px] animate-in fade-in-0">
      <div class="grid justify-items-center gap-2">
        <Icon name="upload" class="size-7" />
        <b class="text-[14px] font-semibold">{t('dropToImport')}</b>
        <span class="text-[12px] text-muted">{t('dropDetail')}</span>
      </div>
    </div>
  {/if}
</div>

<NewItem bind:open={creating} {mod} onSaved={saved} onImport={() => add()} />
<Conflict bind:open={comparing} {conflicts} start={compareFrom} reload={load} />
