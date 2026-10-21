<script lang="ts">
  import ArchiveShell from '$lib/components/legacy/ArchiveShell.svelte'
  import type { Snippet } from 'svelte'
  let { children }: { children?: Snippet } = $props()
</script>

<ArchiveShell
  >{#if children}{@render children()}{/if}</ArchiveShell
>
