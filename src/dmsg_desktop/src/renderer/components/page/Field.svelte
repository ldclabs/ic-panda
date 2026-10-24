<script lang="ts">
  import type { Snippet } from 'svelte'
  import { cn } from '../ui'

  /** A form field: its name above the control, and a line of help below. */
  let { label, hint, hintId, class: className, children }: { label: string; hint?: string; hintId?: string; class?: string; children: Snippet } = $props()
</script>

<label class={cn('grid min-w-0 content-start gap-1', className)}>
  <span class="text-[12px] text-faint">{label}</span>
  {@render children()}
  {#if hint}<span id={hintId} class="text-[11.5px] text-faint">{hint}</span>{/if}
</label>
