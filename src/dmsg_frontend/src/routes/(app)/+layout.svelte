<script lang="ts">
  import { browser } from '$app/environment'
  import ModalHost from '$lib/components/ui/ModalHost.svelte'
  import ToastHost from '$lib/components/ui/ToastHost.svelte'
  import { authStore } from '$lib/stores/auth'
  import { setInitialClassState } from '$lib/ui/mode'
  import { getToastStore } from '$lib/ui/stores'
  import { initReconnect, isOnline } from '$lib/utils/window'
  import '$src/app.css'
  import Loading from '$src/lib/components/ui/Loading.svelte'
  import { Tooltip } from 'bits-ui'
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
  const toastStore = getToastStore()

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

<ModalHost />

<ToastHost />

<!-- One provider for every HoverPopup in the app. -->
<Tooltip.Provider delayDuration={150}>
  <div
    class="relative grid h-full w-full grid-cols-1 overflow-x-hidden overflow-y-auto scroll-smooth"
  >
    {#if globalLoading.value}
      <Loading />
    {:else}
      {@render children_render?.()}
    {/if}
  </div>
</Tooltip.Provider>
