<script lang="ts">
  import { t, type Label } from '../../i18n/index.svelte'
  import { startLogin } from '../../shell/login.svelte'
  import { nextStep } from '../../shell/session'
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
      : step?.label === 'continueSetup'
        ? 'setupHint'
        : locked && !status.fast_unlock
          ? 'lockedNeedsLogin'
          : locked && status.fast_presence
            ? 'lockedPresence'
            : 'locked'
  )
</script>

<div class="grid flex-1 place-items-center p-8" data-testid={testid}>
  <div class="grid max-w-sm justify-items-center gap-3 text-center">
    <span class="grid size-12 place-items-center rounded-2xl border border-line bg-surface text-faint shadow-xs [&_svg]:size-6"><Icon name="lock" /></span>
    <p class="text-[13.5px] text-muted">{t(hint)}</p>
    {#if step}
      <button class={buttonClass('primary')} onclick={step.run} data-testid="locked-step"><Icon name={step.icon} />{t(step.label)}</button>
    {/if}
    {#if locked && status.fast_unlock}
      <button class={buttonClass('ghost', 'sm')} onclick={() => startLogin()} data-testid="locked-login">{t('loginWithIi')}</button>
    {/if}
  </div>
</div>
