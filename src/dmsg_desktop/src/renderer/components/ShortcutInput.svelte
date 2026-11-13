<script lang="ts">
  import { onDestroy } from 'svelte'
  import TextField from './TextField.svelte'
  import { recordedShortcut, shortcutLabel } from '../../shared/shortcuts'
  import { t } from '../i18n/index.svelte'
  import { bridge, errorText } from '../rpc'

  let { value, label, disabled = false, error = '', testid, recording = $bindable(false), onChange }: {
    value: string | null; label: string; disabled?: boolean; error?: string; testid: string;
    recording?: boolean; onChange: (value: string | null) => void
  } = $props()
  let armed = $state(false), failure = $state(''), invalid = $state(false)
  let generation = 0, alive = true

  async function start() {
    const turn = ++generation
    recording = true; armed = false; failure = ''; invalid = false
    try {
      await bridge(() => window.dmsg.captureShortcut(true))
      if (alive && turn === generation) armed = true
    } catch (error) {
      if (alive && turn === generation) { recording = false; failure = errorText(error) }
    }
  }

  function stop() {
    generation++; recording = false; armed = false; invalid = false
    void bridge(() => window.dmsg.captureShortcut(false)).catch(error => { if (alive) failure = errorText(error) })
  }

  function capture(event: KeyboardEvent) {
    const input = event.currentTarget as HTMLInputElement
    if (event.key === 'Tab') return
    event.preventDefault(); event.stopPropagation()
    if (event.key === 'Escape') { input.blur(); return }
    if (!armed || event.repeat || /^(Meta|Control|Alt|Shift)$/.test(event.key)) return
    if (['Backspace', 'Delete'].includes(event.key) && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey) {
      onChange(null)
    } else {
      const next = recordedShortcut(event, window.dmsg.platform)
      if (!next) { invalid = true; return }
      onChange(next)
    }
    input.blur()
  }

  onDestroy(() => { alive = false; stop() })
</script>

<svelte:window onblur={stop} />

<TextField {label} {disabled} readonly dir={recording ? 'auto' : 'ltr'} autocomplete="off" data-testid={testid}
  value={recording ? (armed ? t('shortcutRecording') : t('loading')) : shortcutLabel(value, window.dmsg.platform)}
  hint={t('shortcutRecordHint')} error={failure || (invalid ? t('shortcutInvalid') : error)}
  onfocus={start} onblur={stop} onkeydown={capture} />
