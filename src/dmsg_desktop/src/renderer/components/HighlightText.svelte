<script lang="ts">
  import type { HighlightSpan } from './highlight'

  let { text = '', spans }: { text?: string; spans?: HighlightSpan[] } = $props()
</script>

{#if spans && spans.length > 0}
  {#each spans as span, i (i)}
    {#if span.matched}<mark class="rounded-[3px] bg-forest/15 px-0.5 font-semibold text-ink dark:bg-forest/30">{span.text}</mark>{:else}{span.text}{/if}
  {/each}
{:else}
  {text}
{/if}
