<script lang="ts">
  import { Checkbox } from 'bits-ui'
  import type { Snippet } from 'svelte'
  import { cn } from './ui'

  /**
   * A bits-ui checkbox with its label; clicking the label toggles it. The
   * fill grows from the centre and the tick draws (`.check-box` in `app.css`).
   * A setting that is on or off and applies at once is a `Switch` instead.
   */
  let {
    checked = $bindable(false),
    disabled = false,
    onCheckedChange,
    label,
    descriptionId,
    testid,
    class: className,
    children
  }: {
    checked?: boolean
    disabled?: boolean
    onCheckedChange?: (checked: boolean) => void
    label?: string
    descriptionId?: string
    testid?: string
    class?: string
    children?: Snippet
  } = $props()
</script>

<label class={cn('inline-flex items-center gap-2 select-none', disabled && 'cursor-not-allowed opacity-60', className)}>
  <Checkbox.Root
    bind:checked
    {disabled}
    {onCheckedChange}
    data-testid={testid}
    aria-label={label}
    aria-describedby={descriptionId}
    class="check-box relative grid size-4 shrink-0 place-items-center rounded-[5px] border border-edge bg-surface text-on-forest outline-none transition-colors duration-120 focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=checked]:border-forest"
  >
    <svg viewBox="0 0 16 16" class="relative size-3.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3.6 8.4 6.6 11.3 12.4 4.9" pathLength="1" /></svg>
  </Checkbox.Root>
  {#if children}<span>{@render children()}</span>{/if}
</label>
