<script lang="ts">
  import Home from '$lib/components/airdrop/Home.svelte'
  import { authStore } from '$lib/stores/auth'

  $: principal = $authStore.identity.getPrincipal()
</script>

<div
  class="mx-auto flex h-full w-full max-w-5xl flex-col flex-nowrap content-center items-center"
>
  {#key principal.toText()}
    <Home />
  {/key}
</div>
