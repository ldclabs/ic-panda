<script lang="ts">
  import { followLink } from './link-confirm.svelte'
  import { linkParts } from './links'
  import HighlightText from './HighlightText.svelte'
  import type { HighlightSpan } from './highlight'

  /** Text whose web and dMsg addresses can be followed, keeping its own lines. */
  let { text, spans }: { text: string; spans?: HighlightSpan[] | null } = $props()
  const parts = $derived.by(() => {
    let index = 0, offset = 0
    return linkParts(text).map(part => {
      const slices: HighlightSpan[] = []
      let remaining = part.text.length
      while (spans && index < spans.length && remaining > 0) {
        const span = spans[index]
        const length = Math.min(remaining, span.text.length - offset)
        if (length) slices.push({ text: span.text.slice(offset, offset + length), matched: span.matched })
        remaining -= length
        offset += length
        if (offset === span.text.length) { index++; offset = 0 }
      }
      return { ...part, spans: slices }
    })
  })
</script>

{#each parts as part}{#if part.url}<a
      href={part.url}
      class="text-forest underline underline-offset-2 hover:no-underline"
      onclick={(event) => {
        event.preventDefault()
        event.stopPropagation()
        followLink(part.url, part.host)
      }}
      data-testid="message-link"><HighlightText text={part.text} spans={part.spans} /></a>{:else}<HighlightText text={part.text} spans={part.spans} />{/if}{/each}
