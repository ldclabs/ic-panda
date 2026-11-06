<script lang="ts">
  import { followLink } from './link-confirm.svelte'
  import { linkParts } from './links'

  /** Text whose web and dMsg addresses can be followed, keeping its own lines. */
  let { text }: { text: string } = $props()
  const parts = $derived(linkParts(text))
</script>

{#each parts as part}{#if part.url}<a
      href={part.url}
      class="text-forest underline underline-offset-2 hover:no-underline"
      onclick={(event) => {
        event.preventDefault()
        event.stopPropagation()
        followLink(part.url, part.host)
      }}
      data-testid="message-link">{part.text}</a>{:else}{part.text}{/if}{/each}
