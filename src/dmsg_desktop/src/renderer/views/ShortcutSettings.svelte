<script lang="ts">
  import { onMount } from 'svelte'
  import Section from '../components/page/Section.svelte'
  import Notice from '../components/page/Notice.svelte'
  import ShortcutInput from '../components/ShortcutInput.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, code, errorText } from '../rpc'
  import { defaultShortcuts, shortcutActions, type ShortcutBindings, type ShortcutStatus } from '../../shared/shortcuts'

  let saved = $state<ShortcutStatus | null>(null)
  let draft = $state<ShortcutBindings>(defaultShortcuts(window.dmsg.platform))
  let loading = $state(true), busy = $state(false), error = $state(''), failureCode = $state('')
  let openRecording = $state(false), lockRecording = $state(false)
  let alive = true, generation = 0
  const duplicate = $derived(!!draft.open && draft.open === draft.lock)
  const dirty = $derived(!!saved && shortcutActions.some(action => draft[action] !== saved!.bindings[action]))
  const unavailable = $derived(!!saved && shortcutActions.some(action => !!saved!.bindings[action] && !saved!.registered[action]))
  const recording = $derived(openRecording || lockRecording)
  const failure = $derived(failureCode === 'SHORTCUT_UNAVAILABLE' ? t('shortcutUnavailable') : error)

  async function load() {
    const turn = ++generation
    loading = true; error = ''; failureCode = ''
    try {
      const value = await bridge<ShortcutStatus>(() => window.dmsg.shortcuts())
      if (alive && turn === generation) { saved = value; draft = { ...value.bindings } }
    } catch (cause) { if (alive && turn === generation) error = errorText(cause) }
    finally { if (alive && turn === generation) loading = false }
  }

  async function save() {
    if (busy || !saved || duplicate || recording) return
    busy = true; error = ''; failureCode = ''
    try {
      const value = await bridge<ShortcutStatus>(() => window.dmsg.setShortcuts({ ...draft }))
      if (alive) { saved = value; draft = { ...value.bindings } }
    } catch (cause) {
      if (alive) { failureCode = code(cause); error = errorText(cause) }
    } finally { if (alive) busy = false }
  }

  onMount(() => { void load(); return () => { alive = false; generation++ } })
</script>

<Section title={t('globalShortcuts')} description={t('globalShortcutsHint')} icon="system" testid="shortcut-settings">
  <div class="grid max-w-xl gap-5">
    {#if failure}<Notice tone="danger">{failure}</Notice>{/if}
    {#if loading}<p class="text-sm text-muted" role="status">{t('loading')}</p>
    {:else if saved}
      <ShortcutInput value={draft.open} label={t('shortcutOpen')} disabled={busy} testid="shortcut-open" bind:recording={openRecording}
        error={duplicate ? t('shortcutDuplicate') : draft.open && draft.open === saved.bindings.open && !saved.registered.open ? t('shortcutUnavailable') : ''}
        onChange={value => { draft.open = value; error = ''; failureCode = '' }} />
      <ShortcutInput value={draft.lock} label={t('shortcutLock')} disabled={busy} testid="shortcut-lock" bind:recording={lockRecording}
        error={duplicate ? t('shortcutDuplicate') : draft.lock && draft.lock === saved.bindings.lock && !saved.registered.lock ? t('shortcutUnavailable') : ''}
        onChange={value => { draft.lock = value; error = ''; failureCode = '' }} />
      <div class="flex flex-wrap gap-2">
        <button class={buttonClass('primary')} disabled={busy || duplicate || recording || (!dirty && !unavailable)} onclick={save} data-testid="shortcuts-save">{dirty ? t('save') : unavailable ? t('retry') : t('savedValue')}</button>
        <button class={buttonClass('secondary')} disabled={busy || recording} onclick={() => { draft = defaultShortcuts(window.dmsg.platform); error = ''; failureCode = '' }} data-testid="shortcuts-defaults">{t('shortcutDefaults')}</button>
      </div>
    {:else}<button class={buttonClass('secondary')} onclick={load}>{t('retry')}</button>{/if}
  </div>
</Section>
