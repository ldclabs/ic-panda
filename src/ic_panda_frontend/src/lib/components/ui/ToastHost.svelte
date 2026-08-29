<script lang="ts">
  import { getToastStore } from '$lib/ui/stores'

  const toastStore = getToastStore()

  function tone(background?: string) {
    if (background?.includes('error'))
      return 'border-red-300 bg-red-50 text-red-900'
    if (background?.includes('warning'))
      return 'border-amber-300 bg-amber-50 text-amber-900'
    return 'border-panda/30 bg-white text-black'
  }
</script>

<div
  class="pointer-events-none fixed right-4 bottom-4 z-[10000] flex w-[min(36rem,calc(100vw-2rem))] flex-col gap-2"
  aria-live="polite"
  aria-atomic="false"
>
  {#each $toastStore as toast (toast.id)}
    <div
      class="pointer-events-auto flex items-start gap-3 rounded-xl border px-4 py-3 shadow-lg {tone(
        toast.background
      )}"
      role={toast.background?.includes('error') ? 'alert' : 'status'}
    >
      <p class="min-w-0 flex-1 text-sm break-words whitespace-pre-line">
        {toast.message}
      </p>
      {#if !toast.hideDismiss}
        <button
          class="shrink-0 text-lg leading-none opacity-60 hover:opacity-100"
          aria-label="Dismiss notification"
          on:click={() => toastStore.close(toast.id!)}>&times;</button
        >
      {/if}
    </div>
  {/each}
</div>
