<script lang="ts">
  import { browser } from '$app/environment'
  import PageHeader from '$lib/components/core/PageHeader.svelte'
  import { authStore, fetchRootKey } from '$lib/stores/auth'
  import { initReconnect, isOnline } from '$lib/utils/window'
  import '$src/app.pcss'
  import { storePopup as storePopup2 } from '$src/lib/utils/Popup'
  import {
    arrow,
    autoUpdate,
    computePosition,
    flip,
    offset,
    shift
  } from '@floating-ui/dom'
  import {
    Modal,
    Toast,
    getToastStore,
    initializeStores,
    storePopup
  } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { pwaInfo } from 'virtual:pwa-info'

  initReconnect(
    () => console.log('Device is online:', isOnline()),
    () =>
      toastStore.trigger({
        hideDismiss: false,
        message: 'Device is offline',
        background: 'variant-filled-error',
        timeout: 5000,
        hoverable: true
      })
  )
  initializeStores()
  storePopup.set({ computePosition, autoUpdate, offset, shift, flip, arrow })
  storePopup2.set({ computePosition, autoUpdate, offset, shift, flip, arrow })
  const toastStore = getToastStore()

  /**
   * Init authentication
   */

  let initAuth = false
  onMount(async () => {
    if (browser) {
      await fetchRootKey()

      try {
        await authStore.sync()
      } catch (err) {}

      const spinner = document.querySelector('body > #app-spinner')
      spinner?.remove()
    }

    initAuth = true
  })

  $: webManifest = pwaInfo ? pwaInfo.webManifest.linkTag : ''
</script>

<svelte:head>
  {@html webManifest}
</svelte:head>

<Modal position="items-start" class="*:max-h-full" />

<Toast position="br" width="max-w-xl w-full" zIndex="z-[10000]" />

{#if initAuth}
  <div id="appShell" class="flex h-full w-full flex-col overflow-hidden">
    <header id="shell-header" class="z-10 flex-none">
      <PageHeader />
    </header>

    <div
      id="page"
      class="flex flex-1 flex-col overflow-x-hidden scroll-smooth"
      style:scrollbar-gutter="stable both-edges"
      on:scroll
    >
      <main id="page-content" class="flex-auto"><slot /></main>
    </div>
  </div>
{/if}

{#await import('$lib/ReloadPrompt.svelte') then { default: ReloadPrompt }}
  <ReloadPrompt />
{/await}
