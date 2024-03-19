<script lang="ts">
  import IconGoldPanda from '$lib/components/icons/IconGoldPanda.svelte'
  import IconInfo from '$lib/components/icons/IconInfo.svelte'
  import { formatNumber } from '$lib/utils/token'
  import { popup, getModalStore } from '@skeletonlabs/skeleton'
  import { PANDAToken } from '$lib/utils/token'
  import AirdropModal from './AirdropModal.svelte'
  import { authStore } from '$lib/stores/auth'
  import { signIn } from '$lib/services/auth'
  import { type Readable } from 'svelte/store'
  import {
    type LuckyPoolAPI,
    type AirdropState,
    type State
  } from '$lib/canisters/luckypool'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'

  export let luckyPoolState: Readable<State | null>
  export let airdropState: Readable<AirdropState | null>
  export let luckyPoolAPI: LuckyPoolAPI

  const modalStore = getModalStore()

  function claimNowHandler() {
    if ($authStore.identity.getPrincipal().isAnonymous()) {
      signIn({})
    } else {
      modalStore.trigger({
        type: 'component',
        component: { ref: AirdropModal }
      })
    }
  }

  let submitting = false
  async function harvestHandler() {
    if (claimableAmount > 0n) {
      submitting = true
      await luckyPoolAPI.harvest({ amount: claimableAmount })
      await luckyPoolAPI.refreshAllState()
      submitting = false
    }
  }

  $: totalBalance = $luckyPoolState?.airdrop_balance || 0n
  $: claimableAmount = $airdropState?.claimable || 0n
  $: claimedAmount = $airdropState?.claimed || 0n
  $: luckyCode = $airdropState?.lucky_code[0] || ''
</script>

<div
  class="flex w-[400px] max-w-full flex-col justify-center rounded-2xl bg-white p-4"
>
  <section class="mb-12 mt-6 flex flex-col justify-center">
    <h5 class="h5 text-center font-extrabold">
      <span>Free PANDA Airdrop</span>
    </h5>
    <div class="m-auto mt-12 flex flex-row gap-4">
      <div>
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
    {#if claimedAmount == 0n}
      <p class="flex flex-row justify-center gap-1 text-gold">
        <span>You can get</span>
        <span class="mt-[2px] *:size-5"><IconGoldPanda /></span>
        <span>
          {formatNumber(Number(claimableAmount / PANDAToken.one))}
        </span>
        <span>PANDA for free</span>
      </p>
      <button
        disabled={claimableAmount === 0n ||
          totalBalance < claimableAmount + PANDAToken.fee}
        on:click={claimNowHandler}
        class="variant-filled-primary btn btn-lg m-auto mt-3 block w-[300px] max-w-full text-white"
      >
        Claim Now
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
        class="variant-filled-primary btn btn-lg m-auto mt-3 flex w-[300px] max-w-full flex-row items-center gap-2 text-white"
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>
            {'Harvest ' +
              formatNumber(Number(claimableAmount / PANDAToken.one)) +
              ' PANDA tokens'}
          </span>
        {/if}
      </button>
    {/if}
  </footer>
</div>
