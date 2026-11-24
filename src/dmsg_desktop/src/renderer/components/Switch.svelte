<script lang="ts">
  import { Switch } from 'bits-ui'
  import type { Snippet } from 'svelte'
  import { cn } from './ui'

  /**
   * A setting that is on or off and takes effect at once, as in macOS
   * settings: its words first, the switch at the row's end; clicking the words
   * flips it. The thumb slides across (`.switch` in `app.css`). Options sent
   * with a form, several choices and confirmations stay `Checkbox`.
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

<label class={cn('flex w-full min-w-0 items-center justify-between gap-3 select-none', disabled && 'cursor-not-allowed opacity-60', className)}>
  {#if children}<span class="min-w-0">{@render children()}</span>{/if}
  <Switch.Root
    bind:checked
    {disabled}
    {onCheckedChange}
    data-testid={testid}
    aria-label={label}
    aria-describedby={descriptionId}
    class="switch outline-none focus-visible:ring-3 focus-visible:ring-focus/35"
  >
    <Switch.Thumb class="switch-thumb" />
  </Switch.Root>
</label>
