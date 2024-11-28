<script lang="ts">
  import { goto } from '$app/navigation'
  import Chat from '$lib/components/messages/Chat.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import { MyMessageState } from '$lib/stores/message'
  import { onMount } from 'svelte'

  let myState: MyMessageState | null = $state(null)
  onMount(async () => {
    myState = await MyMessageState.load()
    if (myState.principal.isAnonymous()) {
      return goto('/')
    }
  })
</script>

{#if myState}
  <Chat {myState} />
{:else}
  <div class="mx-auto pt-24"><Loading /></div>
{/if}
