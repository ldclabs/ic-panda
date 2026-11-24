<script lang="ts">
  import { roll } from './motion'

  /**
   * A count in a badge. Only the digits that changed roll: up when it grew,
   * down when it shrank. Past `max` it reads `max+`. `quiet` swaps digits
   * without rolling, for a value that follows a drag.
   */
  let { value, max = Infinity, quiet = false }: { value: number; max?: number; quiet?: boolean } = $props()

  const step = { last: undefined as number | undefined, up: true }
  const digits = $derived.by(() => {
    step.up = step.last === undefined || value >= step.last
    step.last = value
    return [...(value > max ? `${max}+` : String(value))]
  })
</script>

{#each digits as digit, i (digits.length - i)}{#key digit}<span class="inline-block" in:roll={{ up: step.up, quiet }}>{digit}</span>{/key}{/each}
