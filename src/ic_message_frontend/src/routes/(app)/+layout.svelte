<script lang="ts">
  import { browser } from '$app/environment'
  import { authStore } from '$lib/stores/auth'
  import { storePopup as storePopup2 } from '$lib/utils/popup'
  import { initReconnect, isOnline } from '$lib/utils/window'
  import '$src/app.pcss'
  import Loading from '$src/lib/components/ui/Loading.svelte'
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
    getModalStore,
    getToastStore,
    initializeStores,
    setInitialClassState,
    storePopup,
    type ModalSettings
  } from '@skeletonlabs/skeleton'
  import { onMount, setContext, type Snippet } from 'svelte'
  import { pwaInfo } from 'virtual:pwa-info'
  interface Props {
    children?: Snippet
  }

  let { children }: Props = $props()

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
  const modalStore = getModalStore()

  ;(modalStore as any).trigger2 = (modal: ModalSettings) => {
    modalStore.update((mStore) => {
      mStore.unshift(modal)
      return mStore
    })
  }

  /**
   * Init authentication
   */

  let globalLoading = $state({ value: true })
  setContext('globalLoading', globalLoading)
  onMount(async () => {
    if (browser) {
      setInitialClassState()
      await authStore.ready()

      try {
        await authStore.sync()
      } catch (err) {}
    }

    if (pwaInfo) {
      const { registerSW } = await import('virtual:pwa-register')
      registerSW({
        immediate: true,
        onRegistered(r) {
          if (!r) return
          console.log(`SW Registered: ${r}`)
          r.update()
          setInterval(
            () => {
              console.log('Checking for sw update')
              r.update()
            },
            60 * 60 * 1000
          )
        },
        onRegisterError(error) {
          console.log('SW registration error', error)
        }
      })
    }

    globalLoading.value = false
  })

  const children_render = $derived(children)
</script>

<svelte:head>
  {#if pwaInfo?.webManifest.linkTag}
    {@html pwaInfo.webManifest.linkTag}
  {/if}
</svelte:head>

<Modal
  position="items-start"
  class="*:max-h-full"
  regionBackdrop="!bg-black/10"
/>

<Toast position="br" width="max-w-xl w-full" zIndex="z-[10000]" />

<div
  class="relative grid h-full w-full grid-cols-1 overflow-y-auto overflow-x-hidden scroll-smooth"
>
  {#if globalLoading.value}
    <Loading />
  {:else}
    {@render children_render?.()}
  {/if}
</div>
