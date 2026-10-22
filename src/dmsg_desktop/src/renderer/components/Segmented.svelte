<script lang="ts">
  import { ToggleGroup } from 'bits-ui'
  import { cn } from './ui'

  /**
   * One value from a short, ordered set, such as a number of days, on a
   * bits-ui single toggle group. A pick cannot be cleared, only changed.
   */
  let {
    value = $bindable(''),
    options,
    label,
    disabled = false,
    testid,
    class: className
  }: {
    value?: string
    options: { value: string; label: string }[]
    label: string
    disabled?: boolean
    testid?: string
    class?: string
  } = $props()
</script>

<ToggleGroup.Root
  type="single"
  bind:value={() => value, (next) => next && (value = next)}
  {disabled}
  aria-label={label}
  data-testid={testid}
  class={cn('inline-flex shrink-0 gap-0.5 rounded-[10px] border border-edge-soft bg-surface p-0.5', disabled && 'opacity-50', className)}
>
  {#each options as option (option.value)}
    <ToggleGroup.Item
      value={option.value}
      data-value={option.value}
      class="h-7 min-w-8 rounded-lg px-2 text-[13px] text-ink-2 tabular-nums outline-none transition-colors duration-150 data-[state=off]:hover:not-disabled:bg-wash focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=on]:bg-forest data-[state=on]:hover:not-disabled:bg-forest-hover data-[state=on]:font-semibold data-[state=on]:text-on-forest"
    >
      {option.label}
    </ToggleGroup.Item>
  {/each}
</ToggleGroup.Root>
