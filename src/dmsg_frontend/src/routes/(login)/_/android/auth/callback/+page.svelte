<script lang="ts">
  import { onMount } from 'svelte'
  // A browser fallback never forwards or persists the delegation fragment.
  onMount(() => history.replaceState(null, '', location.pathname))
</script>

<svelte:head><title>dMsg</title><meta name="referrer" content="no-referrer" /></svelte:head>
<main class="mx-auto max-w-lg p-8">
  <h1>Return to dMsg for Android</h1>
  <p>This sign-in must finish in the app that started it. Return to dMsg and start again.</p>
</main>
