<script lang="ts">
  import type { Snippet } from 'svelte'
  import { cn } from '../ui'

  /**
   * Settings rows in one card, as the system's settings group them: a
   * Surface with a rule between rows. Reading content stays unframed.
   */
  let { class: className, testid, children }: { class?: string; testid?: string; children: Snippet } = $props()
</script>

<dl class={cn('divide-y divide-line rounded-xl border border-line bg-surface px-4 [&>*]:py-2.5', className)} data-testid={testid}>
  {@render children()}
</dl>
