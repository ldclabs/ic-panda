<script lang="ts">
  import { Progress } from 'bits-ui'
  import { progressValue } from './form-values'
  let {
    value = null,
    max = 100,
    label,
    testid
  }: {
    value?: number | null
    max?: number
    label: string
    testid?: string
  } = $props()
  const state = $derived(progressValue(value, max))
</script>

<div class="grid gap-1.5" data-testid={testid}>
  <div class="flex justify-between gap-3 text-[12px] text-muted">
    <span>{label}</span>{#if state.value !== null}<span class="tabular-nums"
        >{Math.round(state.percent)}%</span
      >{/if}
  </div>
  <Progress.Root
    value={state.value}
    max={state.max}
    aria-label={label}
    class="h-1.5 overflow-hidden rounded-full bg-edge-soft"
  >
    {#if state.value === null}<span
        class="progress-run block h-full rounded-full bg-forest"
      ></span>
    {:else}<span
        class="block h-full rounded-full bg-forest transition-[width] duration-200 motion-reduce:transition-none"
        style:width={`${state.percent}%`}
      ></span>{/if}
  </Progress.Root>
</div>
