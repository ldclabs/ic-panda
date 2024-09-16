<script lang="ts">
  import { page } from '$app/stores'
  import ProfileDetail from '$lib/components/messages/ProfileDetail.svelte'

  $: username = ($page?.params || {})['username'] || ''
</script>

<div class="m-auto mt-4 flex max-w-4xl flex-col">
  {#key username}
    <ProfileDetail userId={username} />
  {/key}
</div>
