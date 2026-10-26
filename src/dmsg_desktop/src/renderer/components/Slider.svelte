<script lang="ts">
  import { Slider } from 'bits-ui'
  import { locale } from '../i18n/index.svelte'
  import { localeDir } from '../../shared/locales'
  let {
    value = $bindable(0),
    min = 0,
    max = 100,
    step = 1,
    label,
    disabled = false,
    testid
  }: {
    value?: number
    min?: number
    max?: number
    step?: number
    label: string
    disabled?: boolean
    testid?: string
  } = $props()
</script>

<div class="grid min-w-0 gap-1" data-testid={testid}>
  <div class="flex justify-between gap-3 text-[12px] text-muted">
    <span>{label}</span><span class="font-medium tabular-nums text-ink"
      >{value}</span
    >
  </div>
  <Slider.Root
    type="single"
    bind:value
    {min}
    {max}
    {step}
    {disabled}
    dir={localeDir(locale())}
    class="relative flex h-8 w-full touch-none items-center select-none data-disabled:opacity-50"
  >
    <div class="relative h-1.5 w-full rounded-full bg-edge-soft">
      <Slider.Range class="absolute h-full rounded-full bg-forest" />
    </div>
    <Slider.Thumb
      index={0}
      aria-label={label}
      class="block size-4 rounded-full border-2 border-forest bg-surface outline-none focus-visible:ring-3 focus-visible:ring-focus/35 data-disabled:cursor-not-allowed"
    />
  </Slider.Root>
</div>
