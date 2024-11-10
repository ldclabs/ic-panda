<script lang="ts">
  import WalletDetailModal from '$lib/components/core/WalletDetailModal.svelte'
  import IconExchange from '$lib/components/icons/IconExchangeDollar.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconHomeLine from '$lib/components/icons/IconHomeLine.svelte'
  import IconLogout from '$lib/components/icons/IconLogout.svelte'
  import IconRefresh from '$lib/components/icons/IconRefresh.svelte'
  import IconWallet from '$lib/components/icons/IconWallet.svelte'
  import { APP_VERSION } from '$lib/constants'
  import { authStore } from '$lib/stores/auth'
  import { getModalStore } from '@skeletonlabs/skeleton'

  interface Props {
    target: string
  }

  let { target }: Props = $props()

  const modalStore = getModalStore()

  function onWalletHandler(): void {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: WalletDetailModal,
        props: {}
      }
    })
  }

  function onLogoutHandler(): void {
    authStore.signOut('/')
  }
</script>

<div class="card z-20 w-52 bg-white px-0 py-2 shadow-lg" data-popup={target}>
  <div
    class="flex flex-col items-start text-sm *:bg-surface-hover-token *:flex *:w-full *:flex-row *:gap-2 *:px-3 *:py-2"
  >
    <button type="button" onclick={onWalletHandler}>
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
    <button type="button" onclick={() => window.location.reload()}>
      <span class="*:size-5"><IconRefresh /></span>
      <span>Reload App</span>
      <span class="text-surface-500">(v{APP_VERSION})</span>
    </button>
    <button
      type="button"
      class="border-t border-surface-500/20"
      onclick={onLogoutHandler}
    >
      <span class="*:size-5"><IconLogout /></span>
      <span>Sign Out</span>
    </button>
  </div>
</div>
