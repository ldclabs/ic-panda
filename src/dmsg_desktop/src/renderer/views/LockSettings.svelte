<script lang="ts">
  import { onMount } from 'svelte'
  import type { DesktopSettings } from '../../shared/contract'
  import Checkbox from '../components/Checkbox.svelte'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import Select from '../components/Select.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, errorText } from '../rpc'

  /**
   * When this device closes the account: after being left idle, with the
   * screen, and how quickly it opens again. The idle time is the daemon's; the
   * screen lock is this app's own, since only it hears the system.
   */
  let { status }: { status: any } = $props()

  const CHOICES = [5, 15, 30, 60, 240]

  let desktop = $state<DesktopSettings | null>(null)
  let minutes = $state<number | null>(null)
  let busy = $state(false), reading = $state(true)
  let error = $state(''), readError = $state('')
  let alive = true, revision = 0
  const daemonMinutes = $derived(Math.round((status?.idle_lock_ms ?? 900_000) / 60_000))
  $effect(() => { void daemonMinutes; minutes = null })
  const idle = $derived(minutes ?? daemonMinutes)
  const options = $derived([...new Set([...CHOICES, idle])].sort((a, b) => a - b).map((n) => ({ value: String(n), label: t('idleLockMinutes', { n }) })))
  const unlocked = $derived(status?.state === 'unlocked')

  async function attempt(work: () => Promise<void>) {
    if (busy) return
    busy = true
    error = ''
    try { await work() }
    catch (cause) { if (alive) error = errorText(cause) }
    finally { if (alive) busy = false }
  }
  function setIdle(next: string) {
    return attempt(async () => {
      const before = idle
      minutes = Number(next)
      try { await bridge(() => window.dmsg.filesPreferences({ idle_lock_minutes: Number(next) })) }
      catch (cause) { if (alive) minutes = before; throw cause }
    })
  }
  function setDesktop(change: Partial<DesktopSettings>) {
    return attempt(async () => {
      if (!desktop) return
      const before = desktop
      revision++
      desktop = { ...before, ...change }
      try {
        const next = await bridge<DesktopSettings>(() => window.dmsg.setDesktopSettings(change))
        if (alive) desktop = next
      } catch (cause) {
        if (alive) desktop = before
        throw cause
      }
    })
  }
  async function readDesktop() {
    const turn = ++revision
    reading = true
    try {
      const next = await bridge<DesktopSettings>(() => window.dmsg.desktopSettings())
      if (alive && turn === revision) { desktop = next; readError = '' }
    } catch (cause) { if (alive && turn === revision) readError = errorText(cause) }
    finally { if (alive && turn === revision) reading = false }
  }
  onMount(() => {
    void readDesktop()
    const off = window.dmsg.onEvent(event => { if (event.type === 'desktop/changed') void readDesktop() })
    return () => { alive = false; revision++; off() }
  })
</script>

<Section title={t('lockSection')} description={t('lockSectionHint')} icon="lock" testid="lock-settings">
  {#if readError}<Notice tone="danger" testid="lock-read-error">{readError}
    <button class={buttonClass('ghost', 'xs')} disabled={reading} onclick={readDesktop}>{t('retry')}</button></Notice>{/if}
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  <dl>
    <Fact label={t('idleLock')}>
      <Select value={String(idle)} {options} label={t('idleLock')} onChange={setIdle} disabled={busy} testid="idle-lock" class="w-48" />
    </Fact>
    <Fact label={t('lockOnScreenLock')}>
      {#if desktop}<Checkbox checked={desktop.lock_on_screen_lock} onCheckedChange={(on) => setDesktop({ lock_on_screen_lock: on })} disabled={busy || reading || !!readError} label={t('lockOnScreenLock')} testid="lock-on-screen-lock">{t('lockOnScreenLockHint')}</Checkbox>{:else if !readError}<span class="text-muted text-xs" role="status">{t('loading')}</span>{/if}
    </Fact>
    <Fact label={t('openAtLogin')}>
      {#if desktop}<Checkbox checked={desktop.open_at_login} onCheckedChange={(on) => setDesktop({ open_at_login: on })} disabled={busy || reading || !!readError} label={t('openAtLogin')} testid="open-at-login">{t('openAtLoginHint')}</Checkbox>{:else if !readError}<span class="text-muted text-xs" role="status">{t('loading')}</span>{/if}
    </Fact>
    {#if status?.account}
      <Fact label={t('fastUnlock')} testid="fast-unlock">
        <Pill tone={status.fast_unlock ? 'forest' : 'neutral'}>{status.fast_unlock ? t('on') : t('off')}</Pill>
        {#if status.fast_unlock}
          <span class="text-[12.5px]" data-testid="fast-unlock-method">
            {t(status.fast_presence === 'biometrics' ? 'presenceBiometrics' : status.fast_presence === 'password' ? 'presencePassword' : 'presenceKeychain')}
          </span>
        {/if}
        <!-- Turning it on needs the open account's keys; its key goes to the OS keychain. -->
        {#if unlocked}
          <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => attempt(async () => { await call('session/fast_unlock', { enable: !status.fast_unlock }) })} data-testid="fast-unlock-toggle">
            <Icon name="key" />{status.fast_unlock ? t('fastUnlockOff') : t('fastUnlockOn')}
          </button>
        {/if}
        <span class="w-full text-[12px] text-faint">{t('fastUnlockHint')}</span>
      </Fact>
    {/if}
  </dl>
</Section>
