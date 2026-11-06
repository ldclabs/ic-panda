<script lang="ts">
  import { Command, Dialog } from 'bits-ui'
  import { appearances } from '../../shared/appearance'
  import { localeNames, locales } from '../../shared/locales'
  import { appearance, appearanceLabel, setAppearance } from '../appearance.svelte'
  import type { Glyph } from '../components/glyphs'
  import Icon from '../components/Icon.svelte'
  import { locale, setLocale, t } from '../i18n/index.svelte'
  import { copyText } from '../components/clipboard'
  import { iconButtonClass } from '../components/ui'
  import { toast } from '../components/toast.svelte'
  import { bridge, call, errorText } from '../rpc'
  import { nameOf } from '../views/channels/names.svelte'
  import { focus as fileFocus } from '../views/files/focus.svelte'
  import { focus } from '../views/vault/focus.svelte'
  import { workflowText } from '../views/secrets/workflow-text'
  import { isManaged, otpField, typeIcon, typeLabel, typeOf, valueOf, workflowIcon, workflows } from '../views/vault/vault'
  import { settingsPages, shortcuts, views } from './nav'
  import { best, GROUP_LIMIT, tokensOf } from './palette'
  import { lock, nextStep, shortId, sync } from './session'

  /**
   * The command palette: go to any view or settings page, the session,
   * appearance and language commands, and what the account holds (vault items,
   * channels, letters, files), found by typing. It matches on its own and shows
   * a few rows per group, so a large vault costs nothing. → opens a vault
   * item's copy and open actions.
   */
  let {
    open = $bindable(false),
    status,
    mod,
    collapsed,
    go,
    onToggleSidebar,
    onShortcuts
  }: {
    open?: boolean
    status: any
    mod: string
    collapsed: boolean
    go: (path: string) => void
    onToggleSidebar: () => void
    onShortcuts: () => void
  } = $props()

  interface Entry {
    label: string
    /** What tells entries apart when the label is not unique. */
    value?: string
    icon: Glyph
    run: () => unknown
    hint?: string
    keywords?: string[]
    checked?: boolean
    /** More to do with it, as a second level. */
    more?: () => Entry[]
  }

  let search = $state('')
  let highlighted = $state('')
  let input = $state<HTMLInputElement | null>(null)
  /** The entry whose actions show instead of the groups. */
  let detail = $state<Entry | null>(null)
  /** What the account holds, read when the palette opens on an unlocked device. */
  let items = $state<any[]>([])
  let channels = $state<any[]>([])
  let letters = $state<any[]>([])
  let files = $state<any[]>([])
  const unlocked = $derived(status?.state === 'unlocked')
  const tokens = $derived(tokensOf(search))

  // Read again only when the palette opens or the lock changes, not on every status refresh.
  $effect(() => {
    items = channels = letters = files = []
    detail = null
    if (!open) {
      search = ''
    } else if (unlocked) {
      call<any[]>('vault/list').then((list) => open && (items = list)).catch(() => {})
      call<any[]>('channel/list').then((list) => open && (channels = list)).catch(() => {})
      call<any[]>('inbox/list', { refresh: false }).then((list) => open && (letters = list)).catch(() => {})
    }
  })

  // Files are searched in the daemon, a moment after typing stops.
  $effect(() => {
    const asked = search.trim()
    if (!open || !unlocked || detail || asked.length < 2) {
      files = []
      return
    }
    const timer = setTimeout(() => void call<any[]>('files/search', { query: asked }).then((list) => asked === search.trim() && (files = list)).catch(() => {}), 150)
    return () => clearTimeout(timer)
  })

  /** Settings › About dMsg shows what the check finds. */
  function checkUpdates() {
    go('/settings/about')
    void window.dmsg.checkUpdates().catch(() => {})
  }

  /** What can be done with a vault item; dMsg's own keys only open. */
  function itemActions(entry: any, opened: Entry): Entry[] {
    const actions: Entry[] = [{ ...opened, more: undefined, label: t('open'), hint: undefined }]
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

  const vaultEntries = $derived(items.map((entry): Entry => {
    const type = typeOf(entry.item)
    const opened: Entry = {
      label: entry.item.title,
      value: `vault:${entry.id}`,
      icon: typeIcon[type],
      hint: t(typeLabel[type]),
      keywords: [...entry.item.tags, valueOf(entry.item, 'username'), valueOf(entry.item, 'url')].filter(Boolean),
      run: () => {
        focus.id = entry.id
        go(views.vault.path)
      }
    }
    opened.more = () => itemActions(entry, opened)
    return opened
  }))
  const channelEntries = $derived(channels.map((channel): Entry => ({
    label: channel.name,
    value: `channel:${channel.id}`,
    icon: 'channels',
    hint: channel.unread ? String(channel.unread) : undefined,
    run: () => go(`/channels?open=${channel.id}`)
  })))
  const letterEntries = $derived(letters.filter((letter) => letter.state === 'visible').map((letter): Entry => ({
    label: letter.text.split('\n')[0] || t('letter'),
    value: `letter:${letter.order_id}`,
    icon: 'inbox',
    hint: nameOf(letter.sender) || shortId(letter.sender),
    keywords: [nameOf(letter.sender), letter.sender, letter.text].filter(Boolean),
    run: () => go(`/inbox?open=${letter.order_id}`)
  })))
  const fileEntries = $derived(files.slice(0, GROUP_LIMIT).map((file): Entry => ({
    label: file.name,
    value: `file:${file.id}`,
    icon: 'file',
    keywords: [search],
    run: () => {
      fileFocus.id = file.id
      go(views.files.path)
    }
  })))

  const groups = $derived.by(() => {
    if (detail) return [{ heading: detail.label, entries: tokens.length ? best(detail.more?.() ?? [], tokens) : (detail.more?.() ?? []) }]
    const places: Entry[] = [
      ...shortcuts.map((view) => ({ label: t(view.label), icon: view.icon, hint: `${mod}${view.key}`, run: () => go(view.path) })),
      { label: t(views.reminders.label), icon: views.reminders.icon, run: () => go(views.reminders.path) }
    ]
    const secretParts: Entry[] = workflows.map((name) => ({ label: `${t('vault')} › ${workflowText(name)}`, value: `workflow:${name}`, icon: workflowIcon[name], run: () => go(`/vault/${name}`) }))
    const pages: Entry[] = settingsPages.map((page) => ({ label: `${t('settings')} › ${t(page.label)}`, value: `settings:${page.value}`, icon: page.icon, run: () => go(`/settings/${page.value}`) }))
    const step = nextStep(status)
    const offered: (Entry | null)[] = [
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
      run: () => setAppearance(value)
    }))
    if (!tokens.length) return [
      { heading: t('goTo'), entries: places },
      { heading: t('commands'), entries: commands },
      { heading: t('appearance'), entries: looks }
    ]
    const languages: Entry[] = locales.map((code) => ({
      label: `${t('language')}: ${localeNames[code]}`,
      icon: 'language',
      keywords: [code, 'language'],
      checked: locale() === code,
      run: () => setLocale(code)
    }))
    return [
      { heading: t('vault'), entries: best(vaultEntries, tokens) },
      { heading: t('channels'), entries: best(channelEntries, tokens) },
      { heading: t('inbox'), entries: best(letterEntries, tokens, 8) },
      { heading: t('files'), entries: fileEntries },
      { heading: t('goTo'), entries: best([...places, ...secretParts, ...pages], tokens) },
      { heading: t('commands'), entries: best(commands, tokens) },
      { heading: t('appearance'), entries: best(looks, tokens) },
      { heading: t('language'), entries: best(languages, tokens) }
    ]
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

  function enter(entry: Entry) {
    detail = entry
    search = ''
    input?.focus()
  }

  function leave() {
    detail = null
    search = ''
    input?.focus()
  }

  function onInputKey(event: KeyboardEvent) {
    const atEnd = input?.selectionStart === search.length
    if (event.key === 'ArrowRight' && !detail && current?.more && atEnd) {
      event.preventDefault()
      enter(current)
    } else if (event.key === 'ArrowLeft' && detail && !search) {
      event.preventDefault()
      leave()
    }
  }
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
            placeholder={detail ? detail.label : t('searchOrCommand')}
            class="h-12 border-0 bg-transparent px-0 text-[15px] focus-visible:shadow-none"
            data-testid="palette-input"
          />
          <kbd class="rounded border border-line bg-surface-2 px-1.5 font-mono text-[10.5px] leading-[18px]">esc</kbd>
        </div>
        <Command.List class="max-h-[min(400px,60vh)] overflow-y-auto p-1.5">
          <Command.Viewport>
            <Command.Empty class="py-10 text-center text-[13px] text-faint">{t('noResults')}</Command.Empty>
            {#each groups as group (group.heading)}
              {#if group.entries.length}
                <Command.Group>
                  <Command.GroupHeading class="px-2.5 pt-2.5 pb-1 text-[11px] font-medium text-faint">{group.heading}</Command.GroupHeading>
                  <Command.GroupItems>
                    {#each group.entries as entry (entry.value ?? entry.label)}
                      <Command.Item
                        value={entry.value ?? entry.label}
                        keywords={entry.keywords}
                        onSelect={() => run(entry)}
                        class="flex h-9 cursor-default items-center gap-2.5 rounded-lg px-2.5 text-[13.5px] outline-none select-none data-selected:bg-pale"
                      >
                        <span class="grid size-6 shrink-0 place-items-center text-faint [&_svg]:size-4.5">
                          <Icon name={entry.icon} class="size-3.5" />
                        </span>
                        <span class="min-w-0 flex-1 truncate">{entry.label}</span>
                        {#if entry.checked}<Icon name="check" class="size-3.5 text-forest" />{/if}
                        {#if entry.hint}<kbd class="max-w-[40%] truncate font-mono text-[10.5px] text-faint">{entry.hint}</kbd>{/if}
                        {#if entry.more}
                          <button
                            class={iconButtonClass('ghost', 'md', '-me-1.5 size-7')}
                            aria-label={`${t('paletteMore')} · ${entry.label}`}
                            onclick={(event) => {
                              event.stopPropagation()
                              enter(entry)
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
        <div class="flex gap-4 border-t border-line bg-surface-2 px-4 py-2 text-[11.5px] text-faint">
          <span class="inline-flex items-center gap-1.5"><Icon name="arrows" class="size-3.5" />{t('paletteMove')}</span>
          <span class="inline-flex items-center gap-1.5"><Icon name="enter" class="size-3.5" />{t('paletteRun')}</span>
          {#if current?.more}<span class="inline-flex items-center gap-1.5"><kbd class="font-mono text-[11px]">→</kbd>{t('paletteMore')}</span>{/if}
        </div>
      </Command.Root>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
