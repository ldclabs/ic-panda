<script lang="ts">
  import IconArrowDown from '$lib/components/icons/IconArrowDown.svelte'
  import IconCornerDownLeft from '$lib/components/icons/IconCornerDownLeft.svelte'
  import { ErrData } from '$lib/types/result'
  import type { SendTokenArgs } from '$lib/types/token'
  import { AccountIdentifier } from '$lib/utils/account_identifier'
  import { TokenDisplay, type TokenInfo } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import Loading from './Loading.svelte'

  interface Props {
    token: TokenInfo
    availableBalance?: bigint
    sendFrom: Principal
    onSubmit: (args: SendTokenArgs) => Promise<bigint>
  }

  let { token, availableBalance = 0n, sendFrom, onSubmit }: Props = $props()

  let stepN: 0 | 1 = $state(0)
  let submitting = $state(false)
  let validating = $state(false)
  let sendTo = $state('')
  let sendAmount = $state(0)
  let formRef: HTMLFormElement | undefined = $state()
  let transferSuccess: bigint | null = $state(null)
  let transferError: ErrData<any> | null = $state(null)
  let txInfo: {
    from: string
    to: string
    balance: string
    amount: string
    total: string
  } | null = $state(null)

  let tokenDisplay = $state(new TokenDisplay(token, 0n))

  const addressTip =
    'Principal' + (token.symbol == 'ICP' ? ' or ICP Address' : '')

  function setMaxAmount(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    if (formRef) {
      tokenDisplay.amount =
        availableBalance > token.fee ? availableBalance - token.fee : 0n
      sendAmount = tokenDisplay.num
      const input = formRef['amount'] as HTMLInputElement
      input?.setCustomValidity('')
      validating = formRef.checkValidity()
    }
  }

  function validateAddress(e: Event) {
    const input = e.target as HTMLInputElement

    if (token.symbol == 'ICP' && !sendTo.includes('-')) {
      try {
        AccountIdentifier.fromHex(sendTo)
      } catch (error) {
        input.setCustomValidity('Invalid ICP address')
        return
      }
    } else {
      try {
        Principal.fromText(sendTo)
      } catch (error) {
        input.setCustomValidity('Invalid principal')
        return
      }
    }

    input.setCustomValidity('')
  }

  function validateAmount(e: Event) {
    const input = e.target as HTMLInputElement
    if (tokenDisplay.total > availableBalance) {
      input.setCustomValidity('Amount exceeds available balance')
      return
    }

    if (sendAmount <= 0.001) {
      input.setCustomValidity('Amount must be greater than 0.001')
      return
    }

    if (input.value.startsWith('0')) {
      input.value = sendAmount.toString()
    }

    input.setCustomValidity('')
  }

  function onFormChange(e: Event) {
    const form = e.currentTarget as HTMLFormElement
    if (sendAmount <= 0.001) {
      const input = form['amount'] as HTMLInputElement
      input?.setCustomValidity('Amount must be greater than 0.001')
    }
    tokenDisplay.num = sendAmount || 0
    validating = form.checkValidity()
  }

  function onClear() {
    sendTo = ''
    sendAmount = 0

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
      to: sendTo,
      balance: tokenDisplay.displayValue(availableBalance),
      amount: tokenDisplay.display(),
      total: tokenDisplay.displayTotal()
    }
  }

  function onPrevStep() {
    stepN = 0
    transferSuccess = null
    transferError = null
  }

  function onFormSubmit() {
    submitting = true
    onSubmit({
      to: sendTo,
      amount: tokenDisplay.amount
    })
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
</script>

{#if stepN === 0}
  <div class="flex w-full flex-col gap-4">
    <!-- Enable for debugging: -->
    <form class="flex flex-col" bind:this={formRef} onchange={onFormChange}>
      <label class="label">
        <span>Send to destination</span>
        <input
          class="border-gray/10 peer input truncate rounded-xl bg-white/20 valid:input-success"
          type="text"
          name="sendTo"
          minlength="8"
          maxlength="65"
          data-1p-ignore
          bind:value={sendTo}
          oninput={validateAddress}
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
          onclick={setMaxAmount}
        >
          <span class="*:w-5"><IconCornerDownLeft /></span>
          <span class="!ml-1">Max</span>
        </a>
        <input
          class="border-gray/10 peer input truncate rounded-xl bg-white/20 valid:input-success"
          type="number"
          name="amount"
          min="0"
          step="any"
          bind:value={sendAmount}
          oninput={validateAmount}
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
        <p>{tokenDisplay.displayFee()} {token.symbol}</p>
      </div>
    </form>
    <!-- prettier-ignore -->
    <footer class="flex flex-row justify-end gap-4">
			<button class="btn btn-md variant-ghost-surface" disabled={submitting} onclick={onClear}>Clear</button>
			<button class="btn btn-md variant-ghost-primary" disabled={submitting || !validating} onclick={onContinue}>Continue</button>
		</footer>
  </div>
{:else if txInfo != null}
  <div class="flex w-full flex-col gap-4">
    <div class="flex flex-col gap-2 text-sm *:gap-2">
      <h4 class="h4 text-center">Review Transaction</h4>
      <div class="flex flex-row justify-between">
        <span>From</span>
        <span class="min-w-0 text-pretty break-all text-right">
          {txInfo.from}
        </span>
      </div>
      <div class="flex flex-row justify-between">
        <span>Available Balance</span>
        <span class="text-pretty break-all text-right">
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
        <span class="text-right"
          >{tokenDisplay.displayFee()} {token.symbol}</span
        >
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
        <p class="min-w-0 text-pretty break-all text-right">
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
        onclick={onPrevStep}
      >
        Edit Transaction
      </button>
      <button
        class="variant-ghost-primary btn max-md:btn-sm"
        disabled={submitting || !validating}
        onclick={onFormSubmit}
      >
        Send Now
      </button>
    </footer>
  </div>
{/if}
