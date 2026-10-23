<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import { copyText } from '../components/clipboard'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import Locked from '../components/page/Locked.svelte'
  import Select from '../components/Select.svelte'
  import TagInput from '../components/TagInput.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, closed, errorText, onChange, single } from '../rpc'
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
    type Scope,
    type Sort,
    type Templates
  } from './vault/vault'

  /**
   * The vault: filters, the list and the open item side by side. New
   * items, imports and conflict copies open in dialogs; select mode acts on
   * many items. Files have their own library and items only name them.
   * Each change reaches the list through the daemon's `vault/changed`.
   */
  let { status, mod, go }: { status: any; mod: string; go: (path: string) => void } = $props()

  const SORT_KEY = 'dmsg.vault.sort'

  let entries = $state<any[]>([])
  let trashed = $state<any[]>([])
  let conflicts = $state<any[]>([])
  let findings = $state<any[]>([])
  let templates = $state<Templates | null>(null)
  let scope = $state<Scope>('all')
  /** The tag whose Agent access is open. */
  let accessTag = $state<string | null>(null)
  let query = $state('')
  let sort = $state<Sort>(readSort())
  let selected = $state<string | null>(null)
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
  let choosing = $state(false)
  let chosen = $state<Set<string>>(new Set())
  let tagging = $state<'add' | 'remove' | null>(null)
  let batchTags = $state<string[]>([])
  let generation = 0
  let alive = true
  let contentKey = ''
  let checkedKey = ''
  let armedCopy = 0
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
  const current = $derived(shown.find((entry) => entry.id === selected) ?? null)
  const scopes = $derived([
    { value: 'all', label: t('allItems') },
    { value: 'starred', label: t('starred') },
    { value: 'health', label: t('securityCheck') },
    ...types.filter((type) => facets.types[type]).map((type) => ({ value: `type:${type}`, label: t(typeLabel[type]) })),
    ...facets.tags.map(([tag]) => ({ value: `tag:${tag}`, label: `#${tag}` })),
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
      selected = null
      editing = dirty = creating = importing = comparing = choosing = false
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
    if (scope.startsWith('tag:') || scope.startsWith('type:'))
      if (!scopes.some((option) => option.value === scope)) scope = 'all'
  })

  // The trash is read when it is opened; select mode ends with it.
  $effect(() => {
    if (trash) {
      choosing = false
      void loadTrash()
    }
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

  function choose(id: string) {
    const next = new Set(chosen)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    chosen = next
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

  async function copyField(field: string | undefined, label: string, armed: boolean) {
    if (!current || !field || !valueOf(current.item, field)) return
    if (armed && Date.now() - armedCopy > 3000) {
      armedCopy = Date.now()
      toast(() => t('pressAgainCopy', { what: label }), { ms: 3000 })
      return
    }
    armedCopy = 0
    try {
      const { clear_after_ms } = await bridge<{ clear_after_ms: number }>(() => window.dmsg.copySecret(current.id, field, null))
      toast((seconds) => t('copied', { seconds }), { ms: clear_after_ms, countdown: true, testid: 'secret-copied' })
    } catch (e) {
      toast(() => errorText(e))
    }
  }

  /**
   * ⌘N opens a new item, ⌘F the search; ⌘⇧C copies the open item's
   * secret (pressed twice), ⌘⇧U its user name and ⌘⇧O its one-time code
   * (Ctrl elsewhere).
   */
  function onKey(event: KeyboardEvent) {
    const held = mod === '⌘' ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
    if (!held || event.altKey || creating || comparing || importing) return
    const key = event.key.toLowerCase()
    if (event.shiftKey) {
      if (!unlocked || !current || trash || editing) return
      if (key === 'c') {
        event.preventDefault()
        const primary = templates?.[current.item.type]?.secret
        const secret = primary && valueOf(current.item, primary) ? primary : current.item.fields.find((f: any) => f.type === 'concealed' && f.value)?.id
        void copyField(secret, t('secret'), true)
      } else if (key === 'u') {
        event.preventDefault()
        const name = valueOf(current.item, 'username')
        if (name) void copyText(name)
      } else if (key === 'o') {
        event.preventDefault()
        void copyField(otpField(current.item)?.id, t('fieldTypeOtp'), false)
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

  const ids = () => [...chosen].filter((id) => entries.some((e) => e.id === id && !isManaged(e.item)))
  const trashChosen = () =>
    act(async () => {
      await call('vault/delete', { ids: ids() })
      chosen = new Set()
    }, t('movedToTrash'))
  const retag = () =>
    act(async () => {
      await call('vault/tags', { ids: ids(), [tagging === 'add' ? 'add' : 'remove']: batchTags })
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

<div class="@container relative flex h-full min-h-0" data-testid="vault">
  {#if error === 'LOCKED'}
    <Locked {status} testid="vault-locked" />
  {:else}
    <aside class="hidden w-[184px] shrink-0 flex-col border-e border-line @min-[860px]:flex">
      <Filters bind:scope {facets} flagged={flagged.size} onAccess={(tag) => (accessTag = tag)} />
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
          {/if}
        </div>
        <div class="flex items-center gap-1.5">
          <Select bind:value={() => scope, (next) => (scope = next as Scope)} options={scopes} label={t('vaultFilters')} class="min-w-0 flex-1 @min-[860px]:hidden" testid="vault-scope" />
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
            <Tooltip content={t('selectItems')}>
              {#snippet children(props)}
                <button
                  {...props}
                  class={buttonClass(choosing ? 'secondary' : 'ghost', 'md', 'size-8 px-0')}
                  aria-label={t('selectItems')}
                  aria-pressed={choosing}
                  onclick={() => ((choosing = !choosing), (chosen = new Set()))}
                  data-testid="vault-choose"
                >
                  <Icon name="select" />
                </button>
              {/snippet}
            </Tooltip>
            <Tooltip content={t('importTitle')}>
              {#snippet children(props)}
                <button {...props} class={buttonClass('ghost', 'md', 'size-8 px-0')} aria-label={t('importTitle')} onclick={() => (importing = true)} data-testid="vault-import-open">
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

      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul bind:this={list} class="min-h-0 flex-1 overflow-y-auto px-2 pb-24" aria-label={t('vault')} onkeydown={onListKey} data-testid="vault-list">
        {#each shown as entry (entry.id)}
          <Row
            {entry}
            selected={entry.id === selected}
            conflict={conflicted.has(entry.id)}
            flagged={flagged.has(entry.id)}
            {trash}
            {unlocked}
            {choosing}
            chosen={chosen.has(entry.id)}
            onSelect={() => select(entry.id)}
            onChoose={() => choose(entry.id)}
            onStar={() => star(entry)}
            onDuplicate={() => duplicate(entry)}
            onDiscard={() => discard(entry)}
          />
        {:else}
          {#if loaded && (!trash || trashLoaded)}
            <li class="grid justify-items-center gap-2 px-4 py-10 text-center" data-testid="vault-empty">
              {#if query}
                <p class="text-[13px] text-muted [overflow-wrap:anywhere]">{t('noMatch', { query })}</p>
              {:else if trash}
                <p class="text-[13px] text-muted">{t('emptyTrash')}</p>
              {:else if scope === 'health'}
                <p class="text-[13px] text-muted">{t(healthStatus === 'ready' ? 'healthClear' : healthStatus === 'error' ? 'error' : 'loading')}</p>
              {:else if scope === 'all'}
                <span class="grid size-11 place-items-center rounded-xl border border-dashed border-edge-soft text-faint [&_svg]:size-5"><Icon name="vault" /></span>
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
      </ul>

      {#if choosing}
        <div class="absolute inset-x-2 bottom-2 grid gap-1.5 rounded-xl border border-line bg-surface p-2 shadow-pop" data-testid="vault-batch">
          <div class="flex items-center gap-1.5 text-[12.5px]">
            <span class="min-w-0 flex-1 text-muted">{t('nSelected', { n: ids().length })}</span>
            <button class={buttonClass('ghost', 'xs')} onclick={() => (chosen = new Set(shown.filter((e) => !isManaged(e.item)).map((e) => e.id)))}>{t('selectAll')}</button>
            <button class={buttonClass('ghost', 'xs')} onclick={() => ((choosing = false), (chosen = new Set()))}>{t('done')}</button>
          </div>
          <div class="flex flex-wrap gap-1">
            <button class={buttonClass('secondary', 'xs')} disabled={!ids().length} onclick={() => (tagging = 'add')} data-testid="vault-batch-tag"><Icon name="tag" />{t('addTags')}</button>
            <button class={buttonClass('secondary', 'xs')} disabled={!ids().length} onclick={() => (tagging = 'remove')}><Icon name="tag" />{t('removeTags')}</button>
            <button class={buttonClass('danger', 'xs')} disabled={!ids().length} onclick={trashChosen} data-testid="vault-batch-trash"><Icon name="trash" />{t('moveToTrash')}</button>
          </div>
        </div>
      {/if}
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
  description={t('nSelected', { n: ids().length })}
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
