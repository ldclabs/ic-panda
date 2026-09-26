<script lang="ts">
  import type { Snippet } from 'svelte'
  import Registration from './Registration.svelte'

  interface Props {
    id: string
    /** Two-digit section marker, e.g. "02". */
    index: string
    /** Mono kicker printed next to the marker. */
    label: string
    class?: string
    children?: Snippet
  }

  let { id, index, label, class: klass = '', children }: Props = $props()
</script>

<!-- Each section is its own sheet of tracing paper over the painting. -->
<section {id} class="scroll-mt-16 px-3 py-4 md:px-6 md:py-8 {klass}">
  <div class="sheet mx-auto w-full max-w-6xl px-5 md:px-10">
    <Registration />
    <div
      class="sheet-bar border-ink/10 -mx-5 flex items-center gap-4 border-b px-5 py-4 md:sticky md:top-0 md:z-[1] md:-mx-10 md:px-10"
    >
      <span class="eyebrow text-ink tabular-nums">{index}</span>
      <span class="bg-ink/15 h-px flex-1"></span>
      <span class="eyebrow">{label}</span>
    </div>
    {@render children?.()}
  </div>
</section>
