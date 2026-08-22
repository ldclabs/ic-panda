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

    <dl class="mt-12 border-t border-ink/15">
      {#each PRINCIPLES as p, i (p.name)}
        <Reveal delay={i * 60}>
          <div
            class="group grid grid-cols-1 gap-3 border-b border-ink/10 py-7 transition-colors duration-200 hover:bg-white md:grid-cols-12 md:gap-6 md:py-9"
          >
            <span
              class="font-mono text-xs tabular-nums text-ink-30 md:col-span-1"
              >{p.index}</span
            >
            <dt class="display-sm text-2xl md:col-span-4 md:text-3xl"
              >{p.name}</dt
            >
            <dd
              class="text-pretty leading-relaxed text-ink-70 md:col-span-7 md:pt-1"
            >
              {p.body}
            </dd>
          </div>
        </Reveal>
      {/each}
    </dl>
  </div>
</SectionShell>
