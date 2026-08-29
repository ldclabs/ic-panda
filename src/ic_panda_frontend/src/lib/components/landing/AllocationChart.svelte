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
    class="border-ink flex h-14 w-full overflow-hidden border"
    role="img"
    aria-label="Genesis allocation: 4% development team, 4% seed funders, 12% SNS swap, 80% DAO treasury"
  >
    <span class="hatch border-ink h-full border-r" style="width:4%"></span>
    <span class="hatch-dense border-ink h-full border-r" style="width:4%"
    ></span>
    <span class="border-ink bg-ink/25 h-full border-r" style="width:12%"></span>
    <span
      class="bg-ink flex h-full items-center justify-end px-3"
      style="width:80%"
    >
      <span
        class="text-paper font-mono text-xs font-semibold tracking-[0.14em]"
      >
        DAO TREASURY · 80%
      </span>
    </span>
  </div>

  <!-- Nested treasury bar, aligned to the 80% segment above -->
  <div class="relative ml-[20%] w-[80%]">
    <div class="flex h-3">
      <span class="bg-ink/30 ml-[62.5%] block h-3 w-px"></span>
    </div>
    <div class="border-ink/30 flex h-8 w-full overflow-hidden border">
      {#each TREASURY_ALLOCATION as item, i (item.name)}
        <span
          class="bg-ink/10 h-full {i > 0 ? 'border-ink/30 border-l' : ''}"
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
        <li class="text-ink-70 flex gap-3 font-mono text-xs">
          <span class="text-ink-70 w-8 shrink-0 tabular-nums">
            {item.percent}%
          </span>
          <span>{item.name}</span>
        </li>
      {/each}
    </ul>
  </div>

  <!-- Legend / ledger -->
  <dl class="border-ink/15 mt-8 border-t">
    {#each GENESIS_ALLOCATION as item (item.name)}
      <div
        class="border-ink/10 flex items-baseline gap-4 border-b py-3 md:gap-6"
      >
        <dt
          class="w-14 shrink-0 font-mono text-lg font-semibold tabular-nums md:text-xl"
        >
          {item.percent}%
        </dt>
        <dd class="flex flex-1 flex-wrap items-baseline justify-between gap-2">
          <span class="font-medium">{item.name}</span>
          <span class="text-ink-70 font-mono text-xs tabular-nums">
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
