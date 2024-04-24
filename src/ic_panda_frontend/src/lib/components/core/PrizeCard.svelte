<script lang="ts">
  import { page } from '$app/stores'
  import {
    luckyPoolAPIAsync,
    type LuckyPoolAPI,
    type State
  } from '$lib/canisters/luckypool'
  import IconGoldPanda2 from '$lib/components/icons/IconGoldPanda2.svelte'
  import IconHistory from '$lib/components/icons/IconHistory.svelte'
  import IconQrCode from '$lib/components/icons/IconQrCode.svelte'
  import QrCodeReaderModal from '$lib/components/ui/QRCodeReaderModal.svelte'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { decodePrize } from '$lib/types/prize'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import PrizeCreateModal from './PrizeCreateModal.svelte'
  import PrizeHistoryModal from './PrizeHistoryModal.svelte'
  import PrizeModal from './PrizeModal.svelte'

  let stateStore: Readable<State | null>
  let luckyPoolAPI: LuckyPoolAPI
  let prizeSubsidy: [] | [bigint, number, number, number, number, number] = []

  const modalStore = getModalStore()

  function claimPrizeHandler() {
    if (principal.isAnonymous()) {
      signIn()
    } else {
      modalStore.trigger({
        type: 'component',
        component: { ref: PrizeModal, props: {} }
      })
    }
  }

  async function qrPrizeHandler() {
    if (principal.isAnonymous()) {
      signIn()
      return
    }

    let code: string = await new Promise((resolve) => {
      modalStore.trigger({
        type: 'component',
        component: { ref: QrCodeReaderModal, props: {} },
        response: (res) => resolve(res || '')
      })
    })

    modalStore.trigger({
      type: 'component',
      component: { ref: PrizeModal, props: { prizeCode: code } }
    })
  }

  function createPrizeHandler() {
    if (principal.isAnonymous()) {
      signIn()
    } else {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: PrizeCreateModal,
          props: { prizeSubsidy: prizeSubsidy }
        }
      })
    }
  }

  function prizeHistoryHandler() {
    if (principal.isAnonymous()) {
      signIn()
    } else {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: PrizeHistoryModal,
          props: {}
        }
      })
    }
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()

    if (decodePrize($page.url.searchParams.get('prize') || '')) {
      modalStore.close() // close previous modal
      modalStore.trigger({
        type: 'component',
        component: { ref: PrizeModal, props: {} }
      })
    }
  })

  $: principal = $authStore.identity.getPrincipal()
  $: {
    if (luckyPoolAPI) {
      stateStore = luckyPoolAPI.stateStore
      prizeSubsidy = $stateStore?.prize_subsidy[0] || []

      if (luckyPoolAPI?.principal.toString() != principal.toString()) {
        luckyPoolAPIAsync().then((_luckyPoolAPI) => {
          luckyPoolAPI = _luckyPoolAPI
        })
      }
    }
  }
</script>

<div class="rounded-2xl bg-gradient-to-r from-amber-50 to-red-50">
  <div
    class="relative flex flex-col justify-center bg-[url('/_assets/images/prize-giveaway-bg.webp')] bg-[length:100%_auto] bg-no-repeat p-4"
  >
    <div class="absolute right-4 top-4 flex flex-row gap-3">
      <button on:click={prizeHistoryHandler} class="btn btn-sm bg-white">
        <span class="*:size-5"><IconHistory /></span>
        <span>History</span>
      </button>
      <button
        disabled={prizeSubsidy.length == 0}
        on:click={createPrizeHandler}
        class="btn btn-sm bg-white"
      >
        <span class="*:size-5"><IconGoldPanda2 /></span>
        <span>Create a Prize</span>
      </button>
    </div>
    <section class="mb-6 mt-5 flex flex-col justify-center max-md:mt-10">
      <h5 class="h5 text-center font-extrabold">
        <span>PANDA Prize Giveaway</span>
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
            class="badge-icon bg-gradient-to-r from-amber-300 to-red-500 p-4 text-white"
            >1</span
          >
          <span class="flex-auto">
            Have your own <b>Lucky Code</b> through airdrop.
          </span>
        </li>
        <li>
          <span
            class="badge-icon bg-gradient-to-r from-amber-300 to-red-500 p-4 text-white"
            >2</span
          >
          <span class="flex-auto">
            Have at least <b>50 PANDA</b> in your <b>Lucky Balance</b>.
          </span>
        </li>
        <li>
          <span
            class="badge-icon bg-gradient-to-r from-amber-300 to-red-500 p-4 text-white"
            >3</span
          >
          <span class="flex-auto">
            Each reward has a limit on quantity. First come, first served.
          </span>
        </li>
      </ol>
      <div class="relative mt-4 text-center">
        <button
          on:click={claimPrizeHandler}
          class="btn m-auto mt-3 w-[320px] max-w-full bg-gradient-to-r from-amber-300 to-red-500 font-medium text-white transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
        >
          Claim a Prize
        </button>
        <div class="">
          <button
            on:click={qrPrizeHandler}
            class="btn btn-icon text-orange-600 *:size-8 sm:absolute sm:right-0 sm:top-[calc(50%-16px)]"
          >
            <IconQrCode />
          </button>
        </div>
      </div>
    </footer>
  </div>
</div>
