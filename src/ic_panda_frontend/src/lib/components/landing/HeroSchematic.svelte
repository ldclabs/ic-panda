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

  <div class="border-ink border px-4 py-3 text-center">
    <span class="font-mono text-xs font-semibold tracking-[0.2em] uppercase">
      ICPanda DAO
    </span>
  </div>

  <div class="bg-ink/30 mx-auto h-5 w-px"></div>
  <div class="relative h-5">
    <span class="bg-ink/30 absolute top-0 right-1/4 left-1/4 h-px"></span>
    <span class="bg-ink/30 absolute top-0 left-1/4 h-5 w-px"></span>
    <span class="bg-ink/30 absolute top-0 right-1/4 h-5 w-px"></span>
  </div>

  <div class="grid grid-cols-2 gap-3">
    <a
      href={LINKS.anda}
      target="_blank"
      rel="noreferrer"
      class="group border-ink/20 bg-paper/60 hover:border-ink flex flex-col justify-between border p-3 transition-colors duration-200"
    >
      <div>
        <p class="font-mono text-xs leading-tight tracking-[0.1em] uppercase">
          Sovereign<br />Minds
        </p>
        <p class="text-ink-70 mt-2 text-sm leading-snug">
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
      class="group border-ink/20 bg-paper/60 hover:border-ink flex flex-col justify-between border p-3 transition-colors duration-200"
    >
      <div>
        <p class="font-mono text-xs leading-tight tracking-[0.1em] uppercase">
          Sovereign<br />Markets
        </p>
        <p class="text-ink-70 mt-2 text-sm leading-snug">
          Verifiable capital formation
        </p>
      </div>
      <p class="mt-4 font-mono text-xs font-semibold">
        <span class="text-ink-30">→</span> TokenList
      </p>
    </a>
  </div>

  <div class="relative h-5">
    <span class="bg-ink/30 absolute bottom-0 left-1/4 h-5 w-px"></span>
    <span class="bg-ink/30 absolute right-1/4 bottom-0 h-5 w-px"></span>
    <span class="bg-ink/30 absolute inset-x-4 bottom-0 h-px"></span>
  </div>

  <div
    class="border-ink bg-ink text-paper flex flex-wrap items-center justify-between gap-x-4 gap-y-1 border px-4 py-3"
  >
    <span class="font-mono text-xs font-semibold tracking-[0.2em]">PANDA</span>
    <span class="font-mono text-xs tracking-[0.1em] uppercase opacity-70">
      Governance · Coordination · Treasury
    </span>
  </div>
</figure>
