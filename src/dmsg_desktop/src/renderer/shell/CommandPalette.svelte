<script lang="ts">
  import { untrack } from 'svelte'
  import { Command, Dialog } from 'bits-ui'
  import { appearances } from '../../shared/appearance'
  import { localeDir, localeNames, locales } from '../../shared/locales'
  import { appearance, appearanceLabel, setAppearance } from '../appearance.svelte'
  import type { Glyph } from '../components/glyphs'
  import HighlightText from '../components/HighlightText.svelte'
  import { messagePreview, type HighlightSpan } from '../components/highlight'
  import Icon from '../components/Icon.svelte'
  import Notice from '../components/page/Notice.svelte'
  import { locale, setLocale, t } from '../i18n/index.svelte'
  import { copyText } from '../components/clipboard'
  import { buttonClass, iconButtonClass } from '../components/ui'
  import { toast } from '../components/toast.svelte'
  import { bridge, call, errorText } from '../rpc'
  import { nameOf, visibleName } from '../views/channels/names.svelte'
  import { focus as fileFocus } from '../views/files/focus.svelte'
  import { focus } from '../views/vault/focus.svelte'
  import { workflowText } from '../views/secrets/workflow-text'
  import { isManaged, otpField, typeIcon, typeLabel, typeOf, valueOf, workflowIcon, vaultWorkflows } from '../views/vault/vault'
  import { settingsGroupsFor, shortcuts, views } from './nav'
  import { best, GROUP_LIMIT, tokensOf } from './palette'
  import { lock, nextStep, shortId, sync } from './session'

  /**
   * The command palette: go to any view or settings page, the session,
   * appearance and language commands, and what the account holds (vault items,
   * channels, messages, letters, files and folders), searched via the daemon's
   * unified pinyin/multi-token `search/global` engine. Prefixes:
   * `>` commands & navigation, `#` vault, `@` channels & messages, `/` files.
   * → lazily loads a vault item's copy and open actions.
   */
  let {
    open = $bindable(false),
    status,
    mod,
    collapsed,
    go,
    onToggleSidebar,
    onShortcuts,
    onNearbyReceive
  }: {
    open?: boolean
    status: any
    mod: string
    collapsed: boolean
    go: (path: string) => void
    onToggleSidebar: () => void
    onShortcuts: () => void
    onNearbyReceive?: () => void
  } = $props()

  interface Entry {
    label: string
    spans?: HighlightSpan[]
    /** What tells entries apart when the label is not unique. */
    value?: string
    icon: Glyph
    run: () => unknown
    hint?: string
    sender?: string
    keywords?: string[]
    checked?: boolean
    /** Lazy second-level actions (e.g. vault copy actions). */
    more?: () => Entry[] | Promise<Entry[]>
  }

  interface GlobalSearchState {
    kind: 'idle' | 'loading' | 'ready' | 'failed'
    query?: string
    result?: {
      vault: any[]
      channels: any[]
      messages: any[]
      letters: any[]
      folders: any[]
      files: any[]
    }
    cause?: unknown
  }

  let search = $state('')
  let highlighted = $state('')
  let input = $state<HTMLInputElement | null>(null)
  /** The entry whose actions show instead of the groups. */
  let detail = $state.raw<{ entry: Entry; actions: Entry[]; loading: boolean } | null>(null)
  let globalState = $state.raw<GlobalSearchState>({ kind: 'idle' })
  let globalSeq = 0
  let globalRetry = $state(0)
  const unlocked = $derived(status?.state === 'unlocked')

  /** Quick scope prefixes: `>` commands, `#` vault, `@` channels/messages, `/` files. */
  const parsed = $derived.by(() => {
    const trimmed = search.trim()
    if (!trimmed) return { mode: 'all' as const, query: '', scopes: [] as string[] }
    const first = trimmed[0]
    const rest = trimmed.slice(1).trim()
    if (first === '>') return { mode: 'commands' as const, query: rest, scopes: [] as string[] }
    if (first === '#' && rest) return { mode: 'scoped' as const, query: rest, scopes: ['vault'] }
    if (first === '@' && rest) return { mode: 'scoped' as const, query: rest, scopes: ['channels', 'messages', 'received_letters'] }
    if (first === '/' && rest) return { mode: 'scoped' as const, query: rest, scopes: ['files'] }
    return { mode: 'all' as const, query: trimmed, scopes: ['vault', 'channels', 'messages', 'files', 'received_letters'] }
  })
  const tokens = $derived(tokensOf(parsed.query))

  // Reset on close or lock change.
  $effect(() => {
    if (!open) {
      search = ''
      detail = null
      globalState = { kind: 'idle' }
    } else if (!unlocked) {
      detail = null
      globalState = { kind: 'idle' }
    }
  })

  // Unified daemon global search (supports 1+ chars, including single CJK characters and pinyin).
  $effect(() => {
    const active = open && unlocked && !detail && parsed.mode !== 'commands' && parsed.query.length >= 1
    const q = parsed.query
    const scopes = parsed.scopes
    void globalRetry
    if (!active) {
      globalState = { kind: 'idle' }
      return
    }
    const req = ++globalSeq
    let cancelled = false
    const prev = untrack(() => globalState)
    if (prev.kind !== 'ready' || prev.query !== `${scopes.join(',')}:${q}`) {
      globalState = { kind: 'loading' }
    }
    const timer = setTimeout(async () => {
      try {
        const result = await call<any>('search/global', { query: q, scopes, limit: GROUP_LIMIT })
        if (!cancelled && req === globalSeq) {
          globalState = { kind: 'ready', query: `${scopes.join(',')}:${q}`, result }
        }
      } catch (cause) {
        if (!cancelled && req === globalSeq) {
          globalState = { kind: 'failed', cause }
        }
      }
    }, 90)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  })

  /** Settings › About dMsg shows what the check finds. */
  function checkUpdates() {
    go('/settings/about')
    void window.dmsg.checkUpdates().catch(() => {})
  }

  /** What can be done with a vault item; dMsg's own keys only open. */
  function itemActions(entry: any, opened: Entry): Entry[] {
    const actions: Entry[] = [{ ...opened, more: undefined, label: t('open'), spans: undefined, hint: undefined }]
    if (isManaged(entry.item)) return actions
    const username = valueOf(entry.item, 'username')
    if (username) actions.push({ label: t('copyUsername'), icon: 'user', run: () => copyText(username) })
    const secret = (entry.item.fields ?? []).find((f: any) => f.type === 'concealed' && f.value)
    if (secret) actions.push({ label: t('copySecret'), icon: 'password', run: () => copySecret(entry.id, secret.id) })
    const otp = otpField(entry.item)
    if (otp) actions.push({ label: t('copyCode'), icon: 'otp', run: () => copySecret(entry.id, otp.id) })
    const url = valueOf(entry.item, 'url')
    if (/^https?:\/\//i.test(url)) actions.push({ label: t('openWebsite'), icon: 'external', run: () => window.dmsg.openUrl(url).catch((e) => toast(() => errorText(e))) })
    return actions
  }

  const hits = $derived(globalState.kind === 'ready' ? globalState.result : null)

  const vaultEntries = $derived(
    (hits?.vault ?? []).map((summary): Entry => {
      const type = typeOf({ type: summary.kind ?? summary.type })
      const matchField = summary.matchField ?? summary.match_field
      const matchValue = summary.matchValue ?? summary.match_value
      const hint =
        matchField === 'notes'
          ? t('matchedInNotes')
          : matchField === 'tag' && matchValue
            ? `#${matchValue}`
            : summary.subtitle || t(typeLabel[type])
      const opened: Entry = {
        label: summary.title,
        spans: summary.titleSpans ?? summary.title_spans,
        value: `vault:${summary.id}`,
        icon: typeIcon[type],
        hint,
        run: () => {
          focus.id = summary.id
          go(views.vault.path)
        }
      }
      opened.more = async () => {
        const shown = await call<any>('vault/show', { id: summary.id })
        return itemActions(shown, opened)
      }
      return opened
    })
  )

  const channelEntries = $derived(
    (hits?.channels ?? []).map((hit): Entry => ({
      label: hit.name,
      spans: hit.name_spans,
      value: `channel:${hit.id}`,
      icon: 'channels',
      hint: hit.match_field === 'member' && hit.match_value
        ? t('matchedMember', { name: hit.match_value })
        : hit.unread
          ? String(hit.unread)
          : undefined,
      run: () => go(`/channels?open=${hit.id}`)
    }))
  )

  const messageEntries = $derived(
    (hits?.messages ?? []).map((hit): Entry => {
      const chId = hit.channel ?? hit.channel_id
      const preview = messagePreview(hit)
      return {
        label: preview.text || '…',
        spans: preview.spans,
        value: `msg:${chId}:${hit.message.seq}`,
        icon: 'channels',
        sender: hit.message.account,
        hint: hit.sender_name || nameOf(hit.message.account, 'search') || undefined,
        run: () => go(`/channels?open=${chId}&seq=${hit.message.seq}`)
      }
    })
  )

  // The inbox opens received letters by id; sent, pending and draft letters live elsewhere.
  const letterEntries = $derived(
    (hits?.letters ?? []).map((hit): Entry => ({
      label: (hit.snippet_spans?.map((s: HighlightSpan) => s.text).join('') || hit.display_name || shortId(hit.account)),
      spans: hit.snippet_spans?.length ? hit.snippet_spans : hit.name_spans,
      value: `letter:${hit.box_kind}:${hit.id}`,
      icon: 'inbox',
      sender: hit.account,
      hint: hit.display_name || nameOf(hit.account, 'search') || shortId(hit.account),
      run: () => go(`/inbox?open=${hit.id}`)
    }))
  )

  const fileEntries = $derived(
    (hits?.files ?? []).slice(0, GROUP_LIMIT).map((hit): Entry => {
      const isFolder = hit.kind === 'folder'
      return {
        label: hit.name,
        spans: hit.name_spans,
        value: `${isFolder ? 'folder' : 'file'}:${hit.id}`,
        icon: isFolder ? 'folder' : 'file',
        hint: hit.match_field === 'tag' && hit.match_value
          ? `#${hit.match_value}`
          : (hit.path ?? []).join(' / ') || undefined,
        run: () => {
          if (isFolder) fileFocus.folder = hit.id
          else fileFocus.id = hit.id
          go(views.files.path)
        }
      }
    })
  )

  const groups = $derived.by(() => {
    if (detail) {
      const actionTokens = tokensOf(search)
      return [{ heading: detail.entry.label, entries: actionTokens.length ? best(detail.actions, actionTokens) : detail.actions }]
    }
    const places: Entry[] = [
      ...shortcuts.map((view) => ({ label: t(view.label), icon: view.icon, hint: `${mod}${view.key}`, run: () => go(view.path) })),
      { label: t(views.reminders.label), icon: views.reminders.icon, run: () => go(views.reminders.path) }
    ]
    const secretParts: Entry[] = vaultWorkflows(status?.account_kind === 'local').map((name) => ({ label: `${t('vault')} › ${workflowText(name)}`, value: `workflow:${name}`, icon: workflowIcon[name], run: () => go(`/vault/${name}`) }))
    const pages: Entry[] = settingsGroupsFor(status?.account_kind === 'local').flatMap(group => group.pages).map((page) => ({ label: `${t('settings')} › ${t(page.label)}`, value: `settings:${page.value}`, icon: page.icon, run: () => go(`/settings/${page.value}`) }))
    const step = nextStep(status)
    const offered: (Entry | null)[] = [
      unlocked && onNearbyReceive ? { label: t('nearbyReceive'), icon: 'pair', run: onNearbyReceive } : null,
      unlocked ? { label: t('lock'), icon: 'lock', hint: `${mod}L`, run: lock } : step ? { label: t(step.label), icon: step.icon, run: step.run } : null,
      status?.account ? { label: t('sync'), icon: 'sync-now', run: sync } : null,
      { label: t('checkUpdates'), icon: 'sync-now', run: checkUpdates },
      { label: collapsed ? t('expandSidebar') : t('collapseSidebar'), icon: collapsed ? 'sidebar-unfold' : 'sidebar-fold', run: onToggleSidebar },
      { label: t('shortcutsTitle'), icon: 'info', hint: `${mod}/`, run: onShortcuts }
    ]
    const commands = offered.filter((entry): entry is Entry => !!entry)
    const looks: Entry[] = appearances.map((value) => ({
      label: `${t('appearance')}: ${t(appearanceLabel[value])}`,
      icon: value,
      checked: appearance() === value,
      run: () => setAppearance(value).catch(() => toast(() => t('appearanceFailed')))
    }))
    if (!search.trim()) return [
      { heading: t('goTo'), entries: places },
      { heading: t('commands'), entries: commands },
      { heading: t('appearance'), entries: looks }
    ]
    const languages: Entry[] = locales.map((code) => ({
      label: `${t('language')}: ${localeNames[code]}`,
      icon: 'language',
      keywords: [code, 'language'],
      checked: locale() === code,
      run: () => setLocale(code).catch(() => toast(() => t('languageFailed')))
    }))
    if (parsed.mode === 'commands') {
      return [
        { heading: t('goTo'), entries: tokens.length ? best([...places, ...secretParts, ...pages], tokens) : [...places, ...secretParts, ...pages] },
        { heading: t('commands'), entries: tokens.length ? best(commands, tokens) : commands },
        { heading: t('appearance'), entries: tokens.length ? best(looks, tokens) : looks },
        { heading: t('language'), entries: tokens.length ? best(languages, tokens) : languages }
      ]
    }
    const list: { heading: string; entries: Entry[] }[] = [
      { heading: t('vault'), entries: vaultEntries },
      { heading: t('channels'), entries: channelEntries },
      { heading: t('matchingMessages'), entries: messageEntries },
      { heading: t('inbox'), entries: letterEntries },
      { heading: t('files'), entries: fileEntries }
    ]
    if (parsed.mode === 'all') {
      list.push(
        { heading: t('goTo'), entries: best([...places, ...secretParts, ...pages], tokens) },
        { heading: t('commands'), entries: best(commands, tokens) },
        { heading: t('appearance'), entries: best(looks, tokens) },
        { heading: t('language'), entries: best(languages, tokens) }
      )
    }
    return list
  })

  const rows = $derived(groups.flatMap((group) => group.entries))
  const current = $derived(rows.find((entry) => (entry.value ?? entry.label) === highlighted) ?? null)

  /** A hidden field (by default the item's main secret) to the clipboard, through the main process. */
  async function copySecret(id: string, field: string | null) {
    try {
      const { clear_after_ms } = await bridge<{ clear_after_ms: number }>(() => window.dmsg.copySecret(id, field, null))
      toast((seconds) => t('copied', { seconds }), { ms: clear_after_ms, countdown: true, testid: 'secret-copied' })
    } catch (e) {
      toast(() => errorText(e))
    }
  }

  function run(entry: Entry) {
    open = false
    void entry.run()
  }

  async function enter(entry: Entry) {
    if (!entry.more) return
    const next = { entry, actions: [] as Entry[], loading: true }
    detail = next
    search = ''
    input?.focus()
    try {
      const actions = await entry.more()
      if (detail === next) detail = { entry, actions, loading: false }
    } catch (e) {
      if (detail === next) {
        detail = null
        toast(() => errorText(e))
      }
    }
  }

  function leave() {
    detail = null
    search = ''
    input?.focus()
  }

  function onInputKey(event: KeyboardEvent) {
    if (event.isComposing) return
    const atEnd = input?.selectionStart === search.length && input.selectionEnd === search.length
    if (event.key === (rtl ? 'ArrowLeft' : 'ArrowRight') && !detail && current?.more && atEnd) {
      event.preventDefault()
      void enter(current)
    } else if (event.key === (rtl ? 'ArrowRight' : 'ArrowLeft') && detail && !search) {
      event.preventDefault()
      leave()
    }
  }

  const rtl = $derived(localeDir(locale()) === 'rtl')
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay
      class="fixed inset-0 z-50 bg-ink/25 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0"
    />
    <Dialog.Content
      onEscapeKeydown={(event) => {
        if (!detail) return
        event.preventDefault()
        leave()
      }}
      class="fixed top-[12vh] left-1/2 z-50 w-[min(580px,calc(100vw-32px))] -translate-x-1/2 overflow-hidden rounded-2xl border border-line bg-surface text-ink shadow-pop outline-none duration-200 ease-spring-soft data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-98 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-98"
      data-testid="palette"
    >
      <Dialog.Title class="sr-only">{t('searchOrCommand')}</Dialog.Title>
      <Command.Root loop shouldFilter={false} bind:value={highlighted} label={t('searchOrCommand')}>
        <div class="flex items-center gap-2.5 border-b border-line px-4 text-faint">
          {#if detail}
            <button class={iconButtonClass('ghost', 'md', '-ms-1.5')} aria-label={t('back')} onclick={leave} data-testid="palette-back"><Icon name="back" /></button>
          {:else}
            <Icon name="search" />
          {/if}
          <Command.Input
            bind:ref={input}
            bind:value={search}
            onkeydown={onInputKey}
            placeholder={detail ? detail.entry.label : t('searchOrCommand')}
            class="h-12 border-0 bg-transparent px-0 text-title-3 focus-visible:shadow-none"
            data-testid="palette-input"
          />
          <kbd class="rounded border border-line bg-surface-2 px-1.5 font-mono text-caption leading-[18px]">esc</kbd>
        </div>
        <Command.List class="max-h-[min(400px,60vh)] overflow-y-auto p-1.5">
          <Command.Viewport>
            {#if globalState.kind !== 'failed'}
              <Command.Empty class="py-10 text-center text-body text-faint">
                {t(globalState.kind === 'loading' || detail?.loading ? 'loading' : 'noResults')}
              </Command.Empty>
            {/if}
            {#each groups as group (group.heading)}
              {#if group.entries.length}
                <Command.Group>
                  <Command.GroupHeading class="px-2.5 pt-2.5 pb-1 text-caption font-medium text-faint">{group.heading}</Command.GroupHeading>
                  <Command.GroupItems>
                    {#each group.entries as entry (entry.value ?? entry.label)}
                      <Command.Item
                        value={entry.value ?? entry.label}
                        keywords={entry.keywords}
                        onSelect={() => run(entry)}
                        class="flex h-9 cursor-default items-center gap-2.5 rounded-lg px-2.5 text-body outline-none select-none data-selected:bg-pale"
                      >
                        <span class="grid size-6 shrink-0 place-items-center text-faint [&_svg]:size-4.5">
                          <Icon name={entry.icon} class="size-3.5" />
                        </span>
                        <span {@attach visibleName(entry.sender)} class="min-w-0 flex-1 truncate">
                          <HighlightText text={entry.label} spans={entry.spans} />
                        </span>
                        {#if entry.checked}<Icon name="check" class="size-3.5 text-forest" />{/if}
                        {#if entry.hint}<kbd class="max-w-[40%] truncate font-mono text-caption text-faint">{entry.hint}</kbd>{/if}
                        {#if entry.more}
                          <button
                            class={iconButtonClass('ghost', 'md', '-me-1.5 size-7')}
                            aria-label={`${t('paletteMore')} · ${entry.label}`}
                            onclick={(event) => {
                              event.stopPropagation()
                              void enter(entry)
                            }}
                            data-testid="palette-more"><Icon name="chevron-end" /></button>
                        {/if}
                      </Command.Item>
                    {/each}
                  </Command.GroupItems>
                </Command.Group>
              {/if}
            {/each}
          </Command.Viewport>
        </Command.List>
      </Command.Root>
      {#if globalState.kind === 'failed'}
        <div class="px-3 py-2">
          <Notice tone="warn" failure={errorText(globalState.cause)}>
            <div class="flex items-center justify-between gap-2">
              <span class="min-w-0 flex-1">{errorText(globalState.cause)}</span>
              <button class={buttonClass('subtle', 'xs')} onclick={() => (globalRetry += 1)}>{t('retry')}</button>
            </div>
          </Notice>
        </div>
      {/if}
      <div class="flex gap-4 border-t border-line bg-surface-2 px-4 py-2 text-caption text-faint">
        <span class="inline-flex items-center gap-1.5"><Icon name="arrows" class="size-3.5" />{t('paletteMove')}</span>
        <span class="inline-flex items-center gap-1.5"><Icon name="enter" class="size-3.5" />{t('paletteRun')}</span>
        {#if current?.more}<span class="inline-flex items-center gap-1.5"><kbd class="font-mono text-caption">{rtl ? '←' : '→'}</kbd>{t('paletteMore')}</span>{/if}
      </div>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
