<script lang="ts">
  import { goto } from '$app/navigation'
  import Loading from '$lib/components/ui/Loading.svelte'
  import { MyMessageState } from '$lib/stores/message'
  import MyProfile from '$src/lib/components/messages/MyProfile.svelte'
  import { onMount } from 'svelte'

  let myState: MyMessageState
  onMount(async () => {
    myState = await MyMessageState.load()
    if (myState.principal.isAnonymous()) {
      return goto('/')
    }
  })
</script>

{#if myState}
  <MyProfile {myState} />
{:else}
  <div class="mx-auto pt-24"><Loading /></div>
{/if}
