<script lang="ts">
  import { Checkbox } from 'bits-ui'
  import type { Snippet } from 'svelte'
  import Icon from './Icon.svelte'
  import { cn } from './ui'

  /** A bits-ui checkbox with its label; clicking the label toggles it. */
  let {
    checked = $bindable(false),
    disabled = false,
    onCheckedChange,
    testid,
    class: className,
    children
  }: {
    checked?: boolean
    disabled?: boolean
    onCheckedChange?: (checked: boolean) => void
    testid?: string
    class?: string
    children?: Snippet
  } = $props()
</script>

<label class={cn('inline-flex items-center gap-2 select-none', disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer', className)}>
  <Checkbox.Root
    bind:checked
    {disabled}
    {onCheckedChange}
    data-testid={testid}
    class="grid size-4 shrink-0 place-items-center rounded-[5px] border border-edge bg-surface text-on-forest outline-none transition-colors duration-150 focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=checked]:border-forest data-[state=checked]:bg-forest"
  >
    {#snippet children({ checked })}
      {#if checked}<Icon name="check" class="size-3.5" />{/if}
    {/snippet}
  </Checkbox.Root>
  {#if children}<span>{@render children()}</span>{/if}
</label>
