<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { authStore } from '$lib/stores/auth'
  import IconUser from '$lib/components/icons/IconUser1.svelte'
  import IconMessage3Line from '$lib/components/icons/IconMessage3Line.svelte'
  import IconWallet from '$lib/components/icons/IconWallet.svelte'
  import IconLogout from '$lib/components/icons/IconLogout.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconExchange from '$lib/components/icons/IconExchangeDollar.svelte'
  import IconMoreFill from '$lib/components/icons/IconMoreFill.svelte'
  import IconHomeLine from '$lib/components/icons/IconHomeLine.svelte'
  import IconRefresh from '$lib/components/icons/IconRefresh.svelte'
  import { initPopup } from '$lib/utils/popup'
  import { onDestroy } from 'svelte'
  import WalletDetailModal from '$lib/components/core/WalletDetailModal.svelte'
  import { getModalStore } from '@skeletonlabs/skeleton'

  const modalStore = getModalStore()

  $: principal = $authStore.identity.getPrincipal()
  $: selectedProfile = selected('Profile', $page.url?.pathname || '')
  $: selectedMessages = selected('Messages', $page.url?.pathname || '')

  const { popupOpenOn, popupDestroy } = initPopup({
    target: 'popupNavigationMore'
  })

  function selected(
    tab: 'Profile' | 'Messages' | 'More',
    pathname: string
  ): boolean {
    if (pathname.startsWith('/_')) {
      if (pathname.startsWith('/_/profile')) {
        return tab === 'Profile'
      } else {
        return tab === 'Messages'
      }
    }
    return tab === 'More'
  }

  function onWalletHandler(): void {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: WalletDetailModal,
        props: {}
      }
    })
  }

  async function onLogoutHandler(): Promise<void> {
    await goto('/')
    await authStore.signOut()
  }

  onDestroy(() => {
    popupDestroy()
  })
</script>

{#key principal.toText()}
  <div
    class="mx-auto grid h-dvh w-full bg-white shadow-md max-md:grid-rows-[1fr_auto] md:max-w-5xl md:grid-cols-[auto_1fr]"
  >
    <slot />
    <div
      class="nav grid items-start gap-2 border-surface-500/20 bg-gray-100/50 *:flex *:flex-col *:items-center *:justify-center *:py-1 *:text-xs max-md:h-[60px] max-md:grid-cols-3 max-md:border-t md:order-first md:grid-rows-[auto_1fr_auto] md:border-r md:p-2"
    >
      <a
        href="/_/profile"
        role="button"
        class="transition-all {selectedProfile
          ? 'cursor-default text-primary-500'
          : 'hover:scale-105'}"
      >
        <span><IconUser /></span>
        <span>Profile</span>
      </a>
      <a
        href="/_/messages"
        role="button"
        class="transition-all {selectedMessages
          ? 'cursor-default text-primary-500'
          : 'hover:scale-105'}"
      >
        <span><IconMessage3Line /></span>

        <span>Messages</span>
      </a>
      <button
        class="btn px-0 transition-all hover:scale-105"
        on:click={(ev) => {
          popupOpenOn(ev.currentTarget)
        }}
      >
        <span><IconMoreFill /></span>
        <span class="!m-0">More</span>
      </button>
    </div>
  </div>
{/key}
<div
  class="card z-20 w-52 bg-white px-0 py-2 shadow-lg"
  data-popup="popupNavigationMore"
>
  <div
    class="flex flex-col items-start text-sm text-neutral-500 *:bg-surface-hover-token *:flex *:w-full *:flex-row *:gap-2 *:px-3 *:py-2"
  >
    <button type="button" on:click={onWalletHandler}>
      <span class="*:size-5"><IconWallet /></span>
      <span>Wallet</span>
    </button>
    <a type="button" href="/">
      <span class="*:size-5"><IconHomeLine /></span>
      <span>Home Page</span>
    </a>
    <a
      type="button"
      target="_blank"
      href="https://app.icpswap.com/swap?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
    >
      <span class="*:size-5"><IconExchange /></span>
      <span>Exchange $PANDA</span>
    </a>
    <a type="button" target="_blank" href="https://github.com/ldclabs/ic-panda">
      <span class="*:size-5"><IconGithub /></span>
      <span>Source Code</span>
    </a>
    <button type="button" on:click={() => window.location.reload()}>
      <span class="*:size-5"><IconRefresh /></span>
      <span>Reload App</span>
    </button>
    <button
      type="button"
      class="border-t border-surface-500/20"
      on:click={onLogoutHandler}
    >
      <span class="*:size-5"><IconLogout /></span>
      <span>Sign Out</span>
    </button>
  </div>
</div>

<style>
  @media (min-width: 768px) {
    .nav {
      background: linear-gradient(
        180deg,
        rgba(17, 194, 145, 0.1) -2%,
        rgba(255, 255, 255, 0) 35%
      );
    }
  }
</style>
