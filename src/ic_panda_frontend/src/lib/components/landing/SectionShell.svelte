<script lang="ts">
  import type { Snippet } from 'svelte'

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

<section {id} class="scroll-mt-16 border-t border-ink/15 {klass}">
  <div class="mx-auto w-full max-w-6xl px-5 md:px-10">
    <div
      class="flex items-center gap-4 border-b border-ink/10 py-4 md:sticky md:top-0 md:z-[1] md:bg-paper/90 md:backdrop-blur"
    >
      <span class="eyebrow tabular-nums text-ink">{index}</span>
      <span class="h-px flex-1 bg-ink/15"></span>
      <span class="eyebrow">{label}</span>
    </div>
    {@render children?.()}
  </div>
</section>
