<script lang="ts">
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type AirdropState
  } from '$lib/canisters/luckypool'
  import AccountDetailModal from '$lib/components/core/AccountDetailModal.svelte'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconGoldPanda from '$lib/components/icons/IconGoldPanda.svelte'
  import IconWallet from '$lib/components/icons/IconWallet.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { errMessage } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import {
    focusTrap,
    getModalStore,
    getToastStore
  } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let submitting = false
  let validating = false
  let luckyPoolAPI: LuckyPoolAPI
  let airdropState: Readable<AirdropState | null>
  let inputAmount = 0
  let claimableAmount = 0n
  let result: AirdropState

  const modalStore = getModalStore()
  const toastStore = getToastStore()

  function transferAmount(n: number) {
    try {
      return BigInt(Number(PANDAToken.one) * n)
    } catch (err) {
      return 0n
    }
  }

  let transferred = 0n
  async function transferHandler() {
    const amount = transferAmount(inputAmount)
    if (amount > 0n) {
      submitting = true
      try {
        result = await luckyPoolAPI.harvest({
          amount,
          recaptcha: []
        })
        submitting = false
        transferred = amount

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

  function checkInput() {
    const amount = transferAmount(inputAmount)
    if (amount < PANDAToken.one || amount > claimableAmount) {
      return 'invalid amount'
    }
    return ''
  }

  function onFormChange(e: Event) {
    const form = e.currentTarget as HTMLFormElement
    const input = form['pandaTokens'] as HTMLInputElement
    input?.setCustomValidity(checkInput())

    validating = form.checkValidity()
  }

  function onCheckWallet() {
    parent && parent['onClose']()
    modalStore.trigger({
      type: 'component',
      component: { ref: AccountDetailModal }
    })
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()
  })

  $: {
    if (luckyPoolAPI) {
      airdropState = luckyPoolAPI.airdropStateStore
      claimableAmount = $airdropState?.claimable || 0n
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
          You have successfully transferred <b
            >{formatNumber(Number(transferred) / Number(PANDAToken.one))}</b
          > PANDA tokens to your wallet.
        </span>
      </p>
    </div>
    <div
      class="!mt-12 flex flex-row justify-between rounded-lg bg-gray/5 px-4 py-3"
    >
      <div class="flex flex-row">
        <span><IconWallet /></span>
        <span class="ml-2">Wallet</span>
      </div>
      <div class="flex flex-row">
        <span>
          {'+ ' + formatNumber(inputAmount)}
        </span>
        <span class="ml-2 *:mt-[2px] *:h-5 *:w-5"><IconGoldPanda /></span>
      </div>
    </div>
    <div class="!mt-12">
      <button
        class="variant-filled btn btn-lg m-auto block"
        on:click={onCheckWallet}
      >
        Check Wallet
      </button>
    </div>
  {:else}
    <h6 class="h6">Transfer from Lucky Balance</h6>
    <p class="text-gray/50">
      <span>
        The more lucky balance you have, the larger your claim in a luck-based
        <b>PANDA Prize</b>.
      </span>
    </p>
    <p class="text-gray/50">
      <span>
        It is recommended that you keep between 100 ~ 20,000 PANDA tokens in
        lucky balance.
      </span>
    </p>
    <hr class="!border-t-1 !border-gray/10" />
    <div class="text-sm">
      <p>
        <span>Lucky Balance:</span>
        <span
          >{formatNumber(
            Number(claimableAmount) / Number(PANDAToken.one)
          )}</span
        >
        <span>{PANDAToken.symbol}</span>
      </p>
    </div>
    <form
      class="flex flex-col gap-4"
      on:input={onFormChange}
      use:focusTrap={true}
    >
      <div
        class="input-group input-group-divider grid-cols-[auto_1fr_auto] bg-gray/5"
      >
        <div class="input-group-shim bg-gray/5">PANDA</div>
        <input
          class="input rounded-none invalid:input-warning hover:bg-white/90"
          type="number"
          name="pandaTokens"
          step="any"
          bind:value={inputAmount}
          disabled={submitting}
          placeholder="Enter an amount (>= 1)"
          required
        />
      </div>
    </form>
    <footer class="">
      <button
        class="variant-filled-primary btn w-full text-white"
        disabled={submitting || !validating}
        on:click={transferHandler}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Transfer</span>
        {/if}
      </button>
    </footer>
  {/if}
</ModalCard>
