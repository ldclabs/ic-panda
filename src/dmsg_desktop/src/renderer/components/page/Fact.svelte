<script lang="ts">
  import type { Snippet } from 'svelte'

  /** One fact in a definition list: alignment and spacing group its name and value. */
  let { label, testid, children }: { label: string; testid?: string; children: Snippet } = $props()
</script>

<div class="grid gap-x-4 gap-y-0.5 py-2 sm:grid-cols-[144px_1fr] sm:items-center">
  <dt class="text-[12px] text-faint">{label}</dt>
  <dd class="flex min-w-0 flex-wrap items-center gap-1.5 text-[13px] [overflow-wrap:anywhere]" data-testid={testid}>{@render children()}</dd>
</div>
