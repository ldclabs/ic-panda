<script lang="ts">
  import { type ChannelInfo } from '$lib/canisters/messagechannel'
  import { tokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { MESSAGE_CANISTER_ID } from '$lib/constants'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { getShortNumber } from '$lib/utils/helper'
  import { PANDAToken, TokenDisplay, formatNumber } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { focusTrap, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  const toastStore = getToastStore()

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    myState: MyMessageState
    channel: ChannelInfo
  }

  let { parent, myState, channel }: Props = $props()

  const messageCanisterPrincipal = Principal.fromText(MESSAGE_CANISTER_ID)

  let validating = $state(false)
  let submitting = $state(false)
  let availablePandaBalance = $state(0n)

  let amountInput = $state(0)
  let topupErr = $state('')

  async function onTopup() {
    submitting = true
    toastRun(async (signal: AbortSignal) => {
      if (tokenDisplay.total > availablePandaBalance) {
        topupErr = 'Insufficient balance'
        submitting = false
        validating = true
        return
      }

      await tokenLedgerAPI.ensureAllowance(
        messageCanisterPrincipal,
        tokenDisplay.total
      )
      const channelInfo = await myState.api.topup_channel({
        id: channel.id,
        canister: channel.canister,
        payer: myState.principal,
        amount: tokenDisplay.total
      })

      await myState.agent.setChannel(channelInfo)
      parent && parent['onClose']()
    }, toastStore).finally(() => {
      submitting = false
      validating = false
    })
  }

  function validateAmount(e: Event) {
    const input = e.target as HTMLInputElement
    if (tokenDisplay.total > availablePandaBalance) {
      topupErr = 'Insufficient balance'
      input.setCustomValidity(topupErr)
      return
    }

    if (amountInput < 1) {
      topupErr = 'Amount must be greater than 1'
      input.setCustomValidity(topupErr)
      return
    }

    topupErr = ''
    if (input.value.startsWith('0')) {
      input.value = amountInput.toString()
    }

    input.setCustomValidity('')
  }

  function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    validating = form.checkValidity()
  }

  onMount(() => {
    const { abort } = toastRun(async (signal: AbortSignal) => {
      const pandaBalance = tokenLedgerAPI.balance()
      availablePandaBalance = await pandaBalance
    }, toastStore)

    return abort
  })

  let tokenDisplay = $derived(
    TokenDisplay.fromNumber(PANDAToken, amountInput || 0, false)
  )
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Topup gas</div>

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    onchange={onFormChange}
    use:focusTrap={true}
  >
    <div class="relative">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 pr-16 invalid:input-warning"
        type="number"
        name="amountInput"
        min="0"
        step="any"
        bind:value={amountInput}
        oninput={validateAmount}
        disabled={submitting}
        placeholder="Enter an amount >=1"
        data-focusindex="1"
        required
      />
      <div class="absolute right-2 top-2 text-neutral-500 outline-0"
        >{PANDAToken.symbol}</div
      >
      <p class="h-5 pl-3 text-sm {topupErr ? 'text-error-500' : 'text-panda'}"
        >{topupErr ? topupErr : getShortNumber(tokenDisplay.total) + ' Gas'}</p
      >
    </div>
    <div class="!mt-4 mb-2 text-sm">
      <div class="flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-2">
          <span class="*:size-6"><IconPanda /></span>
          <b>Your Wallet Balance:</b>
        </div>
        <div class="flex flex-row gap-1 text-neutral-500">
          <span
            >{formatNumber(
              Number(availablePandaBalance) / Number(PANDAToken.one)
            )}</span
          >
          <span>{PANDAToken.symbol}</span>
        </div>
      </div>
    </div>
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting ||
        !validating ||
        tokenDisplay.total > availablePandaBalance}
      onclick={onTopup}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>Processing...</span>
      {:else}
        <span>Topup</span>
      {/if}
    </button>
  </footer>
</ModalCard>
