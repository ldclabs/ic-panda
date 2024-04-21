<script lang="ts">
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type AirdropState
  } from '$lib/canisters/luckypool'
  import AccountDetailModal from '$lib/components/core/AccountDetailModal.svelte'
  import IconArrowDownLine from '$lib/components/icons/IconArrowDownLine.svelte'
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
  let showTips = false
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
      <div class="flex flex-row items-center">
        <span><IconWallet /></span>
        <span class="ml-2">Wallet</span>
      </div>
      <div class="flex flex-row items-center">
        <span>
          {'+ ' + formatNumber(inputAmount)}
        </span>
        <span class="ml-2 *:size-6"><IconGoldPanda /></span>
      </div>
    </div>
    <div class="!mt-12">
      <button class="variant-filled btn m-auto block" on:click={onCheckWallet}>
        Check Wallet
      </button>
    </div>
  {:else}
    <h3 class="h3 !mt-0 text-center">💳</h3>
    <div class="!mt-0 text-center text-xl font-bold">Transfer to Wallet</div>
    <div class="space-y-2 rounded-xl bg-gray/5 p-4 text-gray/50">
      <p class="">
        <span>
          The <b>More Lucky Balance</b> you have, the larger your claim in a
          <b>Lucky PANDA Prize</b>.
        </span>
      </p>
      <div class="text-sm {showTips ? '' : 'hidden'}">
        <p>10: claim up to 0.1 * avg;</p>
        <p>100: claim up to 0.29 * avg;</p>
        <p>200: claim up to 0.44 * avg;</p>
        <p>500: claim up to 0.78 * avg;</p>
        <p>900: claim up to 1.09 * avg;</p>
        <p>1k: claim up to 1.15 * avg;</p>
        <p>1.5k: claim up to 1.42 * avg;</p>
        <p>2k: claim up to 1.64 * avg;</p>
        <p>5k: claim up to 2.43 * avg;</p>
        <p>10k: claim up to 3.15 * avg;</p>
        <p>50k: claim up to 5.21 * avg;</p>
        <p>100k: claim up to 6.25 * avg;</p>
        <p>500k: claim up to 9.06 * avg;</p>
        <p>1m: claim up to 10.44 * avg;</p>
        <p>All: 20% chance of claiming avg.</p>
      </div>
      <button
        class="btn ml-[-1px] !flex p-0 text-gray/50 outline-0"
        on:click={() => {
          showTips = !showTips
        }}
      >
        <span class="">Lucky balance tips</span>
        <span
          class="duration-400 transition ease-in-out {showTips
            ? 'rotate-180'
            : 'rotate-0'}"><IconArrowDownLine /></span
        >
      </button>
    </div>
    <hr class="!border-t-1 mx-[-24px] !mt-6 !border-dashed !border-gray/20" />
    <div class="!mt-5 text-sm">
      <div class="mt-1 flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-2">
          <span class="*:size-6"><IconGoldPanda /></span>
          <b>Your Lucky Balance:</b>
        </div>
        <div class="flex flex-row gap-1 text-gray/50">
          <span
            >{formatNumber(
              Number(claimableAmount) / Number(PANDAToken.one)
            )}</span
          >
          <span>{PANDAToken.symbol}</span>
        </div>
      </div>
    </div>
    <form
      class="flex flex-col gap-2"
      on:input={onFormChange}
      use:focusTrap={true}
    >
      <div class="relative">
        <input
          class="input truncate rounded-xl border-gray/10 bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
          type="number"
          name="pandaTokens"
          step="any"
          bind:value={inputAmount}
          disabled={submitting}
          placeholder="Enter an amount at least 1 token"
          required
        />
        <div class="absolute right-2 top-2 outline-0">PANDA</div>
      </div>
    </form>
    <footer class="!mt-6">
      <button
        class="variant-filled-primary btn w-full text-white"
        disabled={submitting || !validating}
        on:click={transferHandler}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Transfer Now</span>
        {/if}
      </button>
    </footer>
  {/if}
</ModalCard>
