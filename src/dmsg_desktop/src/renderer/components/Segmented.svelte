<script lang="ts">
  import { ToggleGroup } from 'bits-ui'
  import { indicator } from './motion'
  import { cn } from './ui'

  /**
   * One value from a short, ordered set, such as a number of days, on a
   * bits-ui single toggle group. A pick cannot be cleared, only changed; the
   * thumb slides from the old pick to the new one.
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
  data-roving=""
  class={cn('relative inline-flex shrink-0 gap-0.5 rounded-[10px] border border-edge-soft bg-surface p-0.5', disabled && 'opacity-50', className)}
>
  <span
    aria-hidden="true"
    class="pointer-events-none absolute top-0 left-0 rounded-lg bg-forest transition-[transform,width] duration-220 ease-spring-soft"
    use:indicator={{ selector: value ? `[data-value="${CSS.escape(value)}"]` : null }}
  ></span>
  {#each options as option (option.value)}
    <ToggleGroup.Item
      value={option.value}
      data-value={option.value}
      class="relative h-7 min-w-8 rounded-lg px-2 text-body text-ink-2 tabular-nums outline-none transition-colors duration-120 data-[state=off]:hover:not-disabled:bg-wash focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=on]:font-semibold data-[state=on]:text-on-forest"
    >
      {option.label}
    </ToggleGroup.Item>
  {/each}
</ToggleGroup.Root>
