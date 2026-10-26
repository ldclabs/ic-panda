<script lang="ts">
  // Outside the initiating system session this page discards the response.
  // It never forwards a delegation or accepts a caller-selected destination.
  import UtilityPage from '$lib/components/site/UtilityPage.svelte'
  import { t } from '$lib/i18n'
  import { onMount } from 'svelte'

  onMount(() => history.replaceState(null, '', location.pathname))
</script>

<svelte:head>
  <title>dMsg</title>
  <meta name="referrer" content="no-referrer" />
</svelte:head>

<UtilityPage>
  <h1>{$t('Return to the dMsg iOS app')}</h1>
  <p class="utility-lead"
    >{$t(
      'This sign-in must finish in the app that started it. Return to dMsg and start again.'
    )}</p
  >
</UtilityPage>
