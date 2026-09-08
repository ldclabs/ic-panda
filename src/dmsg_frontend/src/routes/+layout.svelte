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
          'A private workspace for your secrets, chosen collaborators, and explicit signatures. Chrome extension in development. Legacy app available in read-only mode.'
        )
      )
  }
</script>

<svelte:head>
  <title>{$t('dMsg — Your space. Your say.')}</title>
</svelte:head>

<slot />
