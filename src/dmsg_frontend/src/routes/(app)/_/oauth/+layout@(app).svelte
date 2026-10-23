<script lang="ts">
  import UtilityPage from '$lib/components/site/UtilityPage.svelte'
  import type { Snippet } from 'svelte'
  let { children }: { children: Snippet } = $props()
</script>

<UtilityPage>{@render children()}</UtilityPage>
