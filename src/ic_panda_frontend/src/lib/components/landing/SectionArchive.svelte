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
          Engineering that<br />carries forward.
        </h2>
      </Reveal>
    </div>
    <div class="lg:col-span-6 lg:pt-3">
      <Reveal delay={80}>
        <p class="text-ink-70 text-lg leading-relaxed text-pretty">
          Every serious experiment should leave knowledge behind. Earlier work
          in identity, storage, and cryptography powers the tools we use today.
        </p>
      </Reveal>
    </div>
  </div>

  <Reveal>
    <article
      class="border-ink/25 mb-16 grid grid-cols-1 rounded-xl border border-dashed md:mb-24 md:grid-cols-12"
    >
      <div
        class="border-ink/25 flex items-center justify-between gap-4 border-b border-dashed p-6 md:col-span-4 md:flex-col md:items-start md:justify-start md:border-r md:border-b-0 md:p-8"
      >
        <div>
          <h3 class="display-sm text-3xl">Early dMsg</h3>
          <p class="text-ink-70 mt-2 font-mono text-xs">Encrypted messaging</p>
        </div>
        <span
          class="border-ink/25 text-ink-70 inline-flex items-center gap-2 rounded-md border px-3 py-1.5 font-mono text-xs tracking-[0.14em] uppercase md:mt-8"
        >
          <span class="bg-ink/40 size-1.5 rounded-full"></span>
          Technical foundation
        </span>
      </div>

      <div class="p-6 md:col-span-8 md:p-8">
        <p class="text-ink-70 leading-relaxed text-pretty">
          dMsg began as a decentralized end-to-end encrypted messaging
          application built fully on the Internet Computer.
        </p>
        <p class="text-ink-70 mt-4 leading-relaxed text-pretty">
          dMsg explored sovereign identity, encrypted communication,
          decentralized storage, and cryptographic key management — ideas that
          helped shape later ICPanda DAO infrastructure.
        </p>
        <p class="text-ink-70 mt-4 text-sm leading-relaxed text-pretty">
          That experience now powers the dMsg Chrome extension: a private vault,
          encrypted collaboration, and identity signing across applications.
          Earlier code remains available as part of our engineering history.
        </p>
        <div class="mt-8 flex flex-wrap items-center gap-x-8 gap-y-4">
          <LinkOut href={LINKS.dmsgGithub} label="Explore the Earlier Source" />
          <a class="link-mark" href="#dmsg">Explore dMsg</a>
        </div>
      </div>
    </article>
  </Reveal>
</SectionShell>
