<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type AirdropState
  } from '$lib/canisters/luckypool'
  import {
    TokenLedgerAPI,
    tokenLedgerAPIAsync
  } from '$lib/canisters/tokenledger'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconCloseCircle from '$lib/components/icons/IconCloseCircle.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconOpenChat from '$lib/components/icons/IconOpenChat.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { decodePrize } from '$lib/types/prize'
  import { errMessage } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  const modalStore = getModalStore()

  let submitting = false
  let validating = false
  let meetRequirements = 0
  let cryptogram = $page.url.searchParams.get('prize') || ''
  let luckyPoolAPI: LuckyPoolAPI
  let tokenLegerAPI: TokenLedgerAPI
  let airdropState: Readable<AirdropState | null>
  let result: AirdropState

  const toastStore = getToastStore()

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

    submitting = true
    try {
      result = await luckyPoolAPI.prize(cryptogram)
      // Remove the prize query parameter from the URL
      if ($page.url.searchParams.get('prize')) {
        const query = $page.url.searchParams
        query.delete('prize')
        goto(`?${query.toString()}`)
      }
    } catch (err: any) {
      submitting = false

      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
    await luckyPoolAPI.refreshAllState()
  }

  function onFormChange(e: Event) {
    checkValidity()
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()
    tokenLegerAPI = await tokenLedgerAPIAsync()
    checkValidity()
  })

  $: {
    if (luckyPoolAPI) {
      airdropState = luckyPoolAPI.airdropStateStore
      if ($airdropState?.lucky_code[0]) {
        meetRequirements = meetRequirements | 1
      }
      if ($airdropState?.claimable && $airdropState?.claimable >= 10n) {
        meetRequirements = meetRequirements | 2
      } else if (tokenLegerAPI) {
        tokenLegerAPI.balance().then((balance) => {
          if (($airdropState?.claimable || 0n) + balance >= 10n) {
            meetRequirements = meetRequirements | 2
          }
        })
      }
    }
  }
</script>

<ModalCard {parent}>
  {#if result}
    <div class="text-center">
      <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
        <IconCheckbox />
      </div>
      <p class="mt-4">
        <span>
          You have successfully claimed <b
            >{formatNumber(
              Number(
                result.claimable - ($modalStore[0]?.meta.claimableAmount || 0n)
              ) / Number(PANDAToken.one)
            )}</b
          > PANDA tokens.
        </span>
      </p>
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
  {:else}
    <h3 class="h3 !mt-1 text-center">🐼 🎁</h3>
    <div class="text-center text-xl font-bold">Get a Prize</div>
    <div class="m-auto mt-5 flex w-80 flex-col content-center">
      <p class="text-sm text-gray/50">Meet requirements:</p>
      <p class="mt-3 flex flex-row items-center gap-2">
        {#if meetRequirements != 1 && meetRequirements != 3}
          <span class="*:size-5"><IconCloseCircle /></span>
        {:else}
          <span class="text-orange *:size-5"><IconCheckbox /></span>
        {/if}
        <span>Have a <b>lucky code</b> through airdrop</span>
      </p>
      <p class="mt-3 flex flex-row items-center gap-2">
        {#if meetRequirements != 2 && meetRequirements != 3}
          <span class="*:size-5"><IconCloseCircle /></span>
        {:else}
          <span class="text-orange *:size-5"><IconCheckbox /></span>
        {/if}
        <span>Have at lest <b>10 PANDA</b> in your wallet</span>
      </p>
    </div>
    <form
      class="m-auto !mt-8 flex w-80 flex-col content-center"
      on:input={onFormChange}
    >
      <label class="label">
        <span>Fill in prize code here:</span>
        <div class="relative">
          <input
            class="input truncate rounded-xl bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
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
    <footer class="m-auto !my-8 w-80">
      <button
        class="btn flex w-full flex-row items-center gap-2 text-white {meetRequirements <
        3
          ? 'bg-gray/20'
          : 'variant-gradient-warning-error bg-gradient-to-br'}"
        disabled={submitting || !validating}
        on:click={onFormSubmit}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Claim</span>
        {/if}
      </button>
    </footer>
    <hr class="!border-t-1 mx-[-24px] !mt-0 !border-dashed !border-gray/20" />
    <div class="m-auto !mt-7 w-80">
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
          <span class="text-left">ICPanda X</span>
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
