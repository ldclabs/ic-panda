<script lang="ts">
  import type { Snippet } from 'svelte'
  import { cn } from '../ui'

  /** A form field: its name above the control, and a line of help below. */
  let { label, hint, hintId, error, errorId, for: controlId, hideLabel = false, class: className, children }: { label: string; hint?: string; hintId?: string; error?: string; errorId?: string; for?: string; hideLabel?: boolean; class?: string; children: Snippet } = $props()
</script>

<svelte:element this={controlId ? 'div' : 'label'} class={cn('grid min-w-0 content-start gap-1.5', className)}>
  {#if controlId}<label for={controlId} class={hideLabel ? 'sr-only' : 'text-[12px] text-muted'}>{label}</label>{:else}<span class="text-[12px] text-muted">{label}</span>{/if}
  {@render children()}
  {#if hint}<span id={hintId} class="text-[11.5px] text-faint">{hint}</span>{/if}
  {#if error}<span id={errorId} class="text-[12px] text-danger" role="alert">{error}</span>{/if}
</svelte:element>
