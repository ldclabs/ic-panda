<script lang="ts">
  import { t } from '../i18n/index.svelte'
  import Icon from './Icon.svelte'
  import { dismiss, toasts } from './toast.svelte'
  import { buttonClass } from './ui'

  /** The main window's notices, newest last, at the bottom of the reading end. */
  let now = $state(Date.now())

  $effect(() => {
    if (!toasts.length) return
    now = Date.now()
    const timer = setInterval(() => (now = Date.now()), 250)
    return () => clearInterval(timer)
  })
</script>

<div class="pointer-events-none fixed end-4 bottom-24 z-50 flex w-[min(420px,calc(100vw-32px))] flex-col items-end gap-2" role="status" aria-live="polite">
  {#each toasts as item (item.id)}
    {@const left = Math.max(0, Math.ceil((item.until - now) / 1000))}
    <div
      class="pointer-events-auto flex max-w-full items-center gap-2.5 rounded-xl bg-ink py-2 ps-3 pe-1.5 text-[12.5px] text-paper shadow-pop duration-300 ease-spring animate-in fade-in-0 slide-in-from-bottom-2"
      data-testid={item.testid}
    >
      {#if item.countdown}
        <span
          class="size-3.5 shrink-0 rounded-full"
          style:background="conic-gradient(var(--paper) {Math.min(1, (item.until - now) / item.ms)}turn, color-mix(in srgb, var(--paper) 25%, transparent) 0)"
          style:mask="radial-gradient(circle 4.5px, transparent 97%, #000 100%)"
          aria-hidden="true"
        ></span>
      {/if}
      <span class="min-w-0 flex-1">{item.text(left)}</span>
      <button
        class={buttonClass('ghost', 'xs', 'text-paper/75 hover:not-disabled:bg-paper/15 hover:not-disabled:text-paper')}
        aria-label={t('dismiss')}
        onclick={() => dismiss(item.id)}><Icon name="close" /></button>
    </div>
  {/each}
</div>
