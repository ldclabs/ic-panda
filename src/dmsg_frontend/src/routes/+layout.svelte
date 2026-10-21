<script lang="ts">
  import { browser } from '$app/environment'
  import { t, locale } from '$lib/i18n'
  import '$lib/i18n/locale.css'

  // Localize the SPA fallback metadata in place, without duplicate tags.
  $: if (browser) {
    const title = $t('dMsg — Private collaboration for people and agents')
    const description = $t(
      'Keep secrets and files private. Collaborate with your team. Give your agents their own identities and permissions to work across an open network.'
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
  <title>{$t('dMsg — Private collaboration for people and agents')}</title>
</svelte:head>

<slot />
