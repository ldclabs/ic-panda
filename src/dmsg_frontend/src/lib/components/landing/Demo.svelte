<script lang="ts">
  import type { Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'

  let {
    title,
    id,
    children
  }: { title: string; id?: string; children: Snippet } = $props()
  let open = $state(false)
</script>

<details {id} class="deep-dive" bind:open>
  <summary
    ><Glyph name="play" size={18} /><span>{title}</span><Glyph
      name="arrow-right"
      size={18}
    /></summary
  >
  {#if open}
    <div class="deep-body">
      <p class="demo-label"
        ><Glyph name="eye" size={16} />{$t(
          'Interactive example · sample data · nothing is sent'
        )}</p
      >
      {@render children()}
    </div>
  {/if}
</details>
