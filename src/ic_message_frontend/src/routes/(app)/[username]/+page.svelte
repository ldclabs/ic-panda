<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import ProfileDetail from '$lib/components/messages/ProfileDetail.svelte'
  import IconArrowLeftSLine from '$lib/components/icons/IconArrowLeftSLine.svelte'
  import { authStore } from '$lib/stores/auth'
  import { MyMessageState } from '$lib/stores/message'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import { onMount } from 'svelte'

  let myState: MyMessageState
  $: username = ($page?.params || {})['username'] || ''
  $: pageKey = $authStore.identity.getPrincipal() + ':' + username

  function onCloseHandler() {
    goto(myState.api.myInfo ? '/_/messages' : '/')
  }

  function onGobackHandler() {
    window.history.back()
  }

  onMount(async () => {
    myState = await MyMessageState.load()
  })
</script>

<div
  class="m-auto flex min-h-full w-full max-w-3xl flex-col items-center bg-white shadow-md"
>
  <header
    class="flex h-[60px] w-full flex-row items-center justify-between px-4 py-2"
  >
    <button
      class="btn btn-icon text-neutral-500 hover:scale-110 hover:text-neutral-950"
      on:click={onGobackHandler}><IconArrowLeftSLine /></button
    >
    <button
      class="btn btn-icon text-neutral-500 hover:scale-110 hover:text-neutral-950"
      on:click={onCloseHandler}><IconClose /></button
    >
  </header>
  {#key pageKey}
    {#if myState}
      <ProfileDetail userId={username} {myState} />
    {:else}
      <div class="placeholder-circle mt-8 w-24 animate-pulse" />
    {/if}
  {/key}
</div>
