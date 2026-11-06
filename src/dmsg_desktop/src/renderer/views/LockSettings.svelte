<script lang="ts">
  import { onMount } from 'svelte'
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

  let lockWithScreen = $state(true)
  let openAtLogin = $state(false)
  let minutes = $state<number | null>(null)
  let busy = $state(false)
  let error = $state('')

  const idle = $derived(minutes ?? Math.round((status?.idle_lock_ms ?? 900_000) / 60_000))
  const options = $derived([...new Set([...CHOICES, idle])].sort((a, b) => a - b).map((n) => ({ value: String(n), label: t('idleLockMinutes', { n }) })))
  const unlocked = $derived(status?.state === 'unlocked')

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  const setIdle = (next: string) =>
    attempt(async () => {
      await bridge(() => window.dmsg.filesPreferences({ idle_lock_minutes: Number(next) }))
      minutes = Number(next)
    })

  const setDesktop = (change: { lock_on_screen_lock?: boolean; open_at_login?: boolean }) =>
    attempt(async () => {
      const next = await bridge(() => window.dmsg.setDesktopSettings(change))
      lockWithScreen = next.lock_on_screen_lock
      openAtLogin = next.open_at_login
    })

  async function readDesktop() {
    const next = await bridge(() => window.dmsg.desktopSettings())
    lockWithScreen = next.lock_on_screen_lock
    openAtLogin = next.open_at_login
  }

  onMount(() => {
    void readDesktop().catch((e) => (error = errorText(e)))
    return window.dmsg.onEvent((event) => {
      if (event.type === 'desktop/changed') void readDesktop().catch(() => {})
    })
  })
</script>

<Section title={t('lockSection')} description={t('lockSectionHint')} icon="lock" testid="lock-settings">
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  <dl>
    <Fact label={t('idleLock')}>
      <Select value={String(idle)} {options} label={t('idleLock')} onChange={setIdle} disabled={busy} testid="idle-lock" class="w-48" />
    </Fact>
    <Fact label={t('lockOnScreenLock')}>
      <Checkbox checked={lockWithScreen} onCheckedChange={(on) => setDesktop({ lock_on_screen_lock: on })} disabled={busy} label={t('lockOnScreenLock')} testid="lock-on-screen-lock">{t('lockOnScreenLockHint')}</Checkbox>
    </Fact>
    <Fact label={t('openAtLogin')}>
      <Checkbox checked={openAtLogin} onCheckedChange={(on) => setDesktop({ open_at_login: on })} disabled={busy} label={t('openAtLogin')} testid="open-at-login">{t('openAtLoginHint')}</Checkbox>
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
          <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => attempt(() => call('session/fast_unlock', { enable: !status.fast_unlock }))} data-testid="fast-unlock-toggle">
            <Icon name="key" />{status.fast_unlock ? t('fastUnlockOff') : t('fastUnlockOn')}
          </button>
        {/if}
        <span class="w-full text-[12px] text-faint">{t('fastUnlockHint')}</span>
      </Fact>
    {/if}
  </dl>
</Section>
