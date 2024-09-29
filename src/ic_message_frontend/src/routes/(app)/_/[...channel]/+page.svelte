<script lang="ts">
  import Chat from '$lib/components/messages/Chat.svelte'
  import { MyMessageState } from '$lib/stores/message'
  import { onMount } from 'svelte'
  import Loading from '$lib/components/ui/Loading.svelte'

  let myState: MyMessageState
  onMount(async () => {
    myState = await MyMessageState.load()
  })
</script>

{#if myState}
  <Chat {myState} />
{:else}
  <div class="mx-auto pt-24"><Loading /></div>
{/if}
