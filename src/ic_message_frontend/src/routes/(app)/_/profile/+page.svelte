<script lang="ts">
  import ProfileDetail from '$lib/components/messages/ProfileDetail.svelte'
  import { MyMessageState } from '$lib/stores/message'
  import { onMount } from 'svelte'
  import Loading from '$lib/components/ui/Loading.svelte'

  let myState: MyMessageState
  onMount(async () => {
    myState = await MyMessageState.load()
  })
</script>

{#if myState}
  <ProfileDetail {myState} userId={myState.id} />
{:else}
  <div class="mx-auto pt-24"><Loading /></div>
{/if}
