<script lang="ts">
  import { RadioGroup } from 'bits-ui'
  import type { Snippet } from 'svelte'
  import { cn } from './ui'

  /** One choice of a bits-ui `RadioGroup.Root`, with its label. */
  let {
    value,
    disabled = false,
    testid,
    class: className,
    children
  }: { value: string; disabled?: boolean; testid?: string; class?: string; children?: Snippet } = $props()
</script>

<label class={cn('inline-flex min-w-0 items-center gap-2 select-none', disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer', className)}>
  <RadioGroup.Item
    {value}
    {disabled}
    data-testid={testid}
    class="group grid size-4 shrink-0 place-items-center rounded-full border border-edge bg-surface outline-none transition-colors duration-150 focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=checked]:border-forest"
  >
    <span class="size-2 scale-0 rounded-full bg-forest transition-transform duration-200 ease-spring group-data-[state=checked]:scale-100"></span>
  </RadioGroup.Item>
  {#if children}<span class="min-w-0">{@render children()}</span>{/if}
</label>
