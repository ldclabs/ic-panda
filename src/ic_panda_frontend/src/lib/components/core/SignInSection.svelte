<script lang="ts">
  import { signIn } from '$lib/services/auth'
  import ButtonIC from '$lib/components/ui/ButtonIC.svelte'
</script>

<section
  class="relative z-10 flex flex-col items-center justify-center sm:pb-8 sm:pt-0"
>
  <h1 class="xs:px-6 mb-0.5 py-8 text-center sm:px-8" style="max-width: 600px">
    A decentralized Panda memes brand based on the Internet Computer.
  </h1>

  <ButtonIC on:click={async () => await signIn({})}>
    <svelte:fragment slot="action">Connect with</svelte:fragment>
    Internet Identity
  </ButtonIC>
</section>

<style>
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
