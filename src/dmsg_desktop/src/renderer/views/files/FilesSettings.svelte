<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Field from '../../components/page/Field.svelte'
  import { buttonClass } from '../../components/ui'
  import Checkbox from '../../components/Checkbox.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, errorText, onChange, single } from '../../rpc'

  /**
   * The file library's settings: whether it may be mounted as a disk and how
   * long the trash keeps files. The main process writes them, asking in a
   * system dialog before allowing the mount; `config.toml` may allow the
   * mount too, which no setting here takes back.
   */
  let alive = true
  let request = 0
  onDestroy(() => { alive = false; request++ })
  let prefs = $state<any>(null)
  let error = $state('')
  let busy = $state(false)
  let open = $state(false)
  let mount = $state(false)
  let days = $state('30')
  const changed = $derived(!!prefs && ((prefs.supported && !prefs.files_mount_config && mount !== prefs.files_mount) || Number(days) !== prefs.files_trash_days))

  function edit() {
    if (!prefs || busy) return
    mount = prefs.files_mount
    days = String(prefs.files_trash_days)
    error = ''
    open = true
  }

  const DAYS = [7, 14, 30, 90, 180, 365]
  const options = $derived([...new Set([...DAYS, prefs?.files_trash_days ?? 30])].sort((a, b) => a - b).map((n) => ({ value: String(n), label: t('daysN', { n }) })))

  const load = single(async () => {
    if (!alive) return
    const turn = request
    try {
      const value = await call('files/preferences')
      if (!alive || turn !== request) return
      if (!open || mount === prefs?.files_mount) mount = value.files_mount
      if (!open || Number(days) === prefs?.files_trash_days) days = String(value.files_trash_days)
      if (!open) error = ''
      prefs = value
    } catch (e) {
      if (alive && turn === request) error = errorText(e)
    }
  })

  async function save() {
    if (busy || !prefs || !changed || !options.some(option => option.value === days)) return
    const changes: { files_mount?: boolean; files_trash_days?: number } = {}
    if (mount !== prefs.files_mount && prefs.supported && !prefs.files_mount_config) changes.files_mount = mount
    if (Number(days) !== prefs.files_trash_days) changes.files_trash_days = Number(days)
    const turn = ++request
    busy = true
    try {
      const value = await bridge(() => window.dmsg.filesPreferences(changes))
      if (!alive || turn !== request) return
      request++
      prefs = value ?? prefs
      if (value) open = false
      error = ''
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      if (alive) busy = false
    }
  }

  onMount(() => {
    void load()
    return onChange(['files/changed', 'files/mount_changed'], load)
  })
</script>

{#if error && !open}<Notice failure={error} tone="danger">{error}
  {#snippet action()}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={load}>{t('retry')}</button>{/snippet}
</Notice>{/if}
<Section title={t('files')} description={t('filesSettingsHint')} icon="folder" testid="files-settings">
  {#if prefs}
    <dl>
      <Fact label={t('mountAllowed')}>{t(prefs.files_mount ? 'on' : 'off')}</Fact>
      <Fact label={t('trashDays')}>{t('daysN', { n: prefs.files_trash_days })}</Fact>
    </dl>
    <button class={buttonClass('secondary', 'sm', 'mt-3')} disabled={busy} onclick={edit} data-testid="files-settings-edit">{t('edit')}</button>
  {:else if !error}<p class="text-[13px] text-muted" role="status">{t('loading')}</p>{/if}
</Section>
<Dialog bind:open title={t('files')} description={t('filesSettingsHint')} icon="folder" dismissible={!busy} testid="files-settings-editor">
  <div class="grid gap-4">
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    <Checkbox bind:checked={mount} disabled={busy || !prefs?.supported || prefs?.files_mount_config} testid="files-settings-mount">{t('mountAllowed')}</Checkbox>
    <p class="text-[12px] text-muted">{!prefs?.supported ? t('mountUnsupported') : prefs.files_mount_config ? t('mountByConfig') : t('mountAllowedHint')}</p>
    <Field label={t('trashDays')}><Select bind:value={days} {options} label={t('trashDays')} disabled={busy} testid="files-settings-trash" class="w-full" /></Field>
  </div>
  {#snippet footer()}
    {#if !changed}<span class="text-[12px] text-faint">{t('securityUnchanged')}</span>{/if}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy} onclick={() => { open = false; error = '' }}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !changed} onclick={save} data-testid="files-settings-save">{t('save')}</button>
  {/snippet}
</Dialog>
