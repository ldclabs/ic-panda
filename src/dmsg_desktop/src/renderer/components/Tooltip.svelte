<script lang="ts">
  import { Tooltip } from 'bits-ui'
  import type { Snippet } from 'svelte'

  /**
   * A short label beside a control, such as an icon-only button; the control
   * renders through `children` with the trigger's props. Needs a
   * `Tooltip.Provider` above it. The first tip of a hover slides 4px out of
   * its control; moving on to the next control shows that one at once.
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
      class="z-50 max-w-xs rounded-md bg-ink px-2 py-1 text-callout text-paper shadow-pop ease-calm data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-100 data-[state=delayed-open]:animate-in data-[state=delayed-open]:fade-in-0 data-[state=delayed-open]:duration-120 data-[side=bottom]:slide-in-from-top-1 data-[side=left]:slide-in-from-right-1 data-[side=right]:slide-in-from-left-1 data-[side=top]:slide-in-from-bottom-1"
    >
      {content}
    </Tooltip.Content>
  </Tooltip.Portal>
</Tooltip.Root>
