<script lang="ts">
  export let value: number = 0
  export let max: number = 100
  export let label: string
  export let height = 'h-3'
  export let width = 'w-full'
  export let meter = 'bg-panda'
  export let track = 'bg-black/10'

  $: percent = max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0
</script>

<div
  class="{width} {height} {track} overflow-hidden rounded-full"
  role="progressbar"
  aria-label={label}
  aria-valuemin="0"
  aria-valuemax={max}
  aria-valuenow={value}
>
  <div class="h-full {meter} transition-[width]" style:width="{percent}%"></div>
</div>
