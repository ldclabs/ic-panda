<script lang="ts">
  import { goto } from '$app/navigation'
  import { MyMessageState } from '$lib/stores/message'
  import ProfileDetail from '$lib/components/messages/ProfileDetail.svelte'
  import { onMount } from 'svelte'

  let myState: MyMessageState | undefined = $state()
  let error = $state('')
  async function load() {
    error = ''
    try {
      const state = await MyMessageState.load()
      if (state.principal.isAnonymous() || !state.api.myInfo)
        return goto('/legacy')
      myState = state
    } catch (cause) {
      error =
        cause instanceof Error
          ? cause.message
          : 'Could not load legacy profile.'
    }
  }
  onMount(() => {
    void load()
  })
</script>

<svelte:head
  ><title>dMsg — Legacy profile</title><meta
    name="robots"
    content="noindex"
  /></svelte:head
>
{#if myState}<div class="mx-auto max-w-3xl p-6"
    ><p class="archive-notice">Legacy profile · Read-only</p><ProfileDetail
      {myState}
      userId={myState.principal}
    /></div
  >
{:else if error}<div class="archive-empty"
    ><p role="alert">{error}</p><button class="button secondary" onclick={load}
      >Try again</button
    ></div
  >
{:else}<p class="archive-empty" role="status">Loading legacy profile…</p>{/if}
