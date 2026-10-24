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
   * The login under way, in three steps: the page opens in the browser, the
   * person logs in there, and the daemon checks what came back. A failure
   * says why and offers to start again. Closing leaves a login finished in
   * the browser later to open the device all the same.
   */
  const steps: { label: Label; phase: 'starting' | 'browser' | 'checking' }[] = [
    { label: 'loginStepOpen', phase: 'starting' },
    { label: 'loginStepIdentity', phase: 'browser' },
    { label: 'loginStepFinish', phase: 'checking' }
  ]

  const current = $derived(steps.findIndex((step) => step.phase === login.phase))
  const until = $derived(login.expiresAt ? new Date(login.expiresAt).toLocaleTimeString(locale(), { hour: 'numeric', minute: '2-digit' }) : '')
  const title = $derived(login.phase === 'failed' ? t('loginFailedTitle') : login.bind ? t('bindLogin') : t('loginWaitTitle'))
  const icon = $derived<Glyph>(login.phase === 'failed' ? 'warning' : 'login')

  onMount(() => onChange(['login/changed', 'session/changed'], checkLogin))
</script>

<Dialog
  bind:open={() => login.phase !== 'idle', (next) => !next && closeLogin()}
  {title}
  {icon}
  tone={login.phase === 'failed' && !temporaryFailure(t(login.reason)) ? 'danger' : 'forest'}
  testid="login-dialog"
>
  {#if login.phase === 'failed'}
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
          <span class={cn(index === current && 'shimmer font-medium')}>{t(step.label)}</span>
        </li>
      {/each}
    </ol>
    {#if login.phase === 'browser'}
      <p class="mt-4 text-[13px] leading-relaxed text-ink-2">{login.bind ? t('loginBindBody') : t('loginWaitBody')}</p>
      {#if until}<p class="mt-1.5 text-[12px] text-faint">{t('loginValidUntil', { time: until })}</p>{/if}
    {:else if login.phase === 'checking'}
      <p class="mt-4 text-[13px] leading-relaxed text-ink-2">{t('loginCheckingBody')}</p>
    {/if}
  {/if}
  {#snippet footer()}
    {#if login.phase === 'browser'}
      <button class={buttonClass('ghost', 'sm')} onclick={() => copyText(login.url)} data-testid="login-copy"><Icon name="copy" />{t('copyLink')}</button>
      <span class="flex-1"></span>
      <button class={buttonClass('secondary', 'sm')} onclick={reopenLogin} data-testid="login-reopen"><Icon name="external" />{t('loginReopen')}</button>
    {:else if login.phase === 'failed'}
      <span class="flex-1"></span>
      <button class={buttonClass('secondary', 'sm')} onclick={closeLogin}>{t('close')}</button>
      <button class={buttonClass('primary', 'sm')} onclick={() => startLogin(login.bind, login.origin)} data-testid="login-retry"><Icon name="sync-now" />{t('retry')}</button>
    {:else}
      <span class="flex-1"></span>
      <button class={buttonClass('secondary', 'sm')} onclick={closeLogin}>{t('close')}</button>
    {/if}
  {/snippet}
</Dialog>
