<script lang="ts">
  import { Select } from 'bits-ui'

  /** A choice from a list on the bits-ui select, in place of a native `<select>`. */
  let {
    value = $bindable(''),
    options,
    label,
    mono = false,
    disabled = false
  }: {
    value?: string
    options: string[]
    label: string
    mono?: boolean
    disabled?: boolean
  } = $props()
</script>

<Select.Root
  type="single"
  bind:value
  {disabled}
  items={options.map((o) => ({ value: o, label: o }))}
>
  <Select.Trigger
    aria-label={label}
    class={[
      'flex h-11 w-full items-center justify-between gap-3 rounded-lg! border! border-[#607566]! bg-white! px-3 text-start text-[15px] data-[state=open]:border-[var(--action)]!',
      mono && 'font-mono text-[14px]'
    ]}
  >
    <span class="truncate">{value}</span>
    <svg
      viewBox="0 0 24 24"
      class="size-4 shrink-0 text-[var(--muted)]"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      aria-hidden="true"><path d="M6 9l6 6 6-6" /></svg
    >
  </Select.Trigger>
  <Select.Portal>
    <Select.Content
      sideOffset={6}
      class="z-50 max-h-72 w-(--bits-select-anchor-width) overflow-y-auto rounded-xl border border-[#dae3d9] bg-white p-1 text-[#10251f] shadow-[0_12px_32px_rgb(16_37_31/0.14)]"
    >
      {#each options as option (option)}
        <Select.Item
          value={option}
          label={option}
          class={[
            'flex cursor-default items-center justify-between gap-3 rounded-lg px-3 py-2 text-[14px] outline-none select-none data-highlighted:bg-[#e8f0e9] data-selected:font-semibold data-selected:text-[#145c45]',
            mono && 'font-mono'
          ]}
        >
          {#snippet children({ selected })}
            {option}
            {#if selected}<svg
                viewBox="0 0 24 24"
                class="size-4"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg
              >{/if}
          {/snippet}
        </Select.Item>
      {/each}
    </Select.Content>
  </Select.Portal>
</Select.Root>
