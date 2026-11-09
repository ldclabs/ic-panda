<script lang="ts">
  import { onMount } from 'svelte'
  import { copyText } from '../components/clipboard'
  import Dialog from '../components/Dialog.svelte'
  import type { Glyph } from '../components/glyphs'
  import Icon from '../components/Icon.svelte'
  import Notice from '../components/page/Notice.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { locale, t, type Label } from '../i18n/index.svelte'
  import { onChange } from '../rpc'
  import { temporaryFailure } from '../failure'
  import { checkLogin, closeLogin, login, reopenLogin, startLogin } from './login.svelte'

  /**
   * Passkeys first offer existing login or explicit creation. Authentication
   * then follows three steps: open the page, authenticate, check the result.
   * An unconfirmed creation offers login with the saved credential. Closing
   * leaves a login finished in the browser later to open the device all the same.
   */
  const steps: { label: Label; phase: 'starting' | 'browser' | 'checking' }[] = [
    { label: 'loginStepOpen', phase: 'starting' },
    { label: 'loginStepIdentity', phase: 'browser' },
    { label: 'loginStepFinish', phase: 'checking' }
  ]

  const current = $derived(steps.findIndex((step) => step.phase === login.phase))
  const until = $derived(login.expiresAt ? new Date(login.expiresAt).toLocaleTimeString(locale(), { hour: 'numeric', minute: '2-digit' }) : '')
  const title = $derived(login.phase === 'failed' ? t('loginFailedTitle') : login.phase === 'choose' ? t(login.bind ? 'bindPasskey' : 'loginWithPasskey') : login.action === 'logout_all' ? t('logoutAllLogins') : login.bind ? t('bindLogin') : t('loginWaitTitle'))
  const icon = $derived<Glyph>(login.phase === 'failed' ? 'warning' : login.phase === 'choose' ? 'key' : 'login')

  onMount(() => onChange(['login/changed', 'session/changed'], checkLogin))
</script>

<Dialog
  bind:open={() => login.phase !== 'idle', (next) => !next && closeLogin()}
  {title}
  {icon}
  tone={login.phase === 'failed' && !temporaryFailure(t(login.reason)) ? 'danger' : 'forest'}
  testid="login-dialog"
>
  {#if login.phase === 'choose'}
    <div class="grid gap-5" data-testid="passkey-choices">
      <div class="grid justify-items-start gap-2.5">
        <p class="text-[13px] leading-relaxed text-muted">{t(login.bind ? 'passkeyBindingHint' : 'passkeyExistingHint')}</p>
        <button class={buttonClass('primary', 'md', 'h-auto min-h-8 whitespace-normal py-1.5 text-start')} onclick={() => startLogin(login.bind, login.origin, 'passkey', 'login')} data-testid="passkey-login"><Icon name="key" />{t('passkeyExistingTitle')}</button>
      </div>
      <div class="grid justify-items-start gap-2.5 border-t border-line pt-5">
        <h3 class="text-[14px] font-semibold">{t('createPasskey')}</h3>
        {#if !login.bind}<p class="text-[13px] leading-relaxed text-muted">{t('passkeyCreateHint')}</p>{/if}
        <button class={buttonClass('secondary', 'md', 'h-auto min-h-8 whitespace-normal py-1.5 text-start')} onclick={() => startLogin(login.bind, login.origin, 'passkey', 'register')} data-testid="passkey-create"><Icon name="add" />{t('createPasskey')}</button>
      </div>
    </div>
  {:else if login.phase === 'failed'}
    <Notice failure={t(login.reason)} tone="danger" testid="login-error">
      {t(login.reason)}
      {#if login.detail}<span class="mt-1 block font-mono text-[11.5px] break-words text-muted">{login.detail}</span>{/if}
    </Notice>
  {:else}
    <ol class="grid gap-2.5" data-testid="login-steps" data-phase={login.phase}>
      {#each steps as step, index (step.phase)}
        <li class={cn('flex items-center gap-2.5 text-[13px]', index > current ? 'text-faint' : 'text-ink')}>
          <span
            class={cn(
              'grid size-6 shrink-0 place-items-center rounded-full border text-[11.5px] font-semibold tabular-nums',
              index < current ? 'border-forest bg-forest text-on-forest' : index === current ? 'border-forest text-forest' : 'border-line'
            )}
          >
            {#if index < current}<Icon name="check" class="size-3.5" />{:else}{index + 1}{/if}
          </span>
          <span class={cn(index === current && 'shimmer font-medium')}>{t(index === 1 && login.provider === 'passkey' ? 'passkeyStep' : step.label)}</span>
        </li>
      {/each}
    </ol>
    {#if login.phase === 'browser'}
      <p class="mt-4 text-[13px] leading-relaxed text-ink-2">{login.action === 'logout_all' ? t('logoutAllLoginsHint') : login.provider === 'passkey' ? t('passkeyWaitBody') : login.bind ? t('loginBindBody') : t('loginWaitBody')}</p>
      {#if until}<p class="mt-1.5 text-[12px] text-faint">{t('loginValidUntil', { time: until })}</p>{/if}
    {:else if login.phase === 'checking'}
      <p class="mt-4 text-[13px] leading-relaxed text-ink-2">{t('loginCheckingBody')}</p>
    {/if}
  {/if}
  {#snippet footer()}
    {#if login.phase === 'choose'}
      <button class={buttonClass('ghost', 'sm', 'h-auto min-h-8 whitespace-normal py-1.5 text-start')} onclick={() => startLogin(login.bind, login.origin, 'ii')} data-testid="passkey-login-ii">{t('loginWithIi')}</button>
      <span class="flex-1"></span>
      <button class={buttonClass('secondary', 'sm')} onclick={closeLogin}>{t('close')}</button>
    {:else if login.phase === 'browser'}
      <button class={buttonClass('ghost', 'sm')} onclick={() => copyText(login.url)} data-testid="login-copy"><Icon name="copy" />{t('copyLink')}</button>
      <span class="flex-1"></span>
      <button class={buttonClass('secondary', 'sm')} onclick={reopenLogin} data-testid="login-reopen"><Icon name="external" />{t('loginReopen')}</button>
    {:else if login.phase === 'failed'}
      {#if login.provider === 'passkey' && login.action === 'login' && login.detail === 'SESSION_LIMIT_EXCEEDED'}
        <button class={buttonClass('secondary', 'sm')} onclick={() => startLogin(false, 'https://dmsg.net', 'passkey', 'logout_all')} data-testid="login-reset-devices">{t('logoutAllLogins')}</button>
      {/if}
      <span class="flex-1"></span>
      <button class={buttonClass('secondary', 'sm')} onclick={closeLogin}>{t('close')}</button>
      {#if login.reason === 'passkeyRegistrationUnconfirmed'}
        <button class={buttonClass('primary', 'sm', 'h-auto min-h-8 whitespace-normal py-1.5 text-start')} onclick={() => startLogin(login.bind, login.origin, 'passkey', 'login')} data-testid="login-use-saved"><Icon name="key" />{t('passkeyExistingTitle')}</button>
      {:else}
        <button class={buttonClass('primary', 'sm')} onclick={() => startLogin(login.bind, login.origin, login.provider, login.action)} data-testid="login-retry"><Icon name="sync-now" />{t('retry')}</button>
      {/if}
    {:else}
      <span class="flex-1"></span>
      <button class={buttonClass('secondary', 'sm')} onclick={closeLogin}>{t('close')}</button>
    {/if}
  {/snippet}
</Dialog>
