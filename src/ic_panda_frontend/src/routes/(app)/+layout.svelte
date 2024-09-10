<script lang="ts">
  import { browser } from '$app/environment'
  import PageHeader from '$lib/components/core/PageHeader.svelte'
  import { GOOGLE_RECAPTCHA_ID } from '$lib/constants'
  import { authStore } from '$lib/stores/auth'
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
  const toastStore = getToastStore()

  /**
   * Init authentication
   */

  const syncAuthStore = async () => {
    if (!browser) {
      return
    }

    try {
      await authStore.sync()
    } catch (err: unknown) {
      toastStore.trigger({
        message: String(err),
        background: 'variant-filled-error',
        timeout: 5000,
        hoverable: true
      })
    }
  }

  onMount(syncAuthStore)

  /**
   * UI loader
   */

  // To improve the UX while the app is loading on mainnet we display a spinner which is attached statically in the index.html files.
  // Once the authentication has been initialized we know most JavaScript resources has been loaded and therefore we can hide the spinner, the loading information.
  $: (() => {
    if (!browser) {
      return
    }

    // We want to display a spinner until the authentication is loaded. This to avoid a glitch when either the landing page or effective content (sign-in / sign-out) is presented.
    if ($authStore == null) {
      return
    }

    const spinner = document.querySelector('body > #app-spinner')
    spinner?.remove()
  })()
  $: webManifest = pwaInfo ? pwaInfo.webManifest.linkTag : ''
</script>

<svelte:head>
  {@html webManifest}
  <script
    src="https://www.google.com/recaptcha/enterprise.js?render={GOOGLE_RECAPTCHA_ID}&badge=inline"
    async
    defer
  ></script>
</svelte:head>

<svelte:window on:storage={syncAuthStore} />

<Modal position="items-start" class="*:max-h-full" />

<Toast position="br" width="max-w-xl w-full" zIndex="z-[10000]" />

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

{#await import('$lib/ReloadPrompt.svelte') then { default: ReloadPrompt }}
  <ReloadPrompt />
{/await}
