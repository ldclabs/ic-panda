<script lang="ts">
  import { t, type Label } from '../../i18n/index.svelte'
  import { startLogin } from '../../shell/login.svelte'
  import { nextStep, unlocking } from '../../shell/session'
  import Icon from '../Icon.svelte'
  import { buttonClass } from '../ui'

  /** A view the device cannot show yet, with the step that opens it: a login, an unlock or the setup. */
  let { status, testid }: { status: any; testid?: string } = $props()

  const step = $derived(nextStep(status))
  const locked = $derived(status?.state === 'locked')
  // Without fast unlock a locked account opens only with a login; the hint says where to turn it on.
  const hint = $derived<Label>(
    status?.state === 'logged_out'
      ? 'loggedOutHint'
      : status?.state === 'moved'
        ? 'movedHint'
        : locked && !status.fast_unlock && !status.login_renewable
          ? 'lockedNeedsLogin'
          : locked && status.fast_presence
            ? 'lockedPresence'
            : 'locked'
  )
</script>

<div class="grid flex-1 place-items-center p-8" data-testid={testid}>
  <div class="grid max-w-sm justify-items-center gap-3 text-center">
    <svg viewBox="0 0 24 24" class="lock mb-1 size-10 overflow-visible text-forest" data-open={$unlocking || undefined} fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path class="lock-shackle" d="M7.5 10V7.5a4.5 4.5 0 0 1 9 0V10" /><rect x="4.5" y="10" width="15" height="11" rx="2" /><path d="M12 14.5v2.5" />
    </svg>
    <p class="text-body text-muted" role="status">{$unlocking ? t('unlocking') : t(hint)}</p>
    {#if step}
      <button class={buttonClass('primary')} onclick={step.run} disabled={$unlocking} aria-busy={$unlocking} data-testid="locked-step"><Icon name={$unlocking ? 'pending' : step.icon} class={$unlocking ? 'motion-safe:animate-spin' : ''} />{$unlocking ? t('unlocking') : t(step.label)}</button>
    {/if}
    {#if locked && status.fast_unlock}
      <button class={buttonClass('ghost', 'sm')} disabled={$unlocking} onclick={() => startLogin()} data-testid="locked-login">{t(status?.passkey_available ? 'loginWithPasskey' : 'loginWithIi')}</button>
    {/if}
  </div>
</div>
