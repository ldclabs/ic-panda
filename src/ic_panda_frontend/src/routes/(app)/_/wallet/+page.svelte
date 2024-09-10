<script lang="ts">
  import Cards from '$lib/components/wallet/Cards.svelte'
  import Saos from 'saos'
</script>

<div
  class="mb-12 mt-12 flex flex-col flex-nowrap content-center items-center px-4"
>
  <div
    class="flex w-full max-w-4xl flex-col flex-nowrap content-center items-center"
  >
    <div class="w-full max-w-[820px]">
      <Saos
        once={true}
        animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
      >
        <Cards />
      </Saos>
    </div>
  </div>
</div>

<style>
  /**
  * ----------------------------------------
  * animation slide-top http://animista.net
  * ----------------------------------------
  */
  @keyframes -global-slide-top {
    0% {
      transform: translateY(100px);
      opacity: 0;
    }
    100% {
      transform: translateY(0);
      opacity: 1;
    }
  }

  @keyframes -global-scale-down-center {
    0% {
      transform: scale(1.5);
    }
    100% {
      transform: scale(1);
    }
  }
</style>
