<script lang="ts">
	import { signIn } from '$lib/services/auth';
	import ButtonIC from '$lib/components/ui/ButtonIC.svelte';
</script>

<section class="flex flex-col justify-center items-center sm:pt-0 sm:pb-8 relative z-10">
	<h1 class="text-center py-8 xs:px-6 sm:px-8 mb-0.5" style="max-width: 600px">
		A decentralized Panda memes brand based on the Internet Computer.
	</h1>

	<ButtonIC on:click={async () => await signIn({})}>
		<svelte:fragment slot="action">Connect with</svelte:fragment>
		Internet Identity
	</ButtonIC>
</section>

<style lang="scss">
	section {
		position: relative;

		margin: var(--padding-6x) 0 0;

		width: 100%;

		@media (min-width: 576px) and (min-height: 600px) {
			position: absolute;
			top: 50%;
			left: 50%;
			transform: translate(-50%, -50%);

			margin: 0;
		}
	}
</style>
