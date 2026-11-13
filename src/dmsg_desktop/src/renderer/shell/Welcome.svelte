<script lang="ts">
  import { isLocale, localeNames, locales } from '../../shared/locales'
  import Icon from '../components/Icon.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Select from '../components/Select.svelte'
  import { toast } from '../components/toast.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { locale, setLocale, t } from '../i18n/index.svelte'
  import { call, errorText } from '../rpc'
  import Moved from '../views/Moved.svelte'
  import Onboarding from '../views/Onboarding.svelte'
  import Brand from './Brand.svelte'
  import { login, startLogin } from './login.svelte'
  import { logout } from './session'

  /**
   * A device without a ready account sees only this: signing in, creating or
   * joining the account (a copy from another Mac can approve this one), and
   * the two choices worth making at the start. Each of them can be changed
   * later in Settings.
   */
  let { status, refresh, finishing, onDone }: { status: any; refresh: () => Promise<void>; finishing: boolean; onDone: () => void } = $props()

  const step = $derived(finishing ? 2 : status?.state === 'logged_out' ? 0 : 1)
  const steps = $derived([t('welcomeStepSignIn'), t('welcomeStepAccount'), t('welcomeStepReady')])

  let busy = $state(false)
  let error = $state('')
  let alerts = $state(false)

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

  const fastUnlock = () => attempt(() => call('session/fast_unlock', { enable: true }))

  /** The first explicit opt-in selects only this account, as the settings' own switch does. */
  const turnOnAlerts = () =>
    attempt(async () => {
      const account = status.account
      const preferences: any = await call('notifications/settings', { account })
      await call('notifications/settings_set', { scope: 'installation', expected_revision: preferences.global.revision, settings: { ...preferences.global, enabled: true } })
      if (preferences.account) await call('notifications/settings_set', { scope: 'account', account, expected_revision: preferences.account.revision, settings: { ...preferences.account, enabled: true } })
      alerts = true
    })

  async function pickLanguage(next: string) {
    if (!isLocale(next) || next === locale()) return
    try {
      await setLocale(next)
    } catch {
      toast(() => t('languageFailed'))
    }
  }
</script>

<div class="flex h-full w-full flex-col overflow-y-auto bg-paper" data-testid="welcome">
  <div class="h-10 shrink-0 [-webkit-app-region:drag]"></div>
  <div class="mx-auto grid w-full max-w-[600px] gap-6 px-6 pb-10">
    <header class="flex items-center gap-2.5">
      <Brand class="h-7 w-6" />
      <span class="me-auto text-xl leading-none font-extrabold tracking-[-0.04em]">dMsg</span>
      {#if step === 0}
        <Select value={locale()} options={locales.map((code) => ({ value: code, label: localeNames[code] }))} label={t('language')} onChange={pickLanguage} testid="welcome-language" class="w-44" />
      {:else if step === 1}
        <button class={buttonClass('ghost', 'sm')} onclick={logout} data-testid="welcome-logout">{t('logout')}</button>
      {/if}
    </header>

    <ol class="flex flex-wrap items-center gap-x-3 gap-y-1 text-[12.5px]" aria-label={t('welcomeSteps')} data-testid="welcome-steps">
      {#each steps as name, index (index)}
        <li class={cn('flex items-center gap-1.5', index === step ? 'font-medium text-ink' : 'text-faint')} aria-current={index === step ? 'step' : undefined}>
          <span class={cn('grid size-5 place-items-center rounded-full text-[11px] tabular-nums', index < step ? 'bg-forest text-on-forest' : index === step ? 'bg-pale text-forest' : 'bg-wash')}>
            {#if index < step}<Icon name="check" class="size-3" />{:else}{index + 1}{/if}
          </span>
          {name}
        </li>
      {/each}
    </ol>

    {#if error}<Notice failure={error} tone="danger" testid="welcome-error">{error}</Notice>{/if}

    {#if step === 0}
      <div class="grid gap-4">
        <h1 class="text-[22px] leading-tight font-semibold">{t('welcomeTitle')}</h1>
        <p class="text-[14px] leading-relaxed text-muted">{t('welcomeBody')}</p>
        <ul class="grid gap-2 text-[13.5px] text-ink-2">
          <li class="flex items-start gap-2.5"><Icon name="vault" class="mt-0.5 shrink-0 text-forest" />{t('welcomeVault')}</li>
          <li class="flex items-start gap-2.5"><Icon name="file" class="mt-0.5 shrink-0 text-forest" />{t('welcomeFiles')}</li>
          <li class="flex items-start gap-2.5"><Icon name="agents" class="mt-0.5 shrink-0 text-forest" />{t('welcomeApprovals')}</li>
        </ul>
        <div class="grid justify-items-start gap-2 pt-2">
          <button class={buttonClass('primary', 'lg')} disabled={login.phase === 'starting'} aria-busy={login.phase === 'starting'} onclick={() => startLogin()} data-testid="welcome-login">
            <Icon name="login" />{t(status?.passkey_available ? 'loginWithPasskey' : 'login')}
          </button>
          {#if status?.passkey_available}
            <button class={buttonClass('ghost', 'sm')} disabled={login.phase === 'starting'} onclick={() => startLogin(false, 'https://dmsg.net', 'ii')} data-testid="welcome-login-ii">{t('loginWithIi')}</button>
          {/if}
          <p class="text-[12.5px] text-faint">{t(status?.passkey_available ? 'passkeyLoginHint' : 'welcomeLoginHint')}</p>
        </div>
      </div>
    {:else if step === 1}
      <Onboarding {status} {refresh} />
      <Moved {status} {refresh} />
    {:else}
      <div class="grid gap-4" data-testid="welcome-finish">
        <h1 class="text-[22px] leading-tight font-semibold">{t('welcomeReadyTitle')}</h1>
        <p class="text-[14px] leading-relaxed text-muted">{t('welcomeReadyBody')}</p>
        <ul class="divide-y divide-line rounded-xl border border-line bg-surface">
          <li class="flex flex-wrap items-center gap-3 px-4 py-3">
            <Icon name="key" class="text-forest" />
            <span class="min-w-40 flex-1">
              <b class="block text-[13.5px] font-medium">{t('fastUnlock')}</b>
              <span class="block text-[12.5px] text-muted">{t('fastUnlockHint')}</span>
            </span>
            {#if status?.fast_unlock}
              <span class="flex items-center gap-1 text-[12.5px] text-forest" data-testid="welcome-fast-on"><Icon name="check" />{t('on')}</span>
            {:else}
              <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={fastUnlock} data-testid="welcome-fast">{t('fastUnlockOn')}</button>
            {/if}
          </li>
          <li class="flex flex-wrap items-center gap-3 px-4 py-3">
            <Icon name="bell" class="text-forest" />
            <span class="min-w-40 flex-1">
              <b class="block text-[13.5px] font-medium">{t('noticeSetupTitle')}</b>
              <span class="block text-[12.5px] text-muted">{t('noticeSetupBody')}</span>
            </span>
            {#if alerts}
              <span class="flex items-center gap-1 text-[12.5px] text-forest" data-testid="welcome-alerts-on"><Icon name="check" />{t('on')}</span>
            {:else}
              <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={turnOnAlerts} data-testid="welcome-alerts">{t('noticeEnable')}</button>
            {/if}
          </li>
        </ul>
        <p class="text-[12.5px] leading-relaxed text-muted" data-testid="welcome-recovery">{t('recoveryPreparation')}</p>
        <button class={buttonClass('primary', 'lg', 'justify-self-start')} onclick={onDone} data-testid="welcome-done">{t('welcomeStart')}<Icon name="chevron-end" /></button>
      </div>
    {/if}
  </div>
</div>
