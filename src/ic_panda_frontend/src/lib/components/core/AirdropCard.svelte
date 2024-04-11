<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import {
    luckyPoolAPIAsync,
    type AirdropState,
    type LuckyPoolAPI,
    type State
  } from '$lib/canisters/luckypool'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconGoldPanda from '$lib/components/icons/IconGoldPanda.svelte'
  import IconInfo from '$lib/components/icons/IconInfo.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { decodePrize } from '$lib/types/prize'
  import { errMessage } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { getModalStore, getToastStore, popup } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import AirdropModal from './AirdropModal.svelte'
  import PrizeModal from './PrizeModal.svelte'

  let luckyPoolState: Readable<State | null>
  let airdropState: Readable<AirdropState | null>
  let luckyPoolAPI: LuckyPoolAPI
  let totalBalance = 0n
  let claimableAmount = 0n
  let claimedAmount = 0n
  let luckyCode = ''

  const modalStore = getModalStore()
  const toastStore = getToastStore()

  function claimNowHandler() {
    if (principal.isAnonymous()) {
      signIn({})
    } else {
      modalStore.trigger({
        type: 'component',
        component: { ref: AirdropModal }
      })
    }
  }

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

  let submitting = false
  let harvested = 0n
  async function harvestHandler() {
    if (claimableAmount > 0n) {
      submitting = true
      try {
        const { claimed } = await luckyPoolAPI.harvest({
          amount: claimableAmount,
          recaptcha: []
        })
        submitting = false
        harvested = claimed - claimedAmount
        setTimeout(() => {
          harvested = 0n
        }, 5000)
        await luckyPoolAPI.refreshAllState()
      } catch (err: any) {
        submitting = false
        toastStore.trigger({
          autohide: false,
          hideDismiss: false,
          background: 'variant-filled-error',
          message: errMessage(err)
        })
      }
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
      luckyPoolState = luckyPoolAPI.stateStore
      airdropState = luckyPoolAPI.airdropStateStore
      totalBalance = $luckyPoolState?.airdrop_balance || 0n
      claimableAmount = $airdropState?.claimable || 0n
      claimedAmount = $airdropState?.claimed || 0n
      luckyCode = $airdropState?.lucky_code[0] || ''
      if (luckyPoolAPI?.principal.toString() != principal.toString()) {
        luckyPoolAPIAsync().then((_luckyPoolAPI) => {
          luckyPoolAPI = _luckyPoolAPI
        })
      }

      if (luckyCode && $page.url.searchParams.get('ref')) {
        const query = $page.url.searchParams
        query.delete('ref')
        goto(`?${query.toString()}`)
      }
    }
  }
</script>

<div
  class="flex flex-col justify-center rounded-2xl bg-white bg-[url('/_assets/images/lucky-pool-bg.webp')] bg-contain bg-no-repeat p-4"
>
  <section class="mb-12 mt-6 flex flex-col justify-center">
    <h5 class="h5 text-center font-extrabold">
      <span>Free PANDA Airdrop</span>
    </h5>
    <div class="m-auto mt-12 flex flex-row gap-4">
      <div
        class="*:rounded-full *:transition *:duration-700 *:ease-in-out *:hover:scale-125 *:hover:shadow-lg"
      >
        <IconGoldPanda />
      </div>
      <div>
        <h2 class="h2 font-extrabold text-gold">
          {formatNumber(Number(totalBalance / PANDAToken.one))}
        </h2>
        <button
          class="mt-2 flex flex-row items-center gap-1 text-gray/50"
          use:popup={{
            event: 'click',
            target: 'AirdropTipHover',
            middleware: {
              size: { availableWidth: 300, availableHeight: 40 }
            }
          }}
        >
          <span>Current available balance</span>
          <span>
            <IconInfo />
          </span>
        </button>
        <div
          class="card max-w-80 bg-surface-800 px-2 py-0 text-white"
          data-popup="AirdropTipHover"
        >
          <p class="min-w-0 text-pretty break-words">
            We will gradually increase the number of PANDA tokens available for
            airdrop to ensure an orderly distribution.
          </p>
          <div class="arrow bg-surface-800" />
        </div>
      </div>
    </div>
  </section>
  <footer class="m-auto mb-6">
    {#if luckyCode == ''}
      <!-- Anonymous -->
      <p class="flex flex-row justify-center gap-1 text-gold">
        <span>You can get</span>
        <span>
          {formatNumber(Number(claimableAmount / PANDAToken.one))}
        </span>
        <span>PANDA tokens for free</span>
      </p>
      <button
        disabled={claimableAmount === 0n ||
          totalBalance < claimableAmount + PANDAToken.fee}
        on:click={claimNowHandler}
        class="variant-filled-error btn m-auto mt-3 w-[320px] max-w-full text-white transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
      >
        Claim Airdrop
      </button>
    {:else if luckyCode == 'AAAAAA'}
      <!-- banned user -->
      <p class="flex flex-row gap-1">
        <span>Sorry, you cannot claim the airdrop.</span>
      </p>
      <button
        disabled={true}
        class="variant-filled-primary btn m-auto mt-3 flex w-[320px] max-w-full flex-row items-center gap-2 text-white transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
      >
        Claim Airdrop
      </button>
    {:else}
      <p class="flex flex-row gap-1">
        <span>You have claimed</span>
        <span>
          {formatNumber(Number(claimedAmount / PANDAToken.one))}
        </span>
        <span>tokens</span>
      </p>
      <p>
        <span>Lucky Code:</span>
        <span class="text-panda">{luckyCode}</span>
        <TextClipboardButton textValue={luckyCode} />
      </p>
      <p class="">
        <span>Link:</span>
        <span>
          {'https://panda.fans/?ref=' + luckyCode}
        </span>
        <TextClipboardButton
          textValue={'https://panda.fans/?ref=' + luckyCode}
        />
      </p>
      <button
        disabled={submitting ||
          claimableAmount === 0n ||
          totalBalance < claimableAmount + PANDAToken.fee}
        on:click={harvestHandler}
        class="variant-filled-primary btn m-auto mt-3 flex w-[320px] max-w-full flex-row items-center gap-2 text-white transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else if harvested > 0n}
          <span>
            {'Harvested ' +
              formatNumber(Number(harvested / PANDAToken.one)) +
              ' PANDA tokens'}
          </span>
          <span>
            <IconCheckbox />
          </span>
        {:else}
          <span>
            {'Harvest ' +
              formatNumber(Number(claimableAmount / PANDAToken.one)) +
              ' PANDA tokens'}
          </span>
        {/if}
      </button>
    {/if}
    <button
      on:click={claimPrizeHandler}
      class="variant-ringed btn m-auto mt-3 flex w-[320px] max-w-full flex-row items-center gap-2 transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
    >
      Claim Prize
    </button>
  </footer>
</div>
