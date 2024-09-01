<script lang="ts">
  import { page } from '$app/stores'
  import AccountDetailModal from '$lib/components/core/AccountDetailModal.svelte'
  import IconUser0 from '$lib/components/icons/IconUser0.svelte'
  import IconUser1 from '$lib/components/icons/IconUser1.svelte'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { TabAnchor, TabGroup, getModalStore } from '@skeletonlabs/skeleton'

  const modalStore = getModalStore()

  async function handleSignIn() {
    await signIn({})
  }

  function showAccountDetail() {
    modalStore.trigger({
      type: 'component',
      component: { ref: AccountDetailModal }
    })
  }

  function scrollToTop(ev: MouseEvent) {
    // AppShell page
    window.document.getElementById('page')?.scrollTo(0, 0)
    if (ev.detail == 2) {
      window.location.reload() // for PWA that has no refresh button
    }
  }
</script>

<div class="relative mt-3 border-none p-0 max-md:mt-1">
  <TabGroup
    justify="justify-left md:justify-center"
    border="border-none"
    padding="px-2 py-2 md:px-6 md:py-3"
    active="border-b-4 border-primary-500/80"
    hover="hover:bg-primary-500/10"
    class="overflow-y-hidden first:ml-2 max-md:max-w-[calc(100vw-100px)]"
  >
    <TabAnchor
      href="/"
      selected={$page.url.pathname == '/' && $page.url.hash === ''}
      on:click={scrollToTop}
    >
      Home
    </TabAnchor>
    <TabAnchor
      href="/_/messages"
      selected={$page.url.pathname.startsWith('/messages')}
    >
      Messages
    </TabAnchor>
    <!-- <TabAnchor
      href="/wallet"
      selected={$page.url.pathname.startsWith('/wallet')}
    >
      Chain Fusion
    </TabAnchor> -->
    <TabAnchor href="/#luckypool" selected={$page.url.hash === '#luckypool'}>
      Lucky Pool
    </TabAnchor>
    <TabAnchor href="/#tokenomics" selected={$page.url.hash === '#tokenomics'}>
      Tokenomics
    </TabAnchor>
  </TabGroup>

  <div class="absolute right-10 top-0 flex h-full flex-row py-2 max-md:right-4">
    {#if $authStore.identity.getPrincipal().isAnonymous()}
      <button
        type="button"
        class="btn bg-white/80 transition duration-500 ease-in-out hover:bg-white/100 hover:shadow-md max-md:px-3"
        on:click={handleSignIn}
      >
        <span class="max-md:hidden"><IconUser0 /></span>
        <span class="max-md:!ml-0">Login</span>
      </button>
    {:else}
      <button
        class="btn btn-icon size-10 bg-white/80 transition duration-500 ease-in-out hover:bg-white/100 hover:shadow-md max-md:size-7"
        on:click={showAccountDetail}
      >
        <span class="md:scale-110"><IconUser1 /></span>
      </button>
    {/if}
  </div>
</div>
