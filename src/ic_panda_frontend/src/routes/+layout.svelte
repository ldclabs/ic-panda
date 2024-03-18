<script lang="ts">
  import '../app.pcss'
  import { pwaInfo } from 'virtual:pwa-info'
  import {
    computePosition,
    autoUpdate,
    offset,
    shift,
    flip,
    arrow
  } from '@floating-ui/dom'
  import { browser } from '$app/environment'
  import { authStore } from '$lib/stores/auth'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import PageHeader from '$lib/components/core/PageHeader.svelte'
  import {
    AppShell,
    Toast,
    Modal,
    getToastStore,
    initializeStores,
    storePopup
  } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'

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
</svelte:head>

<svelte:window on:storage={syncAuthStore} />

<Modal position="items-start" />

<Toast position="br" width="max-w-xl w-full" zIndex="z-[10000]" />

<AppShell regionPage="scroll-smooth" scrollbarGutter="stable both-edges">
  <svelte:fragment slot="header"><PageHeader /></svelte:fragment>
  <slot />
  <svelte:fragment slot="pageFooter"><PageFooter /></svelte:fragment>
</AppShell>

{#await import('$lib/ReloadPrompt.svelte') then { default: ReloadPrompt }}
  <ReloadPrompt />
{/await}
