<script lang="ts">
  import { Slider } from 'bits-ui'
  import { locale } from '../i18n/index.svelte'
  import { localeDir } from '../../shared/locales'
  import Count from './Count.svelte'

  /**
   * A value on a track. The thumb grows while pressed; it and the number
   * follow a drag at once, and glide when a key or a click on the track
   * moves them.
   */
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

  let pressed = false
  let dragging = $state(false)

  function endDrag() {
    pressed = false
    dragging = false
  }
</script>

<svelte:window
  onpointermovecapture={(event) => { if (pressed && (event.buttons & 1)) dragging = true }}
  onpointerup={endDrag}
  onpointercancel={endDrag}
  onblur={endDrag}
/>

<div class="grid min-w-0 gap-1" data-testid={testid}>
  <div class="flex justify-between gap-3 text-callout text-muted">
    <span>{label}</span><span class="font-medium tabular-nums text-ink"
      ><Count {value} quiet={dragging} /></span
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
    class="slider group/slider relative flex h-8 w-full touch-none items-center select-none data-disabled:opacity-50"
    data-dragging={dragging || undefined}
    onpointerdown={(event: PointerEvent) => { pressed = !disabled && event.button === 0 }}
    onValueCommit={endDrag}
  >
    <div class="relative h-1.5 w-full rounded-full bg-edge-soft">
      <Slider.Range class="absolute h-full rounded-full bg-forest" />
    </div>
    <Slider.Thumb
      index={0}
      aria-label={label}
      class="block size-4 rounded-full border-2 border-forest bg-surface outline-none group-active/slider:not-data-disabled:scale-125 group-active/slider:not-data-disabled:ring-6 group-active/slider:not-data-disabled:ring-forest/15 focus-visible:ring-3 focus-visible:ring-focus/35 data-disabled:cursor-not-allowed"
    />
  </Slider.Root>
</div>

<style>
  /* Pressing grows the thumb; a key or a click on the track glides it, a drag does not. */
  :global(.slider [data-slider-thumb]) {
    transition:
      left 120ms var(--ease-calm),
      right 120ms var(--ease-calm),
      scale 180ms var(--ease-spring),
      box-shadow 180ms var(--ease-calm);
  }

  :global(.slider [data-slider-range]) {
    transition:
      left 120ms var(--ease-calm),
      right 120ms var(--ease-calm);
  }

  :global(.slider[data-dragging] :is([data-slider-thumb], [data-slider-range])) {
    transition-property: scale, box-shadow;
  }
</style>
