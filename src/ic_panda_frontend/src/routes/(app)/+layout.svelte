<script lang="ts">
  import { browser } from '$app/environment'
  import { afterNavigate } from '$app/navigation'
  import ModalHost from '$lib/components/ui/ModalHost.svelte'
  import PageHeader from '$lib/components/core/PageHeader.svelte'
  import ToastHost from '$lib/components/ui/ToastHost.svelte'
  import { authStore, fetchRootKey } from '$lib/stores/auth'
  import { getToastStore } from '$lib/ui/stores'
  import { initReconnect, isOnline } from '$lib/utils/window'
  import '$src/app.css'
  import { onMount } from 'svelte'

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
   * `#page` is the scroll container, not the window, so the browser's and
   * SvelteKit's built-in hash handling (which scrolls the window) is a no-op.
   * Deep links and in-page anchors have to be scrolled explicitly.
   */
  function scrollToHash(
    hash: string | null | undefined,
    behavior: ScrollBehavior = 'smooth'
  ) {
    if (!hash || hash.length < 2) return false

    const target = document.getElementById(decodeURIComponent(hash.slice(1)))
    if (!target) return false

    target.scrollIntoView({ behavior, block: 'start' })
    return true
  }

  afterNavigate(({ to }) => scrollToHash(to?.url.hash))

  /**
   * Same-page hash links never reach `afterNavigate`, so they are caught here.
   * Capture phase, because SvelteKit's own document listener runs first and
   * calls preventDefault(). The default is left intact so the router still
   * updates the URL.
   */
  onMount(() => {
    const onClick = (ev: MouseEvent) => {
      if (ev.defaultPrevented || ev.button !== 0) return
      if (ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey) return

      const anchor = (ev.target as Element | null)?.closest?.(
        'a[href]'
      ) as HTMLAnchorElement | null
      if (!anchor || anchor.target === '_blank') return

      const url = new URL(anchor.href, window.location.href)
      if (
        url.origin !== window.location.origin ||
        url.pathname !== window.location.pathname
      ) {
        return
      }

      scrollToHash(url.hash)
    }

    const onHashChange = () => scrollToHash(window.location.hash)
    document.addEventListener('click', onClick, true)
    window.addEventListener('hashchange', onHashChange)

    return () => {
      document.removeEventListener('click', onClick, true)
      window.removeEventListener('hashchange', onHashChange)
    }
  })

  /** How long start-up waits on the network before rendering anyway. */
  const AUTH_INIT_TIMEOUT = 5000

  /**
   * Neither step is required to render the site, so neither may block it:
   * the root key only matters for verifying responses from a local replica,
   * and `sync()` only upgrades `authStore` from the anonymous identity it
   * already holds. They are caught separately so a dead replica still leaves
   * the session restore a chance to run.
   */
  async function initAuthentication() {
    try {
      await fetchRootKey()
    } catch (err) {
      console.warn('Failed to fetch the local replica root key:', err)
    }

    try {
      await authStore.sync()
    } catch (err) {
      console.warn('Failed to restore the session:', err)
    }
  }

  let initAuth = false
  onMount(async () => {
    if (browser) {
      // A replica that hangs rather than refuses would otherwise leave the
      // page stuck behind the spinner, so the wait is bounded. Authentication
      // keeps resolving in the background; the UI reacts when it lands.
      await Promise.race([
        initAuthentication(),
        new Promise((resolve) => setTimeout(resolve, AUTH_INIT_TIMEOUT))
      ])

      const spinner = document.querySelector('body > #app-spinner')
      spinner?.remove()
    }

    initAuth = true

    // Sections only exist once `initAuth` lets the page render. A cold deep
    // link should land on the section, not animate down to it.
    requestAnimationFrame(() => scrollToHash(window.location.hash, 'instant'))
  })
</script>

<ModalHost />
<ToastHost />

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
