<script lang="ts">
  import { Checkbox } from 'bits-ui'
  import type { Snippet } from 'svelte'

  /** A bits-ui checkbox with its label; clicking the label toggles it. */
  let {
    checked = $bindable(false),
    disabled = false,
    children
  }: { checked?: boolean; disabled?: boolean; children: Snippet } = $props()
</script>

<label
  class={[
    'flex items-start gap-3 text-[15px] leading-normal text-[var(--ink)]',
    disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer'
  ]}
>
  <Checkbox.Root
    bind:checked
    {disabled}
    class="mt-0.5 grid size-5 shrink-0 place-items-center rounded-[5px] border! border-[#607566]! bg-white! text-white! data-[state=checked]:border-[var(--action)]! data-[state=checked]:bg-[var(--action)]!"
  >
    {#snippet children({ checked })}
      {#if checked}<svg
          viewBox="0 0 24 24"
          class="size-3.5"
          fill="none"
          stroke="currentColor"
          stroke-width="3"
          aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg
        >{/if}
    {/snippet}
  </Checkbox.Root>
  <span>{@render children()}</span>
</label>
