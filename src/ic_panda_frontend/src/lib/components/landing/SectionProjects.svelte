<script lang="ts">
  import IconArrowRightUp from '$lib/components/icons/IconArrowRightUp.svelte'
  import Reveal from '$lib/components/ui/Reveal.svelte'
  import { ANDA_MODULES, LINKS, TOKENLIST_STAGES } from '$lib/site'
  import LinkOut from './LinkOut.svelte'
  import SectionShell from './SectionShell.svelte'
</script>

<SectionShell id="projects" index="03" label="Projects">
  <div class="grid grid-cols-1 gap-10 py-14 lg:grid-cols-12 lg:gap-12 lg:py-20">
    <div class="lg:col-span-7">
      <Reveal>
        <h2 class="display text-[clamp(2.25rem,5vw,3.5rem)]">
          Open infrastructure for<br class="hidden md:block" />
          intelligence and markets.
        </h2>
      </Reveal>
    </div>
    <div class="lg:col-span-5 lg:pt-3">
      <Reveal delay={80}>
        <p class="text-pretty text-lg leading-relaxed text-ink-70">
          ICPanda DAO builds at two emerging frontiers: persistent machine
          cognition and verifiable on-chain capital formation.
        </p>
      </Reveal>
    </div>
  </div>

  <!-- ── Anda ────────────────────────────────────────────────────────── -->
  <Reveal>
    <article class="border border-ink/15 bg-white">
      <header
        class="flex flex-col gap-6 border-b border-ink/10 p-6 md:flex-row md:items-start md:justify-between md:p-10"
      >
        <div class="max-w-2xl">
          <p class="eyebrow">Sovereign AI · Open Protocols</p>
          <h3 class="display mt-5 text-[clamp(2rem,4.5vw,3.25rem)]">
            Give intelligence a past.
          </h3>
          <p class="mt-6 text-pretty leading-relaxed text-ink-70">
            AI agents should not start from zero every time. Anda develops open
            protocols, infrastructure, and benchmarks for agents that preserve
            cognitive state, learn from experience, and let the past
            meaningfully change future decisions.
          </p>
        </div>
        <img
          class="h-10 w-auto shrink-0 md:h-12"
          src="/_assets/anda.svg"
          alt="Anda"
          width="1144"
          height="310"
        />
      </header>

      <div class="grid grid-cols-1 gap-px bg-ink/10 md:grid-cols-2">
        {#each ANDA_MODULES as mod (mod.name)}
          <a
            class="group flex flex-col bg-white p-6 transition-colors duration-200 hover:bg-paper md:p-8"
            href={mod.href}
            target="_blank"
            rel="noreferrer"
          >
            <div class="flex items-start justify-between gap-4">
              <h4 class="font-mono text-lg font-semibold tracking-tight">
                {mod.name}
              </h4>
              <span
                class="shrink-0 text-ink-30 transition-colors *:size-5 group-hover:text-ink"
                aria-hidden="true"><IconArrowRightUp /></span
              >
            </div>
            <p class="eyebrow mt-2">{mod.kind}</p>
            <p class="mt-4 text-pretty text-sm leading-relaxed text-ink-70">
              {mod.body}
            </p>
          </a>
        {/each}
      </div>

      <footer
        class="flex flex-col gap-6 border-t border-ink/10 p-6 md:flex-row md:items-center md:justify-between md:p-10"
      >
        <p class="display-sm text-xl md:text-2xl">
          Memory gives intelligence continuity through time.
        </p>
        <div class="flex shrink-0 flex-wrap items-center gap-x-8 gap-y-4">
          <LinkOut href={LINKS.anda} label="Explore Anda.ai" />
          <LinkOut href={LINKS.andaGithub} label="View Open Source" />
        </div>
      </footer>
    </article>
  </Reveal>

  <!-- ── TokenList ───────────────────────────────────────────────────── -->
  <Reveal>
    <article class="mb-16 mt-6 bg-ink text-paper md:mb-24">
      <header
        class="flex flex-col gap-6 border-b border-paper/15 p-6 md:flex-row md:items-start md:justify-between md:p-10"
      >
        <div class="max-w-2xl">
          <p
            class="font-mono text-xs font-medium uppercase leading-none tracking-[0.18em] text-paper/50"
          >
            On-chain Markets · Multi-chain
          </p>
          <h3
            class="mt-5 font-display text-[clamp(2rem,4.5vw,3.25rem)] font-black leading-[0.95] tracking-[-0.035em]"
          >
            Infrastructure for verifiable crypto capital formation.
          </h3>
          <p class="mt-6 text-pretty leading-relaxed text-paper/70">
            TokenList builds open infrastructure for creating, issuing, and
            governing on-chain assets with transparent rules and verifiable
            execution.
          </p>
        </div>
        <span
          class="shrink-0 font-mono text-sm font-semibold tracking-[0.16em] text-paper/60"
        >
          TokenList.ing
        </span>
      </header>

      <div class="grid grid-cols-1 gap-px bg-paper/15 md:grid-cols-3">
        {#each TOKENLIST_STAGES as stage (stage.code)}
          <div class="bg-ink p-6 md:p-8">
            <p class="font-mono text-xs tabular-nums text-paper/40">
              {stage.index}
            </p>
            <h4 class="mt-6 font-mono text-2xl font-semibold">{stage.code}</h4>
            <p
              class="mt-2 font-mono text-xs uppercase tracking-[0.18em] text-panda"
            >
              {stage.verb}
            </p>
            <p class="mt-4 text-pretty text-sm leading-relaxed text-paper/70">
              {stage.body}
            </p>
          </div>
        {/each}
      </div>

      <footer
        class="flex flex-col gap-6 border-t border-paper/15 p-6 md:flex-row md:items-center md:justify-between md:p-10"
      >
        <div>
          <p class="font-mono text-sm font-semibold tracking-[0.2em]">
            CREATE <span class="text-paper/40">→</span> ISSUE
            <span class="text-paper/40">→</span> GOVERN
          </p>
          <p class="mt-2 text-sm text-paper/60">
            From an asset's first block to its community-owned future.
          </p>
        </div>
        <a
          class="inline-flex shrink-0 items-center justify-center gap-2 border border-paper bg-paper px-6 py-3 font-mono text-sm font-medium uppercase tracking-[0.08em] text-ink transition-colors duration-200 hover:bg-transparent hover:text-paper"
          href={LINKS.tokenlist}
          target="_blank"
          rel="noreferrer"
        >
          <span>Explore TokenList.ing</span>
          <span class="*:size-4" aria-hidden="true"><IconArrowRightUp /></span>
        </a>
      </footer>
    </article>
  </Reveal>
</SectionShell>
