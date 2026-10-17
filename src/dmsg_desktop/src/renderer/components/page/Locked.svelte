<script lang="ts">
  import { t } from '../../i18n/index.svelte'
  import { unlock } from '../../shell/session'
  import Icon from '../Icon.svelte'
  import { buttonClass } from '../ui'

  /** A view the locked device cannot show, with the way to unlock it. */
  let { status, testid }: { status: any; testid?: string } = $props()
</script>

<div class="grid flex-1 place-items-center p-8" data-testid={testid}>
  <div class="grid max-w-sm justify-items-center gap-3 text-center">
    <span class="grid size-12 place-items-center rounded-2xl border border-line bg-surface text-faint shadow-xs [&_svg]:size-6"><Icon name="lock" /></span>
    <p class="text-[13.5px] text-muted">{t('locked')}</p>
    {#if status?.state === 'locked'}
      <button class={buttonClass('primary')} onclick={() => unlock(status)}>{status?.fast_unlock ? t('fastUnlock') : t('login')}</button>
    {/if}
  </div>
</div>
