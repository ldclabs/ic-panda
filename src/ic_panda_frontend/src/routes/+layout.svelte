<script lang="ts">
  import "../app.pcss";

  import { AppShell, Toast, getToastStore, initializeStores } from '@skeletonlabs/skeleton';
  import { browser } from '$app/environment';
	import { authStore } from '$lib/stores/auth';
	import { onMount } from 'svelte';

  initializeStores();
  const toastStore = getToastStore();

	/**
	 * Init authentication
	 */

	const syncAuthStore = async () => {
		if (!browser) {
			return;
		}

		try {
			await authStore.sync();
		} catch (err: unknown) {
      toastStore.trigger({
        message: String(err),
        background: 'variant-filled-error',
        timeout: 5000,
        hoverable: true,
      });
    }
	};

  onMount(syncAuthStore);

	/**
	 * UI loader
	 */

	// To improve the UX while the app is loading on mainnet we display a spinner which is attached statically in the index.html files.
	// Once the authentication has been initialized we know most JavaScript resources has been loaded and therefore we can hide the spinner, the loading information.
	$: (() => {
		if (!browser) {
			return;
		}

		// We want to display a spinner until the authentication is loaded. This to avoid a glitch when either the landing page or effective content (sign-in / sign-out) is presented.
		if ($authStore === undefined) {
			return;
		}

		const spinner = document.querySelector('body > #app-spinner');
		spinner?.remove();
	})();
</script>

<svelte:window on:storage={syncAuthStore} />

<Toast position="br" width="max-w-xl w-full"/>
<AppShell>
	<slot />
</AppShell>
