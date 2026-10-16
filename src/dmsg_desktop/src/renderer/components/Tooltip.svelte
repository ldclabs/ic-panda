<script lang="ts">
  import { Tooltip } from 'bits-ui'
  import type { Snippet } from 'svelte'

  /**
   * A short label beside a control, such as an icon-only button; the control
   * renders through `children` with the trigger's props. Needs a
   * `Tooltip.Provider` above it.
   */
  let {
    content,
    side = 'top',
    disabled = false,
    children
  }: {
    content: string
    side?: 'top' | 'right' | 'bottom' | 'left'
    disabled?: boolean
    children: Snippet<[Record<string, unknown>]>
  } = $props()
</script>

<Tooltip.Root {disabled}>
  <Tooltip.Trigger>
    {#snippet child({ props })}{@render children(props)}{/snippet}
  </Tooltip.Trigger>
  <Tooltip.Portal>
    <Tooltip.Content
      {side}
      sideOffset={8}
      collisionPadding={8}
      class="z-50 max-w-xs rounded-md bg-ink px-2 py-1 text-xs text-paper shadow-pop data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=delayed-open]:animate-in data-[state=delayed-open]:fade-in-0 data-[state=instant-open]:animate-in data-[state=instant-open]:fade-in-0"
    >
      {content}
    </Tooltip.Content>
  </Tooltip.Portal>
</Tooltip.Root>
