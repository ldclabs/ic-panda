<script lang="ts">
  import LegalPage from '$lib/components/legal/LegalPage.svelte'
  let { data } = $props()
</script>

<LegalPage kind="terms" initialDoc={data.doc} />
