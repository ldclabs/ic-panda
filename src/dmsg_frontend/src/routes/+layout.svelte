<script lang="ts">
  import { browser } from '$app/environment'
  import { onMount } from 'svelte'
  import { t, locale } from '$lib/i18n'
  import '$lib/i18n/locale.css'

  // The asset canister serves this shell for deep links too. Keep its static
  // public overview until the correct client route has mounted successfully.
  onMount(() => {
    document.getElementById('dmsg-public-content')?.remove()
    document.getElementById('dmsg-reading-style')?.remove()
  })

  // Localize the SPA fallback metadata in place, without duplicate tags.
  $: if (browser) {
    // Shared links show the page's promise; the tab title names the product.
    const title = `${$t('Keep what matters private.')} ${$t('Choose who gets access.')}`
    const description = $t(
      'Protect passwords, notes and files with end-to-end encryption. Sync your vault across devices, share files securely and control AI agent access.'
    )
    const image = `https://dmsg.net/_assets/dmsg/og-image${$locale === 'en' ? '' : `-${$locale}`}.png`
    const metadata = {
      'meta[name="description"]': description,
      'meta[property="og:title"]': title,
      'meta[property="og:description"]': description,
      'meta[property="og:image:alt"]': title,
      'meta[property="og:image"]': image,
      'meta[name="twitter:image"]': image,
      'meta[name="twitter:title"]': title,
      'meta[name="twitter:description"]': description,
      'meta[name="twitter:image:alt"]': title
    }
    for (const [selector, content] of Object.entries(metadata)) {
      document.querySelector(selector)?.setAttribute('content', content)
    }
  }
</script>

<svelte:head>
  <title>{$t('dMsg — Encrypted Vault, File Sharing & AI Permissions')}</title>
</svelte:head>

<slot />
