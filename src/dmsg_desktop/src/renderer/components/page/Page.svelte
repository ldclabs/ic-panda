<script lang="ts">
  import type { Snippet } from 'svelte'
  import { cn } from '../ui'

  /** A view that reads as one page: its sections in a column that scrolls. */
  let { wide = false, testid, children }: { wide?: boolean; testid?: string; children: Snippet } = $props()
</script>

<div class="h-full overflow-y-auto" data-testid={testid}>
  <div class={cn('mx-auto grid gap-8 px-3 pt-6 pb-10 sm:px-6', wide ? 'max-w-[1040px]' : 'max-w-[860px]')}>
    {@render children()}
  </div>
</div>
