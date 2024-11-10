<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import IconArrowLeftSLine from '$lib/components/icons/IconArrowLeftSLine.svelte'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import { authStore } from '$lib/stores/auth'
  import { MyMessageState } from '$lib/stores/message'
  import ProfileDetail from '$src/lib/components/messages/ProfileDetail.svelte'
  import { onMount } from 'svelte'

  let myState: MyMessageState | undefined = $state()
  let isDark = $derived(document.documentElement.classList.contains('dark'))
  let username = $derived(($page?.params || {})['username'] || '')
  let pageKey = $derived($authStore.identity.getPrincipal() + ':' + username)

  function onCloseHandler() {
    goto(myState?.api.myInfo ? '/_/messages' : '/')
  }

  function onGobackHandler() {
    window.history.back()
  }

  onMount(async () => {
    myState = await MyMessageState.load()
  })
</script>

<div class="mx-auto flex min-h-dvh w-full max-w-3xl flex-col">
  <div
    class="relative flex w-full flex-[1_0_auto] flex-col items-center px-4 pb-12"
  >
    <header
      class="sticky top-0 flex h-[60px] w-full flex-row items-center justify-between px-0 py-2"
    >
      <button
        class="text-surface-900-50-token btn btn-icon hover:scale-125 hover:text-black dark:hover:text-white"
        onclick={onGobackHandler}><IconArrowLeftSLine /></button
      >
      <button
        class="text-surface-900-50-token btn btn-icon hover:scale-125 hover:text-black dark:hover:text-white"
        onclick={onCloseHandler}><IconClose /></button
      >
    </header>
    {#key pageKey}
      {#if myState}
        <ProfileDetail userId={username} {myState} />
      {:else}
        <div class="placeholder-circle mt-2 w-40 animate-pulse"></div>
      {/if}
    {/key}
  </div>
  <footer
    id="page-footer"
    class="text-surface-900-50-token shrink-0 px-4 py-12"
  >
    <div class="flex h-16 flex-col items-center">
      <p class="flex flex-row items-center gap-1">
        <span class="text-sm">© 2024</span>
        <a class="" href="https://panda.fans" target="_blank"
          ><img
            class="w-28"
            src={isDark
              ? '/_assets/icpanda-dao-white.svg'
              : '/_assets/icpanda-dao.svg'}
            alt="ICPanda DAO"
          /></a
        >
      </p>
      <p class="mt-2 text-center text-sm capitalize antialiased">
        A decentralized Panda meme brand fully running on the <a
          class="underline underline-offset-4"
          href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
          target="_blank"
        >
          Internet Computer
        </a> blockchain.
      </p>
    </div>
  </footer>
</div>
