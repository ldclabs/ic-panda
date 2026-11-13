<script lang="ts">
  import Dialog from '../components/Dialog.svelte'
  import { t, type Label } from '../i18n/index.svelte'
  import { shortcutLabel, type ShortcutStatus } from '../../shared/shortcuts'
  import { buttonClass } from '../components/ui'

  /** Every shortcut the main window answers to, on request (⌘/). */
  let { open = $bindable(false), mod, keys }: { open?: boolean; mod: string; keys: { back: string; forward: string } } = $props()
  let global = $state<ShortcutStatus | null>(null)
  $effect(() => {
    if (!open) return
    let alive = true
    global = null
    void window.dmsg.shortcuts().then(value => { if (alive) global = value }, () => {})
    return () => { alive = false }
  })

  const groups: { label: Label; rows: [Label, string][] }[] = $derived([
    {
      label: 'shortcutsEverywhere',
      rows: [
        ['searchOrCommand', `${mod}K`],
        ['shortcutViews', `${mod}1 – ${mod}8`],
        ['settings', `${mod},`],
        ['lock', `${mod}L`],
        ['shortcutBackForward', `${keys.back} ${keys.forward}`],
        ['shortcutsTitle', `${mod}/`]
      ]
    },
    {
      label: 'shortcutsLists',
      rows: [
        ['shortcutFind', `${mod}F`],
        ['shortcutNew', `${mod}N`],
        ['shortcutMove', '↑ ↓'],
        ['shortcutChooseMore', `Shift+↑ ↓`],
        ['shortcutChooseAll', `${mod}A`],
        ['moveToTrash', 'Delete']
      ]
    },
    {
      label: 'channels',
      rows: [
        ['shortcutChannelMove', `${mod === '⌘' ? '⌥' : 'Alt+'}↑ ↓`],
        ['shortcutSendLine', 'Shift+↵']
      ]
    },
    {
      label: 'vault',
      rows: [
        ['shortcutCopySecret', `${mod}⇧C`],
        ['shortcutCopyUser', `${mod}⇧U`],
        ['shortcutCopyCode', `${mod}⇧O`]
      ]
    },
    {
      label: 'files',
      rows: [
        ['shortcutPreview', 'Space'],
        ['shortcutRename', 'F2'],
        ['shortcutParent', `${mod}↑`]
      ]
    }
  ])
</script>

<Dialog bind:open title={t('shortcutsTitle')} icon="info" testid="shortcuts">
  <section class="mb-4 grid gap-2">
    <h3 class="text-[11.5px] font-medium text-faint">{t('globalShortcuts')}</h3>
    <dl class="grid gap-1 text-[12.5px]">
      {#each ['open', 'lock'] as action}
        {@const key = action as 'open' | 'lock'}
        <div class="flex flex-wrap items-baseline justify-between gap-2">
          <dt>{t(key === 'open' ? 'shortcutOpen' : 'shortcutLock')}</dt>
          <dd><kbd dir="ltr" class="text-muted">{global ? global.registered[key] ? shortcutLabel(global.bindings[key], window.dmsg.platform) : t('off') : t('loading')}</kbd></dd>
        </div>
      {/each}
    </dl>
    <div><button class={buttonClass('ghost', 'xs')} onclick={() => { open = false; void window.dmsg.showMain('/settings/shortcuts') }}>{t('settings')}</button></div>
  </section>
  <div class="grid gap-4 sm:grid-cols-2">
    {#each groups as group (group.label)}
      <section>
        <h3 class="pb-1 text-[11.5px] font-medium text-faint">{t(group.label)}</h3>
        <dl class="grid gap-1">
          {#each group.rows as [label, key] (label)}
            <div class="flex items-baseline justify-between gap-3 text-[12.5px]">
              <dt class="min-w-0 text-ink-2">{t(label)}</dt>
              <dd><kbd class="font-mono text-[11.5px] whitespace-nowrap text-muted" dir="ltr">{key}</kbd></dd>
            </div>
          {/each}
        </dl>
      </section>
    {/each}
  </div>
</Dialog>
