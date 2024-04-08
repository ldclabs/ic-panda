<script lang="ts">
  import {
    icpLedgerAPIAsync,
    type ICPLedgerAPI
  } from '$lib/canisters/icpledger'
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type LuckyDrawOutput
  } from '$lib/canisters/luckypool'
  import {
    tokenLedgerAPIAsync,
    type TokenLedgerAPI
  } from '$lib/canisters/tokenledger'
  import AccountDetailModal from '$lib/components/core/AccountDetailModal.svelte'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconGoldPanda from '$lib/components/icons/IconGoldPanda.svelte'
  import IconWallet from '$lib/components/icons/IconWallet.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { LUCKYPOOL_CANISTER_ID } from '$lib/constants'
  import {
    ICPToken,
    PANDAToken,
    TokenAmount,
    formatNumber,
    formatToken
  } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { LottiePlayer } from '@lottiefiles/svelte-lottie-player'
  import {
    focusTrap,
    getModalStore,
    getToastStore
  } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let stepN: 0 | 1 | 2 = 0
  let submitting = false
  let validating = false
  let luckyPoolAPI: LuckyPoolAPI
  let icpLedgerAPI: ICPLedgerAPI
  let tokenLedgerAPI: TokenLedgerAPI
  let icpTokens = 1
  let icpBalance = 0n
  let luckyPoolBalance = 0n
  let result: LuckyDrawOutput
  let lottiePlayerRef: HTMLDivElement

  const luckyPoolPrincipal = Principal.fromText(LUCKYPOOL_CANISTER_ID)
  const lowestPrizeBalance = 500n * PANDAToken.one

  const modalStore = getModalStore()
  const toastStore = getToastStore()

  function icpCostAmount(n: number) {
    try {
      return BigInt(n) * ICPToken.one
    } catch (err) {
      return 0n
    }
  }

  async function onFormSubmit() {
    submitting = true
    try {
      const amount = BigInt(icpTokens) * ICPToken.one
      stepN = 1
      await icpLedgerAPI.ensureAllowance(luckyPoolPrincipal, amount)

      stepN = 2
      result = await luckyPoolAPI.luckydraw({
        icp: icpTokens
      })
      setTimeout(() => {
        lottiePlayerRef?.remove()
      }, 2100)
    } catch (err: any) {
      submitting = false
      stepN = 0
      let message = err?.message || String(err)
      if (err?.data) {
        message += '\n' + JSON.stringify(err.data)
      }
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message
      })
    }

    await luckyPoolAPI.refreshAllState()
  }

  function checkInput() {
    if (icpTokens < 1 || icpTokens > 10 || !Number.isSafeInteger(icpTokens)) {
      return 'Enter an integer between 1 and 10'
    } else {
      const cost = icpCostAmount(icpTokens)
      if (cost == 0n || cost > icpBalance) {
        return 'Insufficient ICP balance in your wallet'
      } else if (BigInt(icpTokens) * lowestPrizeBalance > luckyPoolBalance) {
        return 'Insufficient lucky pool balance'
      }
    }
    return ''
  }

  function onFormChange(e: Event) {
    const form = e.currentTarget as HTMLFormElement
    const input = form['icpTokens'] as HTMLInputElement
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
    icpLedgerAPI = await icpLedgerAPIAsync()
    tokenLedgerAPI = await tokenLedgerAPIAsync()

    icpBalance = await icpLedgerAPI.balance()
    luckyPoolBalance = await tokenLedgerAPI.getBalanceOf(luckyPoolPrincipal)

    validating = checkInput() == ''
  })

  $: luckyPoolBalanceDisplay = formatToken(
    TokenAmount.fromUlps({ amount: luckyPoolBalance, token: PANDAToken })
  )
  $: icpBalanceDisplay = formatToken(
    TokenAmount.fromUlps({ amount: icpBalance, token: ICPToken })
  )
</script>

<ModalCard {parent}>
  {#if result}
    <div
      class="absolute left-0 right-0 top-0 *:m-auto"
      bind:this={lottiePlayerRef}
    >
      <LottiePlayer
        src="/_assets/animations/celebrate.json"
        autoplay={true}
        loop={false}
        controls={false}
        renderer="svg"
        background="transparent"
        height={360}
        width={360}
      />
    </div>
    <div class="text-center">
      <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
        <IconCheckbox />
      </div>
      <p class="mt-6">
        <span>Congratulations on winning</span>
        <span class="font-bold text-panda">
          {formatNumber(Number(result.amount / PANDAToken.one))}
        </span>
        <span>tokens in the lucky draw.</span>
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
          {'+ ' + formatNumber(Number(result.amount / PANDAToken.one))}
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
    {#if stepN > 0}
      <h6 class="h6">Good Luck To You</h6>
      <div class="flex flex-row items-center gap-2">
        <span class="text-panda *:h-6 *:w-6">
          {#if stepN == 1}
            <IconCircleSpin />
          {:else}
            <IconCheckbox />
          {/if}
        </span>
        <span>Approve allowance</span>
      </div>
      {#if stepN > 1}
        <div class="flex flex-row items-center gap-2">
          <span class="text-panda *:h-6 *:w-6">
            <IconCircleSpin />
          </span>
          <span>Draw PANDA tokens</span>
        </div>
      {/if}
    {:else}
      <h6 class="h6">Lucky Pool Draw Rules</h6>
      <ul class="list text-gray/50">
        <li>
          <span>
            {'1. Let N be the number of ICPs entered per draw, with 1<=N<=10.'}
          </span>
        </li>
        <li>
          <span>
            {'2. A random number is generated and divided by the "PANDA number", resulting in a remainder R, where 0<=R<=3446.'}
          </span>
        </li>
        <li class="flex-col !items-start">
          <p>{'3. Define the winning base B as follows:'}</p>
          <p class="pl-2">{'If R<=5, then B=100000 (highest prize);'}</p>
          <p class="pl-2">{'If R<=1000, then B=1000 (lowest prize);'}</p>
          <p class="pl-2">{'Otherwise, B=R, where 1001<=B<=3446.'}</p>
        </li>
        <li>
          <span>{'4. The total PANDA tokens received T=N*B.'}</span>
        </li>
        <li>
          <span>
            {"5. The maximum possible prize is 1000000 PANDA tokens, subject to the lucky pool's token balance."}
          </span>
        </li>
      </ul>
    {/if}
    <hr class="!border-t-1 !border-gray/10" />
    <div class="text-sm">
      <p>
        <span>Lucky Pool Balance:</span>
        <span>{luckyPoolBalanceDisplay.display}</span>
        <span>{luckyPoolBalanceDisplay.symbol}</span>
      </p>
      <p>
        <span>Your ICP Balance:</span>
        <span>{icpBalanceDisplay.display}</span>
        <span>{icpBalanceDisplay.symbol}</span>
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
        <div class="input-group-shim bg-gray/5">ICP</div>
        <input
          class="input rounded-none invalid:input-warning hover:bg-white/90"
          type="number"
          name="icpTokens"
          min="1"
          max="10"
          step="1"
          bind:value={icpTokens}
          placeholder="Enter an integer between 1 and 10"
          disabled={submitting}
          required
        />
      </div>
    </form>
    <footer class="">
      <button
        class="variant-filled-primary btn w-full text-white"
        disabled={submitting || !validating}
        on:click={onFormSubmit}
      >
        Draw Now
      </button>
    </footer>
  {/if}
</ModalCard>
