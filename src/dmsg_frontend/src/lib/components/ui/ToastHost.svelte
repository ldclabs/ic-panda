<script lang="ts">
  import { t } from '$lib/i18n'
  import { getToastStore, type ToastSettings } from '$lib/ui/stores'

  const toastStore = getToastStore()

  /**
   * Skeleton passed the tone as a `variant-*` class name. Those classes still
   * exist in `app.css`, so we can hand them straight to the element.
   */
  function tone(toast: ToastSettings): string {
    return toast.background ?? 'variant-filled-secondary'
  }

  function isAlert(toast: ToastSettings): boolean {
    return toast.background?.includes('error') === true
  }
</script>

<div
  class="pointer-events-none fixed right-4 bottom-4 z-[10000] flex w-full max-w-xl flex-col gap-2 pl-4"
  aria-live="polite"
  aria-atomic="false"
>
  {#each $toastStore as toast (toast.id)}
    <div
      class="card pointer-events-auto flex items-center gap-4 px-4 py-3 shadow-xl {tone(
        toast
      )} {toast.classes ?? ''}"
      role={isAlert(toast) ? 'alert' : 'status'}
      onmouseenter={() => toast.hoverable && toastStore.pause(toast.id!)}
      onmouseleave={() => toast.hoverable && toastStore.resume(toast.id!)}
    >
      <p class="min-w-0 flex-1 text-sm break-words whitespace-pre-line">
        {toast.message}
      </p>
      {#if toast.action}
        <button
          class="btn btn-sm variant-filled shrink-0"
          onclick={() => {
            toast.action?.response()
            toastStore.close(toast.id!)
          }}>{toast.action.label}</button
        >
      {/if}
      {#if !toast.hideDismiss}
        <button
          class="shrink-0 text-lg leading-none opacity-60 hover:opacity-100"
          aria-label={$t('Dismiss notification')}
          onclick={() => toastStore.close(toast.id!)}>&times;</button
        >
      {/if}
    </div>
  {/each}
</div>
