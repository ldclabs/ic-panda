<script lang="ts">
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import LuckyDrawModal from './LuckyDrawModal.svelte'

  const modalStore = getModalStore()

  function drawNowHandler() {
    if ($authStore.identity.getPrincipal().isAnonymous()) {
      signIn({})
    } else {
      modalStore.trigger({
        type: 'component',
        component: { ref: LuckyDrawModal }
      })
    }
  }
</script>

<div
  class="relative flex flex-col items-center rounded-2xl bg-[#f3fffb] bg-[url('/_assets/images/lucky-pool-draw-bg.webp')] bg-[length:100%_auto] bg-no-repeat p-4"
>
  <div class="absolute bottom-0 -z-0 h-20 w-full rounded-b-2xl bg-[#bff0e0]"
  ></div>
  <img
    class="z-10 mt-5 block w-full max-w-[400px]"
    src="/_assets/images/lucky-pool-draw.webp"
    alt="Lucky Draw"
  />
  <footer class="absolute bottom-8 left-0 right-0 z-20 m-auto">
    <p class="mb-3 flex flex-row justify-center gap-1 text-white">
      <span>Pay ICP to participate in the draw.</span>
    </p>
    <button
      on:click={drawNowHandler}
      class="variant-filled btn m-auto block w-[300px] max-w-full text-white transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
    >
      Draw Now
    </button>
  </footer>
</div>
