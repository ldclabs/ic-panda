<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type AirdropState,
    type ClaimPrizeOutput,
    type PrizeOutput
  } from '$lib/canisters/luckypool'
  import { tokenLedgerAPIAsync } from '$lib/canisters/tokenledger'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconCloseCircle from '$lib/components/icons/IconCloseCircle.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconOpenChat from '$lib/components/icons/IconOpenChat.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { challengeToken, signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { decodePrize } from '$lib/types/prize'
  import { errMessage } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { encodeCBOR } from '@ldclabs/cose-ts/utils'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'
  import PrizeShow from './PrizeShow.svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let submitting = false
  let validating = false
  let canClaim = true
  let meetRequirements = 0
  let cryptogram = $page.url.searchParams.get('prize') || ''
  let luckyPoolAPI: LuckyPoolAPI
  let airdropState: Readable<AirdropState | null>
  let prizeInfo: PrizeOutput
  let result: ClaimPrizeOutput

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  async function prizeCodeCopyPaste(e: Event) {
    e.preventDefault()

    if (cryptogram == '') {
      cryptogram = await navigator.clipboard.readText()
    } else {
      cryptogram = ''
    }
    checkValidity()
  }

  function checkValidity() {
    validating = decodePrize(cryptogram) != null
  }

  async function onFormSubmit(e: Event) {
    e.preventDefault()

    claimPrize()
  }

  async function claimPrize() {
    if (canClaim) {
      submitting = true
      try {
        const prize = decodePrize(cryptogram)
        const token = await challengeToken(
          {
            principal: luckyPoolAPI.principal.toUint8Array(),
            message: encodeCBOR(prize)
          },
          'claim_prize'
        )

        result = await luckyPoolAPI.claimPrize({
          code: cryptogram,
          challenge: token
        })

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

  function onFormChange(e: Event) {
    checkValidity()
  }

  function closePrizeShow() {
    modalStore.clear()

    if ($page.url.searchParams.get('prize')) {
      const query = $page.url.searchParams
      query.delete('prize')
      goto(`?${query.toString()}`)
    }
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()
    checkValidity()

    if (validating) {
      prizeInfo = await luckyPoolAPI.prizeInfo(cryptogram)
      canClaim =
        prizeInfo.filled < prizeInfo.quantity && prizeInfo.ended_at == 0n
    }

    // Remove the prize query parameter from the URL
    if (!principal.isAnonymous() && $page.url.searchParams.get('prize')) {
      const query = $page.url.searchParams
      query.delete('prize')
      goto(`?${query.toString()}`)
    }
  })

  $: principal = $authStore.identity.getPrincipal()
  $: {
    if (luckyPoolAPI) {
      airdropState = luckyPoolAPI.airdropStateStore
      if ($airdropState?.lucky_code[0]) {
        meetRequirements = meetRequirements | 1
      }
      if ($airdropState?.claimable && $airdropState?.claimable >= 10n) {
        meetRequirements = meetRequirements | 2
      } else if (!principal.isAnonymous()) {
        tokenLedgerAPIAsync().then((api) => {
          api.balance().then((balance) => {
            if (($airdropState?.claimable || 0n) + balance >= 10n) {
              meetRequirements = meetRequirements | 2
            }
          })
        })
      }

      if (luckyPoolAPI?.principal.toString() != principal.toString()) {
        luckyPoolAPIAsync().then((_luckyPoolAPI) => {
          luckyPoolAPI = _luckyPoolAPI
        })
      }

      if (
        validating &&
        (prizeInfo == null || prizeInfo.code[0] != cryptogram)
      ) {
        luckyPoolAPI.prizeInfo(cryptogram).then((info) => {
          prizeInfo = info
          canClaim =
            prizeInfo.filled < prizeInfo.quantity && prizeInfo.ended_at == 0n
        })
      }
    }
  }
</script>

<ModalCard {parent}>
  {#if result}
    <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
      <IconCheckbox />
    </div>
    <div class="text-center">
      <p class="mt-4">
        <span>
          You have successfully claimed <b
            >{formatNumber(Number(result.claimed) / Number(PANDAToken.one))}</b
          >
          PANDA tokens.
        </span>
      </p>
      <p class="">
        <span>
          The average claimable amount is
          <b>{formatNumber(Number(result.average) / Number(PANDAToken.one))}</b
          >.
        </span>
      </p>
      {#if result.claimed < result.average}
        <p class="">
          <b>
            The More Lucky Balance you have, the larger your claim in a Lucky
            PANDA Prize.
          </b>
        </p>
      {/if}
      <p class="mt-4 text-left">
        Follow the <a
          title="Follow on Twitter"
          class="text-panda underline"
          href="https://twitter.com/ICPandaDAO"
          target="_blank">ICPanda Twitter</a
        >, or join the
        <a
          title="Join the Community"
          class="text-panda underline"
          href="https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai"
          target="_blank">ICPanda Community</a
        >, to get the latest prize messages in time.
      </p>
    </div>
  {:else if prizeInfo && (principal.isAnonymous() || meetRequirements == 3)}
    <PrizeShow {prizeInfo} {claimPrize} close={closePrizeShow} />
  {:else}
    <h3 class="h3 !mt-0 text-center">🐼 🎁</h3>
    <div class="!mt-0 text-center text-xl font-bold">Get a Prize</div>
    <div class="m-auto mt-5 flex flex-col content-center">
      <p class="text-sm text-gray/50">Meet requirements:</p>
      <p class="mt-3 flex flex-row items-center gap-2">
        {#if meetRequirements != 1 && meetRequirements != 3}
          <span class="*:size-5"><IconCloseCircle /></span>
        {:else}
          <span class="text-orange-500 *:size-5"><IconCheckbox /></span>
        {/if}
        <span>Have a <b>lucky code</b> through airdrop</span>
      </p>
      <p class="mt-3 flex flex-row items-center gap-2">
        {#if meetRequirements != 2 && meetRequirements != 3}
          <span class="*:size-5"><IconCloseCircle /></span>
        {:else}
          <span class="text-orange-500 *:size-5"><IconCheckbox /></span>
        {/if}
        <span>Have at lest <b>10 PANDA</b> in your wallet</span>
      </p>
    </div>
    <form
      class="m-auto !mt-6 flex flex-col content-center"
      on:input={onFormChange}
    >
      <label class="label">
        <span>Fill in prize code here:</span>
        <div class="relative">
          <input
            class="input truncate rounded-xl border-gray/10 bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
            type="text"
            name="cryptogram"
            minlength="20"
            maxlength="50"
            bind:value={cryptogram}
            placeholder="Enter code"
            disabled={submitting}
            required
          />
          <button
            class="btn absolute right-0 top-0 outline-0"
            disabled={submitting}
            on:click={prizeCodeCopyPaste}
          >
            {#if cryptogram == ''}
              <span>Paste</span>
            {:else}
              <span class="*:scale-90"><IconDeleteBin /></span>
            {/if}
          </button>
        </div>
      </label>
    </form>
    <footer class="m-auto !my-6">
      {#if principal.isAnonymous()}
        <button
          class="btn flex w-full flex-row items-center gap-2 bg-gradient-to-r from-amber-300 to-red-500 text-white"
          on:click={() => signIn()}
        >
          <span>Login</span>
        </button>
      {:else}
        <button
          class="btn flex w-full flex-row items-center gap-2 text-white {meetRequirements <
          3
            ? 'bg-gray/20'
            : 'bg-gradient-to-r from-amber-300 to-red-500'}"
          disabled={submitting || !validating || meetRequirements != 3}
          on:click={onFormSubmit}
        >
          {#if submitting}
            <span class=""><IconCircleSpin /></span>
            <span>Processing...</span>
          {:else}
            <span>Claim Now</span>
          {/if}
        </button>
      {/if}
    </footer>
    <hr class="!border-t-1 mx-[-24px] !mt-0 !border-dashed !border-gray/20" />
    <div class="m-auto !mt-5">
      <p class="text-sm text-gray/50">To get the latest updates by following:</p
      >
      <div class="mt-3 flex flex-row justify-between">
        <a
          type="button"
          title="Follow on Twitter"
          class="btn btn-sm rounded-xl border-2 border-gray/10"
          href="https://twitter.com/ICPandaDAO"
          target="_blank"
        >
          <span><IconX /></span>
          <span class="text-left">ICPanda DAO</span>
        </a>
        <a
          type="button"
          title="Join the Community"
          class="btn btn-sm rounded-xl border-2 border-gray/10"
          href="https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai"
          target="_blank"
        >
          <span><IconOpenChat /></span>
          <span class="text-left">ICPanda Community</span>
        </a>
      </div>
    </div>
  {/if}
</ModalCard>
