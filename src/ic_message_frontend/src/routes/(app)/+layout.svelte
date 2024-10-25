<script lang="ts">
  import { browser } from '$app/environment'
  import { authStore, fetchRootKey } from '$lib/stores/auth'
  import { storePopup as storePopup2 } from '$lib/utils/popup'
  import { initReconnect, isOnline } from '$lib/utils/window'
  import '$src/app.pcss'
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
    setInitialClassState,
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
      setInitialClassState()
      await fetchRootKey()

      try {
        await authStore.sync()
      } catch (err) {}
    }

    initAuth = true
  })
</script>

<svelte:head>
  {#if pwaInfo?.webManifest.linkTag}
    {@html pwaInfo.webManifest.linkTag}
  {/if}
</svelte:head>

<Modal position="items-start" class="*:max-h-full" />

<Toast position="br" width="max-w-xl w-full" zIndex="z-[10000]" />

{#if initAuth}
  <div
    class="relative grid h-full w-full grid-cols-1 overflow-y-auto overflow-x-hidden scroll-smooth"
  >
    <slot />
  </div>
{/if}

{#await import('$lib/ReloadPrompt.svelte') then { default: ReloadPrompt }}
  <ReloadPrompt />
{/await}
