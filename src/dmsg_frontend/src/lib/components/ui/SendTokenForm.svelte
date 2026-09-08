<script lang="ts">
  import { t, locale } from '$lib/i18n'
  import { untrack } from 'svelte'
  import IconArrowDown from '$lib/components/icons/IconArrowDown.svelte'
  import IconCornerDownLeft from '$lib/components/icons/IconCornerDownLeft.svelte'
  import { ErrData } from '$lib/types/result'
  import type { SendTokenArgs } from '$lib/types/token'
  import { AccountIdentifier } from '$lib/utils/account_identifier'
  import { TokenDisplay, type TokenInfo } from '$lib/utils/token'
  import { Principal } from '@icp-sdk/core/principal'
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
    balance: bigint
    amount: bigint
    total: bigint
  } | null = $state(null)

  // This dialog owns an editable transaction model (including exact Max
  // amounts). Recreating it on renders would discard the user's input.
  const tokenDisplay = untrack(() => new TokenDisplay(token, 0n))

  const addressTip = $derived(
    $t(
      token.symbol == 'ICP'
        ? 'Enter a valid Principal ID or ICP address.'
        : 'Enter a valid Principal ID.'
    )
  )

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
    // Native input handlers run before bind:value updates sendTo.
    const address = input.value

    if (token.symbol == 'ICP' && !address.includes('-')) {
      try {
        AccountIdentifier.fromHex(address)
      } catch (error) {
        input.setCustomValidity($t('Invalid ICP address'))
        return
      }
    } else {
      try {
        Principal.fromText(address)
      } catch (error) {
        input.setCustomValidity($t('Invalid principal'))
        return
      }
    }

    input.setCustomValidity('')
  }

  function validateAmount(e: Event) {
    const input = e.target as HTMLInputElement
    if (tokenDisplay.total > availableBalance) {
      input.setCustomValidity($t('Amount exceeds available balance'))
      return
    }

    if (sendAmount <= 0.001) {
      input.setCustomValidity($t('Amount must be greater than 0.001'))
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
      input?.setCustomValidity($t('Amount must be greater than 0.001'))
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
      balance: availableBalance,
      amount: tokenDisplay.amount,
      total: tokenDisplay.total
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
    <form class="flex flex-col" bind:this={formRef} oninput={onFormChange}>
      <label class="label">
        <span>{$t('Send to destination')}</span>
        <input
          class="border-gray/10 peer input valid:input-success truncate rounded-xl bg-white/20"
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
        <span class="text-error-500 invisible text-xs peer-invalid:visible">
          {addressTip}
        </span>
      </label>
      <label class="label">
        <span>{$t('Amount')}</span>
        <a
          class="btn hover:text-secondary-500/100 float-right !mt-0 p-0"
          href="/"
          onclick={setMaxAmount}
        >
          <span class="*:w-5"><IconCornerDownLeft /></span>
          <span class="!ml-1">{$t('Max')}</span>
        </a>
        <input
          class="border-gray/10 peer input valid:input-success truncate rounded-xl bg-white/20"
          type="number"
          name="amount"
          min="0"
          step="any"
          bind:value={sendAmount}
          oninput={validateAmount}
          placeholder={$t('Amount')}
          disabled={submitting}
          required
        />
        <span class="text-error-500 invisible text-xs peer-invalid:visible">
          {$t('Enter a valid amount.')}
        </span>
      </label>
      <div>
        <p>{$t('Transaction Fee (billed to source)')}</p>
        <p>{$locale && tokenDisplay.displayFee()} {token.symbol}</p>
      </div>
    </form>
    <!-- prettier-ignore -->
    <footer class="flex flex-row justify-end gap-4">
			<button class="btn btn-md variant-ghost-surface" disabled={submitting} onclick={onClear}>{$t("Clear")}</button>
			<button class="btn btn-md variant-ghost-primary" disabled={submitting || !validating} onclick={onContinue}>{$t("Continue")}</button>
		</footer>
  </div>
{:else if txInfo != null}
  <div class="flex w-full flex-col gap-4">
    <div class="flex flex-col gap-2 text-sm *:gap-2">
      <h4 class="h4 text-center">{$t('Review Transaction')}</h4>
      <div class="flex flex-row justify-between">
        <span>{$t('From')}</span>
        <span class="min-w-0 text-right text-pretty break-all">
          {txInfo.from}
        </span>
      </div>
      <div class="flex flex-row justify-between">
        <span>{$t('Available Balance')}</span>
        <span class="text-right text-pretty break-words">
          {$locale && tokenDisplay.displayValue(txInfo.balance)}
          {token.symbol}
        </span>
      </div>
      <div class="flex flex-row justify-between">
        <span>{$t('Sending Amount')}</span>
        <span class="text-right">
          {$locale && tokenDisplay.displayValue(txInfo.amount)}
          {token.symbol}
        </span>
      </div>
      <div class="flex flex-row justify-between">
        <span>{$t('Transaction Fee')}</span>
        <span class="text-right"
          >{$locale && tokenDisplay.displayFee()} {token.symbol}</span
        >
      </div>
      <div class="flex flex-row justify-between">
        <span>{$t('Total Deducted')}</span>
        <span class="text-right">
          {$locale && tokenDisplay.displayValue(txInfo.total)}
          {token.symbol}
        </span>
      </div>
      <div class="text-panda flex flex-row justify-end *:scale-110">
        <IconArrowDown />
      </div>
      <div class="flex flex-row justify-between">
        <span>{$t('Received Amount')}</span>
        <span class="text-right">
          {$locale && tokenDisplay.displayValue(txInfo.amount)}
          {token.symbol}
        </span>
      </div>
      <div class="flex flex-row justify-between">
        <span>{$t('To')}</span>
        <p class="min-w-0 text-right text-pretty break-all">
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
        <p class="text-success-500 text-lg">
          {$t('Transfer succeeded at block {block}.', {
            block: transferSuccess
          })}
        </p>
      {:else if transferError != null}
        <p class="text-error-500 text-lg">{$t('Transfer failed')}</p>
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
        {$t('Edit Transaction')}
      </button>
      <button
        class="variant-ghost-primary btn max-md:btn-sm"
        disabled={submitting || !validating}
        onclick={onFormSubmit}
      >
        {$t('Send Now')}
      </button>
    </footer>
  </div>
{/if}
