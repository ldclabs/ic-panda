<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  // Outside the initiating system session this page discards the response.
  // It never forwards a delegation or accepts a caller-selected destination.
  onMount(() => history.replaceState(null, '', location.pathname))
</script>

<svelte:head><title>dMsg</title><meta name="referrer" content="no-referrer" /></svelte:head>
<main class="mx-auto max-w-lg p-8">
  <h1>{$t('Return to the dMsg iOS app')}</h1>
  <p>{$t('This sign-in must finish in the app that started it. Return to dMsg and start again.')}</p>
</main>
