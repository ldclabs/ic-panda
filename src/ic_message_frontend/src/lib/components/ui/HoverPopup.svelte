<script lang="ts">
  import { Tooltip } from 'bits-ui'

  /**
   * The hover half of Skeleton's `popup` utility, on the bits-ui tooltip.
   * Used for the "show me the full value" bubbles next to truncated
   * principals and canister ids.
   */
  interface Props {
    contentClass?: string
    side?: 'top' | 'bottom' | 'left' | 'right'
    delayDuration?: number
    /**
     * Renders the trigger. Spread the given props onto your own element —
     * that element becomes the anchor.
     */
    trigger: import('svelte').Snippet<[Record<string, unknown>]>
    content: import('svelte').Snippet
  }

  let {
    contentClass = '',
    side = 'top',
    delayDuration = 150,
    trigger,
    content
  }: Props = $props()
</script>

<!-- The provider lives in the app layout so every tooltip shares one; this
     `delayDuration` is a per-root override of it. -->
<Tooltip.Root {delayDuration}>
  <!-- `child` lets the caller own the trigger element, so a popup around an
         existing button does not end up nesting one button inside another. -->
  <Tooltip.Trigger>
    {#snippet child({ props })}
      {@render trigger(props)}
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Portal>
    <Tooltip.Content
      {side}
      sideOffset={8}
      class="card bg-surface-800 z-[10001] max-w-80 px-2 py-1 text-white shadow-lg {contentClass}"
    >
      {@render content()}
      <Tooltip.Arrow class="text-surface-800" />
    </Tooltip.Content>
  </Tooltip.Portal>
</Tooltip.Root>
