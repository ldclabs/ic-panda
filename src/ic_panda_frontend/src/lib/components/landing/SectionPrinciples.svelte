<script lang="ts">
  import Reveal from '$lib/components/ui/Reveal.svelte'
  import { PRINCIPLES } from '$lib/site'
  import SectionShell from './SectionShell.svelte'
</script>

<SectionShell id="principles" index="05" label="How We Build">
  <div class="py-14 lg:py-20">
    <Reveal>
      <h2 class="display text-[clamp(2.25rem,5vw,3.5rem)]">Open by default.</h2>
    </Reveal>

    <dl class="border-ink/15 isolate mt-12 border-t">
      {#each PRINCIPLES as p, i (p.name)}
        <Reveal delay={i * 60}>
          <div
            class="principle-row border-ink/10 relative grid grid-cols-1 gap-3 border-b py-7 md:grid-cols-12 md:gap-6 md:py-9"
          >
            <span
              class="text-ink-30 font-mono text-xs tabular-nums md:col-span-1"
              >{p.index}</span
            >
            <dt class="display-sm text-2xl md:col-span-4 md:text-3xl"
              >{p.name}</dt
            >
            <dd
              class="text-ink-70 leading-relaxed text-pretty md:col-span-7 md:pt-1"
            >
              {p.body}
            </dd>
          </div>
        </Reveal>
      {/each}
    </dl>
  </div>
</SectionShell>

<style>
  /* The hover plate bleeds past the row box so the white ground clears the
   * text on both sides. Keeping it on a pseudo-element leaves the row's own
   * box alone, so the rules stay aligned with the section grid. Painted
   * behind the content inside the list's isolation context.
   *
   * The bleed matches the shell's gutter (px-5) below md, so the plate runs
   * flush to the viewport edge instead of leaving a 4px sliver of paper. */
  .principle-row::before {
    content: '';
    position: absolute;
    inset: 0 -1.25rem;
    z-index: -1;
    background: var(--surface);
    opacity: 0;
    transition: opacity 200ms;
  }

  .principle-row:hover::before {
    opacity: 1;
  }

  @media (min-width: 768px) {
    .principle-row::before {
      inset: 0 -1.5rem;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .principle-row::before {
      transition: none;
    }
  }
</style>
