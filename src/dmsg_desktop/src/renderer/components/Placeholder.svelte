<script lang="ts">
  import { t } from '../i18n/index.svelte'

  /**
   * Rows that stand in for a list during its first read. They appear after
   * 300 ms, so a quick read never flashes them, and hold still under reduced
   * motion; the status says it is loading either way.
   */
  let { rows = 5, testid }: { rows?: number; testid?: string } = $props()
  let shown = $state(false)

  $effect(() => {
    const timer = setTimeout(() => (shown = true), 300)
    return () => clearTimeout(timer)
  })
</script>

<li class="list-none" data-testid={testid}>
  <span class="sr-only" role="status">{t('loading')}</span>
  {#if shown}
    <div class="grid gap-0.5" aria-hidden="true">
      {#each { length: rows } as _, i (i)}
        <div class="flex items-center gap-2.5 px-2 py-2">
          <span class="size-8 shrink-0 rounded-lg bg-wash motion-safe:animate-pulse"></span>
          <span class="grid flex-1 gap-1.5">
            <span class="h-3 rounded bg-wash motion-safe:animate-pulse" style:width={`${64 - (i % 3) * 14}%`}></span>
            <span class="h-2.5 w-1/3 rounded bg-wash motion-safe:animate-pulse"></span>
          </span>
        </div>
      {/each}
    </div>
  {/if}
</li>
