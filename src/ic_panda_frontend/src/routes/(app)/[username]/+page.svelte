<script lang="ts">
  import { page } from '$app/stores'
  import ProfileDetail from '$lib/components/messages/ProfileDetail.svelte'
  import { authStore } from '$lib/stores/auth'
  import { MyMessageState } from '$src/lib/stores/message'

  $: username = ($page?.params || {})['username'] || ''
  $: pageKey = $authStore.identity.getPrincipal() + ':' + username
</script>

<div class="m-auto mt-4 flex max-w-4xl flex-col items-center">
  {#key pageKey}
    {#await MyMessageState.load()}
      <div class="placeholder-circle mt-8 w-32 animate-pulse sm:mt-24" />
    {:then myState}
      <ProfileDetail userId={username} {myState} />
    {/await}
  {/key}
</div>
