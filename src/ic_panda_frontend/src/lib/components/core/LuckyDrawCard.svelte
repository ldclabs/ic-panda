<script lang="ts">
  import IconGoldPanda from '$lib/components/icons/IconGoldPanda.svelte'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import LuckyDrawModal from './LuckyDrawModal.svelte'
  import { authStore } from '$lib/stores/auth'
  import { signIn } from '$lib/services/auth'

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
  class="flex w-[400px] max-w-full flex-col justify-center rounded-2xl bg-white p-4"
>
  <section class="mb-12 mt-6 flex flex-col justify-center">
    <h5 class="h5 text-center font-extrabold">
      <span>Lucky Draw</span>
    </h5>
    <div class="m-auto mt-12 flex flex-row gap-4">
      <div>
        <IconGoldPanda />
      </div>
      <div>
        <h2 class="h2 font-extrabold text-gold">??????</h2>
        <p class="mt-2 text-gray/50">Draw random PANDA tokens</p>
      </div>
    </div>
  </section>
  <footer class="m-auto mb-6">
    <p class="mb-3 flex flex-row justify-center gap-1 text-gold">
      <span>You need to pay ICP to participate in the draw.</span>
    </p>
    <button
      on:click={drawNowHandler}
      class="variant-filled btn btn-lg m-auto block w-[300px] max-w-full text-white"
    >
      Draw Now
    </button>
  </footer>
</div>
