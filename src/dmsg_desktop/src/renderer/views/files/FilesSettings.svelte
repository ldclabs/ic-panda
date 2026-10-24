<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
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
  /** Remounts the switch after an answer, so a dismissed dialog leaves it as it was. */
  let revision = $state(0)

  const DAYS = [7, 14, 30, 90, 180, 365]
  const options = $derived([...new Set([...DAYS, prefs?.files_trash_days ?? 30])].sort((a, b) => a - b).map((n) => ({ value: String(n), label: t('daysN', { n }) })))

  const load = single(async () => {
    if (!alive) return
    const turn = request
    try {
      const value = await call('files/preferences')
      if (!alive || turn !== request) return
      prefs = value
      error = ''
    } catch (e) {
      if (alive && turn === request) error = errorText(e)
    }
  })

  async function save(changes: { files_mount?: boolean; files_trash_days?: number }) {
    if (busy) return
    const turn = ++request
    busy = true
    try {
      const value = await bridge(() => window.dmsg.filesPreferences(changes))
      if (!alive || turn !== request) return
      request++
      prefs = value ?? prefs
      error = ''
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      busy = false
      revision++
    }
  }

  onMount(() => {
    void load()
    return onChange(['files/changed', 'files/mount_changed'], load)
  })
</script>

{#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
{#if prefs}
  <Section title={t('files')} description={t('filesSettingsHint')} icon="folder" testid="files-settings">
    <div class="grid gap-4">
      <div class="grid gap-1.5">
        {#key revision}
          <Checkbox
            checked={prefs.files_mount}
            disabled={busy || !prefs.supported || prefs.files_mount_config}
            onCheckedChange={(on) => void save({ files_mount: on })}
            testid="files-settings-mount">{t('mountAllowed')}</Checkbox
          >
        {/key}
        <p class="ps-6 text-[12px] text-faint">
          {!prefs.supported ? t('mountUnsupported') : prefs.files_mount_config ? t('mountByConfig') : t('mountAllowedHint')}
        </p>
      </div>
      <div class="grid max-w-xs gap-1 text-[12px] text-faint">
        {t('trashDays')}
        <Select
          bind:value={() => String(prefs.files_trash_days), (next) => void save({ files_trash_days: Number(next) })}
          {options}
          label={t('trashDays')}
          disabled={busy}
          testid="files-settings-trash"
          class="w-full"
        />
      </div>
    </div>
  </Section>
{/if}
