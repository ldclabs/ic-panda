<script lang="ts">
  import Reveal from '$lib/components/ui/Reveal.svelte'
  import { LINKS } from '$lib/site'
  import LinkOut from './LinkOut.svelte'
  import SectionShell from './SectionShell.svelte'
</script>

<SectionShell id="archive" index="06" label="Our History">
  <div class="grid grid-cols-1 gap-10 py-14 lg:grid-cols-12 lg:gap-12 lg:py-20">
    <div class="lg:col-span-6">
      <Reveal>
        <h2 class="display text-[clamp(2.25rem,5vw,3.5rem)]">
          Experiments that got us here.
        </h2>
      </Reveal>
    </div>
    <div class="lg:col-span-6 lg:pt-3">
      <Reveal delay={80}>
        <p class="text-pretty text-lg leading-relaxed text-ink-70">
          Not every project needs to live forever. Every serious experiment
          should leave knowledge behind.
        </p>
      </Reveal>
    </div>
  </div>

  <Reveal>
    <article
      class="mb-16 grid grid-cols-1 border border-dashed border-ink/25 md:mb-24 md:grid-cols-12"
    >
      <div
        class="flex items-center justify-between gap-4 border-b border-dashed border-ink/25 p-6 md:col-span-4 md:flex-col md:items-start md:justify-start md:border-b-0 md:border-r md:p-8"
      >
        <div>
          <h3 class="display-sm text-3xl">dMsg</h3>
          <p class="mt-2 font-mono text-xs text-ink-70">dmsg.net</p>
        </div>
        <span
          class="inline-flex items-center gap-2 border border-ink/25 px-3 py-1.5 font-mono text-xs uppercase tracking-[0.14em] text-ink-70 md:mt-8"
        >
          <span class="size-1.5 rounded-full bg-ink/40"></span>
          Archived · Development paused
        </span>
      </div>

      <div class="p-6 md:col-span-8 md:p-8">
        <p class="text-pretty leading-relaxed text-ink-70">
          A decentralized end-to-end encrypted messaging application built fully
          on the Internet Computer.
        </p>
        <p class="mt-4 text-pretty leading-relaxed text-ink-70">
          dMsg explored sovereign identity, encrypted communication,
          decentralized storage, and cryptographic key management — ideas that
          helped shape later ICPanda DAO infrastructure.
        </p>
        <p class="mt-4 text-pretty text-sm leading-relaxed text-ink-70">
          The application is no longer under active development, but its code
          and technical work remain part of our open-source history.
        </p>
        <div class="mt-8 flex flex-wrap items-center gap-x-8 gap-y-4">
          <LinkOut href={LINKS.dmsgGithub} label="Explore the Source" />
          <LinkOut href={LINKS.dmsg} label="Visit dMsg.net" />
        </div>
      </div>
    </article>
  </Reveal>
</SectionShell>
