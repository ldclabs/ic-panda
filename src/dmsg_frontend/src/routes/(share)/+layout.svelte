<script lang="ts">
  import { page } from '$app/state'
  import type { Snippet } from 'svelte'

  let { children }: { children: Snippet } = $props()
</script>

{#key page.url.pathname}
  {@render children()}
{/key}
