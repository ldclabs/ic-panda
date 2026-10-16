<script lang="ts">
  import { RadioGroup } from 'bits-ui'

  /** One of a few options side by side, on a bits-ui radio group. */
  let {
    value = $bindable(''),
    options,
    label,
    disabled = false
  }: {
    value?: string
    options: string[]
    label: string
    disabled?: boolean
  } = $props()
</script>

<RadioGroup.Root
  bind:value
  {disabled}
  aria-label={label}
  orientation="horizontal"
  class="inline-flex w-fit max-w-full flex-wrap gap-1 rounded-xl border border-[var(--line)] bg-[var(--canvas)] p-1"
>
  {#each options as option (option)}
    <RadioGroup.Item
      value={option}
      class="h-9 min-w-24 rounded-lg! px-4 text-[14px] font-medium text-[var(--muted)]! disabled:cursor-not-allowed disabled:opacity-60 data-[state=checked]:bg-white! data-[state=checked]:text-[var(--action)]! data-[state=checked]:shadow-[0_1px_2px_rgb(16_37_31/0.08),0_0_0_1px_var(--line)]"
      >{option}</RadioGroup.Item
    >
  {/each}
</RadioGroup.Root>
