<script lang="ts">
  import { Command, Dialog } from 'bits-ui'
  import { appearances } from '../../shared/appearance'
  import { localeNames, locales } from '../../shared/locales'
  import { appearance, appearanceLabel, setAppearance } from '../appearance.svelte'
  import type { Glyph } from '../components/glyphs'
  import Icon from '../components/Icon.svelte'
  import { locale, setLocale, t } from '../i18n/index.svelte'
  import { copyText } from '../components/clipboard'
  import { toast } from '../components/toast.svelte'
  import { bridge, call, errorText } from '../rpc'
  import { focus } from '../views/vault/focus.svelte'
  import { isManaged, otpField, typeIcon, typeLabel, typeOf, valueOf } from '../views/vault/vault'
  import { shortcuts, views } from './nav'
  import { startLogin } from './login.svelte'
  import { lock, nextStep, sync } from './session'

  /** The command palette: go to any view, and the session, appearance and language commands, found by typing. */
  let {
    open = $bindable(false),
    status,
    mod,
    collapsed,
    go,
    onToggleSidebar
  }: {
    open?: boolean
    status: any
    mod: string
    collapsed: boolean
    go: (path: string) => void
    onToggleSidebar: () => void
  } = $props()

  interface Entry {
    label: string
    /** What the palette matches and tells entries apart by, when the label is not unique. */
    value?: string
    icon: Glyph
    run: () => unknown
    hint?: string
    keywords?: string[]
    checked?: boolean
  }

  let search = $state('')
  /** The vault's items while the palette is open on an unlocked device; they and their actions show once something is typed. */
  let items = $state<any[]>([])
  const unlocked = $derived(status?.state === 'unlocked')

  // Read again only when the palette opens or the lock changes, not on every status refresh.
  $effect(() => {
    items = []
    if (!open) {
      search = ''
    } else if (unlocked) {
      call<any[]>('vault/list')
        .then((list) => open && (items = list))
        .catch(() => {})
    }
  })

  /** Settings › About dMsg shows what the check finds. */
  function checkUpdates() {
    go('/settings/about')
    void window.dmsg.checkUpdates().catch(() => {})
  }

  const groups = $derived.by(() => {
    const places: Entry[] = [
      ...shortcuts.map((view) => ({ label: t(view.label), icon: view.icon, hint: `${mod}${view.key}`, run: () => go(view.path) })),
      { label: `${t('settings')} › ${t('legacy')}`, icon: views.legacy.icon, run: () => go(views.legacy.path) },
      { label: `${t('settings')} › ${t('aboutApp')}`, icon: 'info', run: () => go('/settings/about') }
    ]
    const step = nextStep(status)
    const offered: (Entry | null)[] = [
      unlocked ? { label: t('lock'), icon: 'lock', hint: `${mod}L`, run: lock } : step ? { label: t(step.label), icon: step.icon, run: step.run } : null,
      status?.account ? { label: t('sync'), icon: 'sync-now', run: sync } : null,
      status?.state && status.state !== 'logged_out' ? { label: t('loginOther'), icon: 'login', run: () => startLogin() } : null,
      { label: t('checkUpdates'), icon: 'sync-now', run: checkUpdates },
      { label: collapsed ? t('expandSidebar') : t('collapseSidebar'), icon: collapsed ? 'sidebar-unfold' : 'sidebar-fold', run: onToggleSidebar }
    ]
    const commands = offered.filter((entry): entry is Entry => !!entry)
    const looks: Entry[] = appearances.map((value) => ({
      label: `${t('appearance')}: ${t(appearanceLabel[value])}`,
      icon: value,
      checked: appearance() === value,
      run: () => setAppearance(value)
    }))
    const languages: Entry[] = locales.map((code) => ({
      label: `${t('language')}: ${localeNames[code]}`,
      icon: 'language',
      keywords: [code, 'language'],
      checked: locale() === code,
      run: () => setLocale(code)
    }))
    const found: Entry[] = search.trim()
      ? items.flatMap((entry) => {
          const type = typeOf(entry.item)
          const title = entry.item.title
          const keywords = [...entry.item.tags, valueOf(entry.item, 'username'), valueOf(entry.item, 'url')].filter(Boolean)
          const open: Entry = {
            label: title,
            value: `${title}\u2063${entry.id}`,
            icon: typeIcon[type],
            hint: t(typeLabel[type]),
            keywords,
            run: () => {
              focus.id = entry.id
              go(views.vault.path)
            }
          }
          // Actions on the item, found by its title too; dMsg's own keys only open.
          if (isManaged(entry.item)) return [open]
          const action = (label: string, icon: Glyph, run: () => unknown): Entry => ({
            label: `${label} · ${title}`,
            value: `${label}\u2063${title}\u2063${entry.id}`,
            icon,
            keywords: [title, ...keywords],
            run
          })
          const actions: Entry[] = [open]
          const username = valueOf(entry.item, 'username')
          if (username) actions.push(action(t('copyUsername'), 'user', () => copyText(username)))
          const secret = (entry.item.fields ?? []).find((f: any) => f.type === 'concealed' && f.value)
          if (secret) actions.push(action(t('copySecret'), 'password', () => copySecret(entry.id, secret.id)))
          const otp = otpField(entry.item)
          if (otp) actions.push(action(t('copyCode'), 'otp', () => copySecret(entry.id, otp.id)))
          const url = valueOf(entry.item, 'url')
          if (/^https?:\/\//i.test(url)) actions.push(action(t('openWebsite'), 'external', () => window.dmsg.openUrl(url).catch((e) => toast(() => errorText(e)))))
          return actions
        })
      : []
    return [
      { heading: t('vault'), entries: found },
      { heading: t('goTo'), entries: places },
      { heading: t('commands'), entries: commands },
      { heading: t('appearance'), entries: looks },
      { heading: t('language'), entries: languages }
    ]
  })

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
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay
      class="fixed inset-0 z-50 bg-ink/25 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0"
    />
    <Dialog.Content
      class="fixed top-[12vh] left-1/2 z-50 w-[min(580px,calc(100vw-32px))] -translate-x-1/2 overflow-hidden rounded-2xl border border-line bg-surface text-ink shadow-pop outline-none duration-200 ease-spring-soft data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-98 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-98"
      data-testid="palette"
    >
      <Dialog.Title class="sr-only">{t('searchOrCommand')}</Dialog.Title>
      <Command.Root loop label={t('searchOrCommand')}>
        <div class="flex items-center gap-2.5 border-b border-line px-4 text-faint">
          <Icon name="search" />
          <Command.Input
            bind:value={search}
            placeholder={t('searchOrCommand')}
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
                        <span class="grid size-6 shrink-0 place-items-center rounded-md border border-line bg-surface text-faint">
                          <Icon name={entry.icon} class="size-3.5" />
                        </span>
                        <span class="min-w-0 flex-1 truncate">{entry.label}</span>
                        {#if entry.checked}<Icon name="check" class="size-3.5 text-forest" />{/if}
                        {#if entry.hint}<kbd class="font-mono text-[10.5px] text-faint">{entry.hint}</kbd>{/if}
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
        </div>
      </Command.Root>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
