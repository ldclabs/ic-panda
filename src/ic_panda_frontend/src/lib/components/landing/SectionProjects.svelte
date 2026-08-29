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
        <p class="text-ink-70 text-lg leading-relaxed text-pretty">
          ICPanda DAO builds at two emerging frontiers: persistent machine
          cognition and verifiable on-chain capital formation.
        </p>
      </Reveal>
    </div>
  </div>

  <!-- ── Anda ────────────────────────────────────────────────────────── -->
  <Reveal>
    <article class="border-ink/15 border bg-white">
      <header
        class="border-ink/10 flex flex-col gap-6 border-b p-6 md:flex-row md:items-start md:justify-between md:p-10"
      >
        <div class="max-w-2xl">
          <p class="eyebrow">Sovereign AI · Open Protocols</p>
          <h3 class="display mt-5 text-[clamp(2rem,4.5vw,3.25rem)]">
            Give intelligence a past.
          </h3>
          <p class="text-ink-70 mt-6 leading-relaxed text-pretty">
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

      <div class="bg-ink/10 grid grid-cols-1 gap-px md:grid-cols-2">
        {#each ANDA_MODULES as mod (mod.name)}
          <a
            class="group hover:bg-paper flex flex-col bg-white p-6 transition-colors duration-200 md:p-8"
            href={mod.href}
            target="_blank"
            rel="noreferrer"
          >
            <div class="flex items-start justify-between gap-4">
              <h4 class="font-mono text-lg font-semibold tracking-tight">
                {mod.name}
              </h4>
              <span
                class="text-ink-30 group-hover:text-ink shrink-0 transition-colors *:size-5"
                aria-hidden="true"><IconArrowRightUp /></span
              >
            </div>
            <p class="eyebrow mt-2">{mod.kind}</p>
            <p class="text-ink-70 mt-4 text-sm leading-relaxed text-pretty">
              {mod.body}
            </p>
          </a>
        {/each}
      </div>

      <footer
        class="border-ink/10 flex flex-col gap-6 border-t p-6 md:flex-row md:items-center md:justify-between md:p-10"
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
    <article class="bg-ink text-paper mt-6 mb-16 md:mb-24">
      <header
        class="border-paper/15 flex flex-col gap-6 border-b p-6 md:flex-row md:items-start md:justify-between md:p-10"
      >
        <div class="max-w-2xl">
          <p
            class="text-paper/50 font-mono text-xs leading-none font-medium tracking-[0.18em] uppercase"
          >
            On-chain Markets · Multi-chain
          </p>
          <h3
            class="font-display mt-5 text-[clamp(2rem,4.5vw,3.25rem)] leading-[0.95] font-black tracking-[-0.035em]"
          >
            Infrastructure for verifiable crypto capital formation.
          </h3>
          <p class="text-paper/70 mt-6 leading-relaxed text-pretty">
            TokenList builds open infrastructure for creating, issuing, and
            governing on-chain assets with transparent rules and verifiable
            execution.
          </p>
        </div>
        <span
          class="text-paper/60 shrink-0 font-mono text-sm font-semibold tracking-[0.16em]"
        >
          TokenList.ing
        </span>
      </header>

      <div class="bg-paper/15 grid grid-cols-1 gap-px md:grid-cols-3">
        {#each TOKENLIST_STAGES as stage (stage.code)}
          <div class="bg-ink p-6 md:p-8">
            <p class="text-paper/40 font-mono text-xs tabular-nums">
              {stage.index}
            </p>
            <h4 class="mt-6 font-mono text-2xl font-semibold">{stage.code}</h4>
            <p
              class="text-panda mt-2 font-mono text-xs tracking-[0.18em] uppercase"
            >
              {stage.verb}
            </p>
            <p class="text-paper/70 mt-4 text-sm leading-relaxed text-pretty">
              {stage.body}
            </p>
          </div>
        {/each}
      </div>

      <footer
        class="border-paper/15 flex flex-col gap-6 border-t p-6 md:flex-row md:items-center md:justify-between md:p-10"
      >
        <div>
          <p class="font-mono text-sm font-semibold tracking-[0.2em]">
            CREATE <span class="text-paper/40">→</span> ISSUE
            <span class="text-paper/40">→</span> GOVERN
          </p>
          <p class="text-paper/60 mt-2 text-sm">
            From an asset's first block to its community-owned future.
          </p>
        </div>
        <a
          class="border-paper bg-paper text-ink hover:text-paper inline-flex shrink-0 items-center justify-center gap-2 border px-6 py-3 font-mono text-sm font-medium tracking-[0.08em] uppercase transition-colors duration-200 hover:bg-transparent"
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
