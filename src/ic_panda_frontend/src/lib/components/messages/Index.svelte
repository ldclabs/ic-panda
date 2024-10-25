<script lang="ts">
  import Loading from '$lib/components/ui/Loading.svelte'
  import { MyMessageState } from '$lib/stores/message'
  import { onMount } from 'svelte'
  import Home from './Home.svelte'

  let myState: MyMessageState

  onMount(async () => {
    myState = await MyMessageState.load()
  })
</script>

{#if myState}
  <Home {myState} />
{:else}
  <Loading />
{/if}
