<script lang="ts">
  import { LINKS } from '$lib/site'
</script>

<!--
  The whole brand structure in one hairline schematic:
  DAO on top, two frontiers below, PANDA as the coordination bus underneath.
-->
<figure class="rule-box p-5 md:p-6" aria-label="ICPanda DAO structure">
  <figcaption class="eyebrow mb-5 flex items-center justify-between">
    <span>fig.01 — structure</span>
    <span class="text-panda">●</span>
  </figcaption>

  <div class="border border-ink px-4 py-3 text-center">
    <span class="font-mono text-xs font-semibold uppercase tracking-[0.2em]">
      ICPanda DAO
    </span>
  </div>

  <div class="mx-auto h-5 w-px bg-ink/30"></div>
  <div class="relative h-5">
    <span class="absolute left-1/4 right-1/4 top-0 h-px bg-ink/30"></span>
    <span class="absolute left-1/4 top-0 h-5 w-px bg-ink/30"></span>
    <span class="absolute right-1/4 top-0 h-5 w-px bg-ink/30"></span>
  </div>

  <div class="grid grid-cols-2 gap-3">
    <a
      href={LINKS.anda}
      target="_blank"
      rel="noreferrer"
      class="group flex flex-col justify-between border border-ink/20 bg-paper/60 p-3 transition-colors duration-200 hover:border-ink"
    >
      <div>
        <p class="font-mono text-xs uppercase leading-tight tracking-[0.1em]">
          Sovereign<br />Minds
        </p>
        <p class="mt-2 text-sm leading-snug text-ink-70">
          Persistent AI cognition
        </p>
      </div>
      <p class="mt-4 font-mono text-xs font-semibold">
        <span class="text-ink-30">→</span> Anda
      </p>
    </a>

    <a
      href={LINKS.tokenlist}
      target="_blank"
      rel="noreferrer"
      class="group flex flex-col justify-between border border-ink/20 bg-paper/60 p-3 transition-colors duration-200 hover:border-ink"
    >
      <div>
        <p class="font-mono text-xs uppercase leading-tight tracking-[0.1em]">
          Sovereign<br />Markets
        </p>
        <p class="mt-2 text-sm leading-snug text-ink-70">
          Verifiable capital formation
        </p>
      </div>
      <p class="mt-4 font-mono text-xs font-semibold">
        <span class="text-ink-30">→</span> TokenList
      </p>
    </a>
  </div>

  <div class="relative h-5">
    <span class="absolute bottom-0 left-1/4 h-5 w-px bg-ink/30"></span>
    <span class="absolute bottom-0 right-1/4 h-5 w-px bg-ink/30"></span>
    <span class="absolute inset-x-4 bottom-0 h-px bg-ink/30"></span>
  </div>

  <div
    class="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 border border-ink bg-ink px-4 py-3 text-paper"
  >
    <span class="font-mono text-xs font-semibold tracking-[0.2em]">PANDA</span>
    <span class="font-mono text-xs uppercase tracking-[0.1em] opacity-70">
      Governance · Coordination · Treasury
    </span>
  </div>
</figure>
