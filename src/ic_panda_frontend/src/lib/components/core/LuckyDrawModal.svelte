<script lang="ts">
  import {
    icpLedgerAPIAsync,
    type ICPLedgerAPI
  } from '$lib/canisters/icpledger'
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type LuckyDrawOutput,
    type State
  } from '$lib/canisters/luckypool'
  import {
    tokenLedgerAPIAsync,
    type TokenLedgerAPI
  } from '$lib/canisters/tokenledger'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconGoldPanda from '$lib/components/icons/IconGoldPanda.svelte'
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { LUCKYPOOL_CANISTER_ID } from '$lib/constants'
  import { decodeAirdropCode, type Prize } from '$lib/types/prize'
  import { errMessage } from '$lib/types/result'
  import {
    formatNumber,
    ICPToken,
    PANDAToken,
    TokenDisplay
  } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { focusTrap, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let stepN: 0 | 1 | 2 = 0
  let submitting = false
  let validating = false
  let luckyPoolAPI: LuckyPoolAPI
  let icpLedgerAPI: ICPLedgerAPI
  let tokenLedgerAPI: TokenLedgerAPI
  let inputAmount = 0.1
  let icpBalance = 0n
  let luckyPoolBalance = 0n
  let result: LuckyDrawOutput
  let lottiePlayerRef: HTMLDivElement
  let cryptogramInfo: Prize | null = null
  let luckyPoolState: Readable<State | null>

  const luckyPoolPrincipal = Principal.fromText(LUCKYPOOL_CANISTER_ID)

  const toastStore = getToastStore()

  function icpCostAmount(n: number) {
    try {
      return BigInt(Number(ICPToken.one) * n)
    } catch (err) {
      return 0n
    }
  }

  async function onFormSubmit() {
    submitting = true
    try {
      const amount = icpCostAmount(inputAmount)
      stepN = 1
      await icpLedgerAPI.ensureAllowance(luckyPoolPrincipal, amount)

      stepN = 2
      result = await luckyPoolAPI.luckydraw({
        icp: 0,
        amount: [amount]
      })
      if (result.airdrop_cryptogram.length > 0) {
        cryptogramInfo = decodeAirdropCode(result.airdrop_cryptogram[0] || '')
      }
      setTimeout(() => {
        lottiePlayerRef?.remove()
      }, 1800)
    } catch (err: any) {
      submitting = false
      stepN = 0
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }

    luckyPoolAPI.refreshAllState()
  }

  function checkInput() {
    if (
      inputAmount < 0.1 ||
      inputAmount > 10 ||
      !Number.isSafeInteger(inputAmount * 10)
    ) {
      return 'Enter an integer between 0.1 and 10'
    } else {
      const cost = icpCostAmount(inputAmount)
      if (cost == 0n || cost > icpBalance) {
        return 'Insufficient ICP balance in your wallet'
      }
    }
    return ''
  }

  function onFormChange(e: Event) {
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    const input = form['icpTokens'] as HTMLInputElement
    input?.setCustomValidity(checkInput())

    validating = form.checkValidity()
  }

  function setAmount(e: Event, n: number) {
    e.preventDefault()

    if (!submitting) {
      inputAmount = n
      validating = checkInput() == ''
    }
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()
    icpLedgerAPI = await icpLedgerAPIAsync()
    tokenLedgerAPI = await tokenLedgerAPIAsync()

    luckyPoolState = luckyPoolAPI.stateStore

    icpBalance = await icpLedgerAPI.balance()
    luckyPoolBalance = await tokenLedgerAPI.getBalanceOf(luckyPoolPrincipal)

    validating = checkInput() == ''
  })

  $: luckyPoolBalanceDisplay = new TokenDisplay(PANDAToken, luckyPoolBalance)
  $: icpBalanceDisplay = new TokenDisplay(ICPToken, icpBalance)
</script>

<ModalCard {parent}>
  {#if result}
    <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
      <IconCheckbox />
    </div>
    <div class="text-center">
      <p class="mt-4">
        <span>Congratulations on winning</span>
        <span class="font-bold text-panda">
          {formatNumber(Number(result.amount) / Number(PANDAToken.one))}
        </span>
        <span>tokens in the lucky draw, check your <b>Lucky Balance</b>.</span>
      </p>
      {#if result.prize_cryptogram.length > 0}
        <p class="mt-4">
          <span>Giving you a prize cryptogram:</span>
        </p>
        <div class="my-2 flex flex-row items-center gap-1">
          <p class="truncate text-panda"
            >{'PRIZE:' + result.prize_cryptogram[0]}</p
          >
          <TextClipboardButton
            textValue={'PRIZE:' + result.prize_cryptogram[0]}
          />
        </div>
        <p>
          <span>
            It contains <b>500</b> PANDA tokens, available for <b>10</b> people
            to claim, valid for <b>7</b> days.
          </span>
        </p>
      {:else if result.airdrop_cryptogram.length > 0 && cryptogramInfo}
        <p class="mt-4">
          <span>Giving you an airdrop challenge code:</span>
        </p>
        <div class="my-2 flex flex-row items-center gap-1">
          <p class="truncate text-panda">{result.airdrop_cryptogram[0]}</p>
          <TextClipboardButton
            textValue={String(result.airdrop_cryptogram[0])}
          />
        </div>
        <p class="text-left">
          It is available for <b>{cryptogramInfo[4]}</b> people to claim
          airdrop, valid for <b>{(cryptogramInfo[2] || 10080) / (24 * 60)}</b> days.
        </p>
        <p class="text-left">
          When a new user claims the airdrop using your code, you'll also
          receive an additional <b
            >{Number($luckyPoolState?.airdrop_amount[0] || 10n) / 2}</b
          > tokens per user.
        </p>
      {/if}
    </div>
  {:else}
    {#if stepN > 0}
      <h3 class="h3 !mt-0 text-center">🎰</h3>
      <div class="!mt-0 text-center text-xl font-bold">Lucky Draw</div>
      <h5 class="h5">Good Luck To You</h5>
      <div class="flex flex-row items-center gap-2">
        <span class="text-panda *:h-6 *:w-6">
          {#if stepN == 1}
            <IconCircleSpin />
          {:else}
            <IconCheckbox />
          {/if}
        </span>
        <span>Approving allowance</span>
      </div>
      {#if stepN > 1}
        <div class="flex flex-row items-center gap-2">
          <span class="text-panda *:h-6 *:w-6">
            <IconCircleSpin />
          </span>
          <span>Drawing PANDA tokens</span>
        </div>
      {/if}
    {:else}
      <h3 class="h3 !mt-0 text-center">🎰</h3>
      <div class="!mt-0 text-center text-xl font-bold">Lucky Draw</div>
      <div class="space-y-2 rounded-xl bg-gray/5 p-4">
        <h5 class="h5 font-extrabold text-panda">CALCULATION RULES</h5>
        <div>
          <b>1. Entry Range:</b>
          <span class="text-gray/50">0.1 ≤ ICP Quantity ≤ 10</span>
        </div>
        <div>
          <b>2. Random Remainder:</b>
          <span class="text-gray/50">0 ≤ R ≤ 3446</span>
          <br />
          <span class="text-sm text-gray/50">
            (A random number is generated, divided by the 'PANDA number',
            yielding a remainder R)
          </span>
        </div>
        <div>
          <p><b>3. Winning Base (B):</b></p>
          <ul class="ml-4 list-disc text-gray/50">
            <li>
              <span>
                {'If R ≤ 5, then B = 100,000 (highest)'}
              </span>
            </li>
            <li>
              <span>
                {'If 5 < R ≤ 1,000, then B = 1,000 (lowest)'}
              </span>
            </li>
            <li>
              <span>
                {'Otherwise, B = R (1,001 ≤ B ≤ 3,446)'}
              </span>
            </li>
          </ul>
        </div>
        <p><b>4. Total Prize = ICP Quantity × B</b></p>
      </div>
    {/if}
    <hr class="!border-t-1 mx-[-24px] !mt-6 !border-dashed !border-gray/20" />
    <div class="!mt-5 text-sm">
      <div class="flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-2">
          <span class="*:size-6"><IconIcLogo /></span>
          <b>Your ICP Balance:</b>
        </div>
        <div class="flex flex-row gap-1 text-gray/50">
          <span>{icpBalanceDisplay.display()}</span>
          <span>{icpBalanceDisplay.token.symbol}</span>
        </div>
      </div>
      <div class="mt-1 flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-2">
          <span class="*:size-6"><IconGoldPanda /></span>
          <b>Lucky Pool Balance:</b>
        </div>
        <div class="flex flex-row gap-1 text-gray/50">
          <span>{luckyPoolBalanceDisplay.display()}</span>
          <span>{luckyPoolBalanceDisplay.token.symbol}</span>
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
          name="icpTokens"
          min="0.1"
          max="10"
          step="0.1"
          bind:value={inputAmount}
          placeholder="Enter an amount between 0.1 and 10"
          disabled={submitting}
          required
        />
        <div class="absolute right-2 top-2 outline-0">ICP</div>
      </div>
      <div class="flex flex-row gap-2 text-sm">
        <button
          class="btn rounded-md bg-gray/5 px-3 py-1 outline-0"
          disabled={submitting}
          on:click={(e) => setAmount(e, 0.1)}
        >
          0.1
        </button>
        <button
          class="btn rounded-md bg-gray/5 px-3 py-1 outline-0"
          disabled={submitting}
          on:click={(e) => setAmount(e, 0.5)}
        >
          0.5
        </button>
        <button
          class="btn rounded-md bg-gray/5 px-3 py-1 outline-0"
          disabled={submitting}
          on:click={(e) => setAmount(e, 1)}
        >
          1
        </button>
        <button
          class="btn rounded-md bg-gray/5 px-3 py-1 outline-0"
          disabled={submitting}
          on:click={(e) => setAmount(e, 10)}
        >
          10
        </button>
      </div>
    </form>
    <footer class="">
      <button
        class="variant-filled-primary btn w-full text-white"
        disabled={submitting || !validating}
        on:click={onFormSubmit}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Draw Now</span>
        {/if}
      </button>
    </footer>
  {/if}
</ModalCard>
