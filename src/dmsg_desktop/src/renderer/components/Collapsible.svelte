<script lang="ts">
  import { Collapsible } from 'bits-ui'
  import type { Snippet } from 'svelte'
  import Icon from './Icon.svelte'

  /** Details shown on request, in place of a native `<details>`: the title toggles the body, which unfolds to its height and folds faster. */
  let {
    open = $bindable(false),
    title,
    testid,
    class: className,
    children
  }: { open?: boolean; title: Snippet; testid?: string; class?: string; children: Snippet } = $props()
</script>

<Collapsible.Root bind:open class={className} data-testid={testid}>
  <Collapsible.Trigger
    class="group flex w-full items-center gap-1.5 rounded-md py-1 text-start text-ink outline-none focus-visible:ring-3 focus-visible:ring-focus/35"
  >
    <Icon
      name="chevron-end"
      class="text-faint transition-transform duration-200 ease-calm group-data-[state=open]:rotate-90 motion-reduce:transition-none rtl:group-data-[state=open]:-rotate-90"
    />
    <span class="min-w-0 flex-1">{@render title()}</span>
  </Collapsible.Trigger>
  <Collapsible.Content
    class="overflow-hidden ps-5.5 ease-calm data-[state=closed]:animate-collapsible-up data-[state=closed]:duration-140 data-[state=open]:animate-collapsible-down data-[state=open]:duration-220"
  >
    {@render children()}
  </Collapsible.Content>
</Collapsible.Root>
