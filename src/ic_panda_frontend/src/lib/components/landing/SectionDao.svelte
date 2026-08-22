<script lang="ts">
  import IconArrowRightUp from '$lib/components/icons/IconArrowRightUp.svelte'
  import Reveal from '$lib/components/ui/Reveal.svelte'
  import { DAO_ENTRIES } from '$lib/site'
  import SectionShell from './SectionShell.svelte'
</script>

<SectionShell id="dao" index="04" label="Community Governance">
  <div class="grid grid-cols-1 gap-10 py-14 lg:grid-cols-12 lg:gap-12 lg:py-20">
    <div class="lg:col-span-6">
      <Reveal>
        <h2 class="display text-[clamp(2.25rem,5vw,3.5rem)]">
          Built in the open.<br />Governed on-chain.
        </h2>
      </Reveal>
    </div>
    <div class="lg:col-span-6 lg:pt-3">
      <Reveal delay={80}>
        <p class="text-pretty text-lg leading-relaxed text-ink-70">
          ICPanda DAO is governed through the Internet Computer's Service
          Nervous System. PANDA holders can lock tokens in neurons, vote on
          proposals, participate in governance, and collectively direct
          DAO-controlled resources.
        </p>
        <p class="mt-6 font-mono text-sm leading-relaxed text-ink">
          No private governance dashboard.<br />
          No off-chain substitute for the source of truth.
        </p>
      </Reveal>
    </div>
  </div>

  <Reveal>
    <div
      class="mb-16 grid grid-cols-1 gap-px border-t border-ink/15 bg-ink/15 md:mb-24 md:grid-cols-2 lg:grid-cols-4"
    >
      {#each DAO_ENTRIES as entry (entry.name)}
        <a
          class="group flex h-full flex-col justify-between bg-white p-6 transition-colors duration-200 hover:bg-paper md:p-8"
          href={entry.href}
          target="_blank"
          rel="noreferrer"
        >
          <div>
            <h3 class="display text-2xl">{entry.name}</h3>
            <p class="mt-3 text-sm leading-relaxed text-ink-70">{entry.body}</p>
          </div>
          <p
            class="mt-10 flex items-center gap-1.5 font-mono text-xs font-medium"
          >
            <span
              class="border-b border-transparent transition-colors group-hover:border-ink"
              >{entry.cta}</span
            >
            <span class="*:size-4" aria-hidden="true"><IconArrowRightUp /></span
            >
          </p>
        </a>
      {/each}
    </div>
  </Reveal>
</SectionShell>
