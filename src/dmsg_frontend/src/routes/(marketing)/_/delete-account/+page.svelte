<script lang="ts">
  import LegalPage from '$lib/components/legal/LegalPage.svelte'
  let { data } = $props()
</script>

<LegalPage kind="delete-account" initialDoc={data.doc} />
