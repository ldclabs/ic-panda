<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import {
    luckyPoolAPIAsync,
    type AirdropState,
    type LuckyPoolAPI,
    type State
  } from '$lib/canisters/luckypool'
  import IconAlarmWarning from '$lib/components/icons/IconAlarmWarning.svelte'
  import IconGoldPanda from '$lib/components/icons/IconGoldPanda.svelte'
  import IconInfo from '$lib/components/icons/IconInfo.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { getModalStore, popup } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import AirdropModal from './AirdropModal.svelte'
  import LuckyTransferModal from './LuckyTransferModal.svelte'

  let luckyPoolState: Readable<State | null>
  let airdropState: Readable<AirdropState | null>
  let luckyPoolAPI: LuckyPoolAPI
  let totalBalance = 0n
  let claimableAmount = 0n
  let claimedAmount = 0n
  let luckyCode = ''

  const modalStore = getModalStore()

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

  function transferHandler() {
    if (principal.isAnonymous()) {
      signIn({})
    } else {
      modalStore.trigger({
        type: 'component',
        component: { ref: LuckyTransferModal }
      })
    }
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()
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
      if (luckyPoolAPI.principal.toString() != principal.toString()) {
        luckyPoolAPIAsync().then((_luckyPoolAPI) => {
          luckyPoolAPI = _luckyPoolAPI
        })
      }

      if (luckyCode && $page.url?.searchParams.get('ref')) {
        const query = $page.url.searchParams
        query.delete('ref')
        goto(`?${query.toString()}`)
      }
    }
  }
</script>

<div
  class="flex flex-col justify-center rounded-2xl bg-white bg-[url('/_assets/images/lucky-pool-bg.webp')] bg-[length:100%_auto] bg-no-repeat p-4"
>
  <section class="mb-10 mt-5 flex flex-col justify-center">
    <h5 class="h5 text-center font-extrabold">
      <span>Free PANDA Airdrop</span>
    </h5>
    <div class="m-auto mt-5 flex flex-row gap-4">
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
      <p class="text-sm text-gray/50">Please read the rules before claiming:</p>
      <ol class="list *:mt-3">
        <li>
          <span class="badge-icon bg-pink-500 p-2 text-white">1</span>
          <span class="flex-auto">
            New users can get <b class="text-pink-500">
              {formatNumber(Number(claimableAmount / PANDAToken.one))} PANDA
            </b>
            or
            <b class="text-pink-500">
              {formatNumber(
                Number(
                  (claimableAmount + claimableAmount / 2n) / PANDAToken.one
                )
              )} PANDA
            </b>
            with <b class="text-pink-500">LUCKY CODE</b>.
          </span>
        </li>
        <li>
          <span class="badge-icon bg-pink-500 p-2 text-white">2</span>
          <span class="flex-auto">
            Your <b>LUCKY CODE</b> will be generated after claiming the airdrop.
          </span>
        </li>
        <li>
          <span class="badge-icon bg-pink-500 p-2 text-white">3</span>
          <span class="flex-auto">
            For each successful <b class="text-pink-500"
              >referral with LUCKY CODE</b
            >, you gain an additional
            <b class="text-pink-500"
              >{formatNumber(
                Number(claimableAmount / (2n * PANDAToken.one))
              )}</b
            >.
          </span>
        </li>
      </ol>
      <div class="mt-10 flex flex-col items-center">
        <p
          class="flex flex-row content-center items-center gap-2 text-sm font-medium text-pink-500"
        >
          <span class="*:size-5"><IconAlarmWarning /></span>Each user
          <span>can only claim ONCE.</span>
        </p>
        <button
          disabled={claimableAmount === 0n ||
            totalBalance < claimableAmount + PANDAToken.fee}
          on:click={claimNowHandler}
          class="btn m-auto mt-3 w-[320px] max-w-full bg-pink-500 font-medium text-white transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
        >
          Understand and Claim Now
        </button>
      </div>
    {:else if luckyCode == 'AAAAAA'}
      <!-- banned user -->
      <p class="">
        <span>Sorry, you can not claim the airdrop.</span>
      </p>
      <button
        disabled={true}
        class="variant-filled-primary btn m-auto mt-3 flex w-[320px] max-w-full flex-row items-center gap-2 text-white transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
      >
        Got It
      </button>
    {:else}
      <p class="">
        <span>
          The more lucky balance you have, the larger your claim in a
          <b>Lucky PANDA Prize</b>.
        </span>
      </p>
      <p class="mt-3">
        <span><b>Lucky Balance:</b></span>
        <span>
          <b class="text-panda"
            >{formatNumber(Number(claimableAmount) / Number(PANDAToken.one))}</b
          > PANDA tokens
        </span>
        <span>
          ({formatNumber(Number(claimedAmount) / Number(PANDAToken.one))} tokens
          transferred out)
        </span>
      </p>
      <p class="mt-3">
        <span>Lucky Code:</span>
        <span class="text-panda"><b>{luckyCode}</b></span>
        <TextClipboardButton textValue={luckyCode} />
      </p>
      <p class="mt-3">
        <span>Link:</span>
        <span>
          {`${APP_ORIGIN}/?ref=${luckyCode}`}
        </span>
        <TextClipboardButton textValue={`${APP_ORIGIN}/?ref=${luckyCode}`} />
      </p>
      <button
        disabled={claimableAmount === 0n}
        on:click={transferHandler}
        class="variant-filled-primary btn m-auto mt-10 flex w-[320px] max-w-full flex-row items-center gap-2 text-white transition duration-700 ease-in-out md:btn-lg hover:scale-110 hover:shadow"
      >
        {#if claimableAmount > 0n}
          <span>Transfer tokens to wallet</span>
        {:else}
          <span>No token to transfer</span>
        {/if}
      </button>
    {/if}
  </footer>
</div>
