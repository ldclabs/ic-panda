<script lang="ts">
  import { browser } from '$app/environment'
  import { t } from '$lib/i18n'
  import '$lib/i18n/locale.css'

  // Update the static fallback in place so there is only one description tag.
  $: if (browser) {
    document
      .querySelector('meta[name="description"]')
      ?.setAttribute(
        'content',
        $t(
          'An encrypted workspace on your computer. The desktop app, the dmsg command line and your AI agents work through it, and nothing is signed until you approve.'
        )
      )
  }
</script>

<svelte:head>
  <title>{$t('dMsg — Your space. Your say.')}</title>
</svelte:head>

<slot />
