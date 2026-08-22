<script lang="ts">
  import { GENESIS_ALLOCATION, TREASURY_ALLOCATION } from '$lib/site'

  // Treasury sub-allocations are shares of the 80% treasury, drawn nested
  // underneath it so the parent/child relationship is visible rather than
  // flattened into eight equal-looking pie slices.
  const TREASURY_PERCENT = 80
</script>

<div class="w-full">
  <!-- Genesis supply bar: 4 / 4 / 12 / 80 -->
  <div
    class="flex h-14 w-full overflow-hidden border border-ink"
    role="img"
    aria-label="Genesis allocation: 4% development team, 4% seed funders, 12% SNS swap, 80% DAO treasury"
  >
    <span class="hatch h-full border-r border-ink" style="width:4%"></span>
    <span class="hatch-dense h-full border-r border-ink" style="width:4%"
    ></span>
    <span class="h-full border-r border-ink bg-ink/25" style="width:12%"></span>
    <span
      class="flex h-full items-center justify-end bg-ink px-3"
      style="width:80%"
    >
      <span
        class="font-mono text-xs font-semibold tracking-[0.14em] text-paper"
      >
        DAO TREASURY · 80%
      </span>
    </span>
  </div>

  <!-- Nested treasury bar, aligned to the 80% segment above -->
  <div class="relative ml-[20%] w-[80%]">
    <div class="flex h-3">
      <span class="ml-[62.5%] block h-3 w-px bg-ink/30"></span>
    </div>
    <div class="flex h-8 w-full overflow-hidden border border-ink/30">
      {#each TREASURY_ALLOCATION as item, i (item.name)}
        <span
          class="h-full bg-ink/10 {i > 0 ? 'border-l border-ink/30' : ''}"
          style="width:{(item.percent / TREASURY_PERCENT) * 100}%"
          title="{item.percent}% — {item.name}"
        ></span>
      {/each}
    </div>
  </div>

  <!-- The caption un-indents on small screens so long labels stay on one line -->
  <div class="mt-3 md:ml-[20%] md:w-[80%]">
    <ul class="space-y-1">
      {#each TREASURY_ALLOCATION as item (item.name)}
        <li class="flex gap-3 font-mono text-xs text-ink-70">
          <span class="w-8 shrink-0 tabular-nums text-ink-70">
            {item.percent}%
          </span>
          <span>{item.name}</span>
        </li>
      {/each}
    </ul>
  </div>

  <!-- Legend / ledger -->
  <dl class="mt-8 border-t border-ink/15">
    {#each GENESIS_ALLOCATION as item (item.name)}
      <div
        class="flex items-baseline gap-4 border-b border-ink/10 py-3 md:gap-6"
      >
        <dt
          class="w-14 shrink-0 font-mono text-lg font-semibold tabular-nums md:text-xl"
        >
          {item.percent}%
        </dt>
        <dd class="flex flex-1 flex-wrap items-baseline justify-between gap-2">
          <span class="font-medium">{item.name}</span>
          <span class="font-mono text-xs tabular-nums text-ink-70">
            {item.tokens} PANDA
          </span>
        </dd>
      </div>
    {/each}
  </dl>
</div>

<style>
  /* Outlined segments read as "not treasury" without adding a colour scale. */
  .hatch {
    background-image: repeating-linear-gradient(
      45deg,
      rgb(11 11 11 / 55%) 0 1px,
      transparent 1px 5px
    );
  }

  .hatch-dense {
    background-image: repeating-linear-gradient(
      -45deg,
      rgb(11 11 11 / 55%) 0 1px,
      transparent 1px 4px
    );
  }
</style>
