<script lang="ts">
  import { TabGroup, TabAnchor, getModalStore } from '@skeletonlabs/skeleton'
  import type { ModalSettings } from '@skeletonlabs/skeleton'
  import { page } from '$app/stores'
  import { authStore } from '$lib/stores/auth'
  import { signIn } from '$lib/services/auth'
  import { onMount } from 'svelte'
  import ButtonIC from '$lib/components/ui/ButtonIC.svelte'
  import IconAccount from '$lib/components/icons/IconAccount.svelte'

  const modalStore = getModalStore()

  onMount(async () => {
    await setTimeout(Promise.resolve, 5000)
    console.log('authStore', $authStore)
  })

  async function handleSignIn() {
    await signIn({})
  }

  function showAccountDetail() {
    if (!$authStore.identity) {
      return
    }

    const modal: ModalSettings = {
      type: 'component',
      title: 'Account',
      body: 'Your Principal: ' + $authStore.identity?.getPrincipal().toString(),
      component: 'modalAccountDetail'
    }
    modalStore.trigger(modal)
  }
</script>

<div class="relative border-none p-0 py-0">
  <TabGroup
    justify="justify-left md:justify-center"
    border="border-none"
    padding="px-2 py-2 md:px-6 md:py-3"
    active="border-b-4 border-primary-500/80"
    hover="hover:bg-primary-500/10"
  >
    <TabAnchor href="#luckypool" selected={$page.url.hash === '#luckypool'}>
      Luckypool
    </TabAnchor>
    <TabAnchor href="#tokenomics" selected={$page.url.hash === '#tokenomics'}>
      Tokenomics
    </TabAnchor>
    <TabAnchor href="#roadmap" selected={$page.url.hash === '#roadmap'}>
      Roadmap
    </TabAnchor>
  </TabGroup>

  <div class="absolute right-0 top-0 flex h-full flex-row p-2">
    {#if $authStore.identity}
      <button
        class=" btn bg-panda/10 transition duration-500 ease-in-out hover:bg-panda/20"
        on:click={showAccountDetail}
      >
        <span class="scale-110"><IconAccount /></span>
      </button>
    {:else}
      <ButtonIC on:click={handleSignIn} />
    {/if}
  </div>
</div>
