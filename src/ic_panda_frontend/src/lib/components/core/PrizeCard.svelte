<script lang="ts">
  import { page } from '$app/stores'
  import {
    luckyPoolAPIAsync,
    type AirdropState,
    type LuckyPoolAPI
  } from '$lib/canisters/luckypool'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { decodePrize } from '$lib/types/prize'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import PrizeModal from './PrizeModal.svelte'

  let airdropState: Readable<AirdropState | null>
  let luckyPoolAPI: LuckyPoolAPI
  let claimableAmount = 0n

  const modalStore = getModalStore()

  function claimPrizeHandler() {
    if (principal.isAnonymous()) {
      signIn({})
    } else {
      modalStore.trigger({
        type: 'component',
        component: { ref: PrizeModal },
        meta: { claimableAmount: claimableAmount }
      })
    }
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()

    await new Promise((res) => setTimeout(res, 400))
    if (
      decodePrize($page.url.searchParams.get('prize') || '') &&
      !principal.isAnonymous()
    ) {
      claimPrizeHandler()
    }
  })

  $: principal = $authStore.identity.getPrincipal()
  $: {
    if (luckyPoolAPI) {
      airdropState = luckyPoolAPI.airdropStateStore
      claimableAmount = $airdropState?.claimable || 0n
      if (luckyPoolAPI?.principal.toString() != principal.toString()) {
        luckyPoolAPIAsync().then((_luckyPoolAPI) => {
          luckyPoolAPI = _luckyPoolAPI
        })
      }
    }
  }
  // bg-[url('/_assets/images/prize-giveaway-bg.webp')]
</script>

<div class="to-red-50 from-amber-50 rounded-2xl bg-gradient-to-r">
  <div
    class="  flex flex-col justify-center bg-[url('/_assets/images/prize-giveaway-bg.webp')] bg-[length:100%_auto] bg-no-repeat p-4"
  >
    <section class="mb-6 mt-5 flex flex-col justify-center">
      <h5 class="h5 text-center font-extrabold">
        <span>Prize Giveaway</span>
      </h5>
    </section>
    <footer class="m-auto mb-6">
      <!-- Anonymous -->
      <p class="text-sm text-gray/100"
        >Before claiming, please ensure that you meet the following conditions:</p
      >
      <ol class="list *:!mt-4 *:!rounded-xl *:bg-white *:px-4 *:py-2">
        <li>
          <span
            class="to-red-500 from-amber-300 badge-icon bg-gradient-to-r p-4 text-white"
            >1</span
          >
          <span class="flex-auto">
            Have your own <b>Lucky Code</b> through airdrop.
          </span>
        </li>
        <li>
          <span
            class="to-red-500 from-amber-300 badge-icon bg-gradient-to-r p-4 text-white"
            >2</span
          >
          <span class="flex-auto">
            Have at least <b>10 PANDA</b> in your wallet.
          </span>
        </li>
        <li>
          <span
            class="to-red-500 from-amber-300 badge-icon bg-gradient-to-r p-4 text-white"
            >3</span
          >
          <span class="flex-auto">
            Each reward has a limit on quantity. First come, first served.
          </span>
        </li>
      </ol>
      <div class="mt-4 flex flex-col items-center">
        <button
          on:click={claimPrizeHandler}
          class="to-red-500 from-amber-300 btn m-auto mt-3 w-[320px] max-w-full bg-gradient-to-r font-medium text-white transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
        >
          Claim a Prize
        </button>
      </div>
    </footer>
  </div>
</div>
