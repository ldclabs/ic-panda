<script lang="ts">
  import {
    TokenAmount,
    formatToken,
    type TokenInfo,
    type TokenAmountDisplay
  } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { AccountIdentifier } from '$lib/utils/account_identifier'
  import IconCornerDownLeft from '$lib/components/icons/IconCornerDownLeft.svelte'
  import IconArrowDown from '$lib/components/icons/IconArrowDown.svelte'
  import type { SendTokenArgs } from '$lib/types/token'
  import { ErrData } from '$lib/types/result'
  import Loading from './Loading.svelte'

  export let token: TokenInfo
  export let availableBalance: bigint = 0n
  export let sendFrom: Principal
  export let onSubmit: (args: SendTokenArgs) => Promise<bigint>

  let stepN: 0 | 1 = 0
  let submitting = false
  let validating = false
  let formRef: HTMLFormElement
  let transferSuccess: bigint | null = null
  let transferError: ErrData<any> | null = null
  let txInfo: {
    from: string
    to: string
    balance: string
    amount: string
    total: string
  } | null = null

  const tokenFee = getTextAmount(token.fee)

  const addressTip =
    'Principal' + (token.symbol == 'ICP' ? ' or ICP Address' : '')

  // Form Data
  const formData: SendTokenArgs = {
    tokenAmount: TokenAmount.fromUlps({ amount: 0n, token }),
    to: '',
    amount: 0
  }

  function getTextAmount(amount: bigint): TokenAmountDisplay {
    return formatToken(TokenAmount.fromUlps({ amount, token }))
  }

  function setMaxAmount(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    if (formRef) {
      formData.amount = getTextAmount(availableBalance - token.fee).amountNum
      const input = formRef['amount'] as HTMLInputElement
      input?.setCustomValidity('')
      validating = formRef.checkValidity()
    }
  }

  function validateAddress(e: Event) {
    const input = e.target as HTMLInputElement

    if (token.symbol == 'ICP' && !formData.to.includes('-')) {
      try {
        AccountIdentifier.fromHex(formData.to)
      } catch (error) {
        input.setCustomValidity('Invalid ICP address')
        return
      }
    } else {
      try {
        Principal.fromText(formData.to)
      } catch (error) {
        input.setCustomValidity('Invalid principal')
        return
      }
    }

    input.setCustomValidity('')
  }

  function validateAmount(e: Event) {
    const input = e.target as HTMLInputElement
    if (
      formData.amount > getTextAmount(availableBalance - token.fee).amountNum
    ) {
      input.setCustomValidity('Amount exceeds available balance')
      return
    }

    if (formData.amount <= 0.001) {
      input.setCustomValidity('Amount must be greater than 0.001')
      return
    }

    if (input.value.startsWith('0')) {
      input.value = formData.amount.toString()
    }

    input.setCustomValidity('')
  }

  function onFormChange(e: Event) {
    const form = e.currentTarget as HTMLFormElement
    if (formData.amount <= 0.001) {
      const input = form['amount'] as HTMLInputElement
      input?.setCustomValidity('Amount must be greater than 0.001')
    }
    validating = form.checkValidity()
  }

  function onClear() {
    formData.to = ''
    formData.amount = 0

    stepN = 0
    submitting = false
    validating = false
    transferSuccess = null
    transferError = null
    txInfo = null
  }

  function onContinue() {
    stepN = 1
    transferSuccess = null
    transferError = null
    txInfo = {
      from: sendFrom.toString(),
      to: formData.to,
      balance: getTextAmount(availableBalance).full,
      amount: tokenAmountDisplay.full,
      total: tokenAmountDisplay.feeAndFull
    }
  }

  function onPrevStep() {
    stepN = 0
    transferSuccess = null
    transferError = null
  }

  function onFormSubmit() {
    submitting = true
    onSubmit(formData)
      .then((n) => {
        submitting = false
        validating = false
        transferSuccess = n
      })
      .catch((err) => {
        submitting = false
        transferError = err
      })
  }

  $: formData.tokenAmount = TokenAmount.fromNumber({
    amount: formData.amount || 0,
    token
  })
  $: tokenAmountDisplay = formatToken(formData.tokenAmount)
</script>

{#if stepN === 0}
  <div class="flex w-full flex-col gap-4">
    <!-- Enable for debugging: -->
    <form class="flex flex-col" bind:this={formRef} on:change={onFormChange}>
      <label class="label">
        <span>Send to destination</span>
        <input
          class="peer input bg-gray/5 valid:input-success hover:bg-white/90"
          type="text"
          name="to"
          minlength="8"
          maxlength="65"
          bind:value={formData.to}
          on:input={validateAddress}
          placeholder={addressTip}
          disabled={submitting}
          required
        />
        <span class="invisible text-xs text-error-500 peer-invalid:visible">
          Enter a valid {addressTip}.
        </span>
      </label>
      <label class="label">
        <span>Amount</span>
        <a
          class="btn float-right !mt-0 p-0 hover:text-secondary-500/100"
          href="/"
          on:click={setMaxAmount}
        >
          <span class="*:w-5"><IconCornerDownLeft /></span>
          <span class="!ml-1">Max</span>
        </a>
        <input
          class="peer input bg-gray/5 valid:input-success hover:bg-white/90"
          type="number"
          name="amount"
          min="0"
          step="any"
          bind:value={formData.amount}
          on:input={validateAmount}
          placeholder="Amount"
          disabled={submitting}
          required
        />
        <span class="invisible text-xs text-error-500 peer-invalid:visible">
          Enter a valid amount.
        </span>
      </label>
      <div>
        <p>Transaction Fee (billed to source)</p>
        <p>{tokenFee.full} {token.symbol}</p>
      </div>
    </form>
    <!-- prettier-ignore -->
    <footer class="flex flex-row justify-end gap-4">
			<button class="btn variant-ghost-surface max-md:btn-sm" disabled={submitting} on:click={onClear}>Clear</button>
			<button class="btn variant-ghost-primary max-md:btn-sm" disabled={submitting || !validating} on:click={onContinue}>Continue</button>
		</footer>
  </div>
{:else if txInfo != null}
  <div class="flex w-full flex-col gap-4">
    <div class="flex flex-col gap-2 text-sm *:gap-2">
      <h4 class="h4 text-center">Review Transaction</h4>
      <div class="flex flex-row justify-between">
        <span>From</span>
        <span class="min-w-0 text-pretty break-words text-right">
          {txInfo.from}
        </span>
      </div>
      <div class="flex flex-row justify-between">
        <span>Available Balance</span>
        <span class="text-pretty break-words text-right">
          {txInfo.balance}
          {token.symbol}
        </span>
      </div>
      <div class="flex flex-row justify-between">
        <span>Sending Amount</span>
        <span class="text-right">
          {txInfo.amount}
          {token.symbol}
        </span>
      </div>
      <div class="flex flex-row justify-between">
        <span>Transaction Fee</span>
        <span class="text-right">{tokenFee.full} {token.symbol}</span>
      </div>
      <div class="flex flex-row justify-between">
        <span>Total Deducted</span>
        <span class="text-right">
          {txInfo.total}
          {token.symbol}
        </span>
      </div>
      <div class="flex flex-row justify-end text-panda *:scale-110">
        <IconArrowDown />
      </div>
      <div class="flex flex-row justify-between">
        <span>Received Amount</span>
        <span class="text-right">
          {txInfo.amount}
          {token.symbol}
        </span>
      </div>
      <div class="flex flex-row justify-between">
        <span>To</span>
        <p class="min-w-0 text-pretty break-words text-right">
          {txInfo.to}
        </p>
      </div>
    </div>
    <div
      class="flex flex-col items-center justify-center transition duration-300 ease-in-out"
    >
      {#if submitting}
        <span class="text-panda *:h-8 *:w-8"><Loading /></span>
      {:else if transferSuccess != null}
        <p class="text-lg text-success-500">
          Transfer success at block {transferSuccess}
        </p>
      {:else if transferError != null}
        <p class="text-lg text-error-500">Transfer failed</p>
        <p>
          {JSON.stringify(transferError.data, (key, value) =>
            typeof value === 'bigint' ? value.toString() : value
          )}
        </p>
      {/if}
    </div>
    <footer class="flex flex-row justify-end gap-4">
      <button
        class="variant-ghost-surface btn max-md:btn-sm"
        disabled={submitting}
        on:click={onPrevStep}
      >
        Edit Transaction
      </button>
      <button
        class="variant-ghost-primary btn max-md:btn-sm"
        disabled={submitting || !validating}
        on:click={onFormSubmit}
      >
        Send Now
      </button>
    </footer>
  </div>
{/if}
