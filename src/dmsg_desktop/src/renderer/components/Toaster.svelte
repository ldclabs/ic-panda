<script lang="ts">
  import { t } from '../i18n/index.svelte'
  import { isFailure, temporaryFailure } from '../failure'
  import Icon from './Icon.svelte'
  import { leave } from './motion'
  import { dismiss, pauseToasts, resumeToasts, toasts } from './toast.svelte'
  import { buttonClass, iconButtonClass } from './ui'

  /**
   * The main window's notices, newest last, at the bottom of the reading end:
   * a raised card in the window's own colors. A failure carries its mark; the
   * close shows on hover or focus, and resting on a notice holds it. Notices
   * spring in and fade out.
   */
  let now = $state(Date.now())

  $effect(() => {
    if (!toasts.length) return
    now = Date.now()
    const timer = setInterval(() => (now = Date.now()), 250)
    return () => clearInterval(timer)
  })
</script>

<div
  class="pointer-events-none fixed end-4 bottom-24 z-[60] flex w-[min(400px,calc(100vw-32px))] flex-col items-end gap-2"
  role="status"
  aria-live="polite"
  onpointerenter={pauseToasts}
  onpointerleave={resumeToasts}
>
  {#each toasts as item (item.id)}
    {@const left = Math.max(0, Math.ceil((item.until - now) / 1000))}
    {@const text = item.text(left)}
    {@const failed = isFailure(text)}
    <div
      class="group pointer-events-auto flex max-w-full items-center gap-2.5 rounded-xl bg-surface py-2 ps-3 pe-1.5 text-callout text-ink shadow-pop duration-300 ease-spring animate-in fade-in-0 slide-in-from-bottom-2 dark:bg-surface-2"
      data-testid={item.testid}
      out:leave
    >
      {#if item.countdown}
        <span
          class="size-3.5 shrink-0 rounded-full"
          style:background="conic-gradient(var(--forest) {Math.min(1, (item.until - now) / item.ms)}turn, color-mix(in srgb, var(--forest) 20%, transparent) 0)"
          style:mask="radial-gradient(circle 4.5px, transparent 97%, #000 100%)"
          aria-hidden="true"
        ></span>
      {:else if failed}
        <Icon name="warning" class={temporaryFailure(text) ? 'text-warn' : 'text-danger'} />
      {/if}
      <span class="min-w-0 flex-1 py-1 [overflow-wrap:anywhere]">{text}</span>
      {#if item.action}
        <button
          class={buttonClass('ghost', 'xs', 'font-semibold text-forest hover:not-disabled:text-forest')}
          onclick={() => {
            dismiss(item.id)
            item.action?.run()
          }}
          data-testid="toast-action">{item.action.label}</button
        >
      {/if}
      <button
        class={iconButtonClass('ghost', 'md', 'opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100')}
        aria-label={t('dismiss')}
        onclick={() => dismiss(item.id)}><Icon name="close" /></button
      >
    </div>
  {/each}
</div>
