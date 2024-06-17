<script lang="ts">
  import { CKDogeMinterAPI } from '$lib/canisters/ckdogeminter'
  import { TokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconArrowDown from '$lib/components/icons/IconArrowDown.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { errMessage } from '$lib/types/result'
  import { Chain, toHashString } from '$lib/utils/dogecoin'
  import {
    DOGEToken,
    TokenAmount,
    ckDOGEToken,
    formatToken,
    type TokenAmountDisplay,
    type TokenInfo
  } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { focusTrap } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let ckDogeMinterAPI: CKDogeMinterAPI
  export let tokenLedgerAPI: TokenLedgerAPI
  export let principal: string
  export let dogeAddress: string
  export let availableBalance: bigint = 0n
  export let chain: Chain
  export let onFinish: () => void

  const token: TokenInfo = Object.assign({}, ckDOGEToken)

  let stepN: 0 | 1 = 0
  let submitting = false
  let validating = false
  let formRef: HTMLFormElement
  let tokenName: 'DOGE' | 'ckDOGE' = 'ckDOGE'
  let sendError = ''
  let sendFrom = principal
  let sendTo = ''
  let sendAmount = 0
  let sendFee = getTextAmount(token.fee)
  let txOutput: { txid: string; url: string; explorer: string } | null = null
  let tokenAmount = TokenAmount.fromNumber({
    amount: sendAmount || 0,
    token
  })

  function switchTokenName(name: 'DOGE' | 'ckDOGE') {
    sendTo = ''
    tokenName = name
    switch (name) {
      case 'DOGE':
        sendFrom = dogeAddress
        token.fee = DOGEToken.fee
        sendFee = getTextAmount(token.fee)
        break
      case 'ckDOGE':
        sendFrom = principal
        token.fee = ckDOGEToken.fee
        sendFee = getTextAmount(token.fee)
        break
    }
  }

  function getTextAmount(amount: bigint): TokenAmountDisplay {
    return formatToken(TokenAmount.fromUlps({ amount, token }))
  }

  async function sendToCopyPaste(e: Event) {
    e.preventDefault()

    if (sendTo == '') {
      sendTo = await navigator.clipboard.readText()
    } else {
      sendTo = ''
    }
    onFormChange()
  }

  function setMaxAmount(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    if (formRef) {
      sendAmount = getTextAmount(availableBalance - token.fee).amountNum
      const input = formRef['sendAmount'] as HTMLInputElement
      input?.setCustomValidity('')
      validating = formRef.checkValidity()
    }
  }

  function onFormChange(e?: Event) {
    e?.preventDefault()

    const form = (e?.currentTarget as HTMLFormElement) || formRef

    const sendToEle = form['sendTo'] as HTMLInputElement
    sendToEle?.setCustomValidity('')
    const _sendTo = sendTo.trim()
    try {
      if (tokenName == 'DOGE') {
        chain.decodeAddress(_sendTo)
      } else {
        Principal.fromText(_sendTo)
      }
    } catch (err) {
      sendToEle?.setCustomValidity(errMessage(err))
    }

    const sendAmountEle = form['sendAmount'] as HTMLInputElement
    sendAmountEle?.setCustomValidity('')
    if (sendAmount < 0.01) {
      sendAmountEle?.setCustomValidity('Amount must be greater than 0.01')
    }
    if (tokenName == 'DOGE' && sendAmount < 0.1) {
      sendAmountEle?.setCustomValidity('Amount must be greater than 0.1')
    }
    if (sendAmountEle?.value.startsWith('0')) {
      sendAmountEle.value = sendAmount.toString()
    }
    tokenAmount = TokenAmount.fromNumber({
      amount: sendAmount || 0,
      token
    })

    if (tokenAmount.toUlps() + token.fee > availableBalance) {
      sendAmountEle?.setCustomValidity('Amount exceeds available balance')
    }

    validating = form.checkValidity()
  }

  function onClear() {
    sendTo = ''
    sendAmount = 0
    sendFee = getTextAmount(token.fee)

    stepN = 0
    submitting = false
    validating = false
    sendError = ''
    txOutput = null
  }

  async function onContinue() {
    stepN = 1
    sendError = ''
    tokenAmount = TokenAmount.fromNumber({
      amount: sendAmount || 0,
      token
    })

    if (tokenName == 'DOGE') {
      await tokenLedgerAPI.ensureAllowance(
        ckDogeMinterAPI.canisterId,
        tokenAmount.toUlps()
      )
    }
  }

  function onPrevStep() {
    stepN = 0
    sendError = ''
  }

  async function onFormSubmit() {
    submitting = true
    try {
      switch (tokenName) {
        case 'DOGE':
          const output = await ckDogeMinterAPI.burnCKDoge({
            fee_rate: 0n,
            address: sendTo,
            amount: tokenAmount.toUlps()
          })

          const txid = toHashString(output.txid)
          txOutput = {
            txid,
            url: chain.txExplorer(txid),
            explorer: chain.explorer
          }
          break
        case 'ckDOGE':
          let blk = await tokenLedgerAPI.transfer(sendTo, tokenAmount)

          txOutput = {
            txid: `block index: ${blk}`,
            url: 'https://dashboard.internetcomputer.org/',
            explorer: 'https://dashboard.internetcomputer.org/'
          }
          break
      }

      onFinish()
    } catch (err) {
      sendError = errMessage(err)
      submitting = false
    } finally {
      submitting = false
      validating = false
    }
  }
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold"
    >{tokenName == 'DOGE' ? 'Withdraw DOGE' : 'Send ckDOGE'}</div
  >
  {#if stepN === 0}
    <div class="!mt-6 flex flex-row items-center justify-center gap-2">
      <button
        class="variant-filled btn btn-sm w-32 rounded-md {tokenName == 'ckDOGE'
          ? 'bg-gray text-white'
          : 'bg-gray/20 text-black'}"
        on:click={() => {
          switchTokenName('ckDOGE')
        }}
      >
        ckDOGE
      </button>
      <button
        class="variant-filled btn btn-sm w-32 rounded-md {tokenName == 'DOGE'
          ? 'bg-gray text-white'
          : 'bg-gray/20 text-black'}"
        on:click={() => {
          switchTokenName('DOGE')
        }}
      >
        DOGE
      </button>
    </div>
    <div class="flex w-full flex-col gap-4">
      <form
        class="flex flex-col"
        bind:this={formRef}
        on:input={onFormChange}
        use:focusTrap={true}
      >
        <label class="label">
          <span>Destination</span>
          <div class="relative">
            <input
              class="peer input truncate rounded-xl border-gray/10 bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
              type="text"
              name="sendTo"
              minlength="32"
              maxlength="36"
              bind:value={sendTo}
              placeholder="Address"
              disabled={submitting}
              required
            />
            <button
              class="btn absolute right-0 top-0 outline-0"
              disabled={submitting}
              on:click={sendToCopyPaste}
            >
              {#if sendTo == ''}
                <span>Paste</span>
              {:else}
                <span class="*:scale-90"><IconDeleteBin /></span>
              {/if}
            </button>
          </div>
          <span class="invisible text-xs text-error-500 peer-invalid:visible">
            Enter a valid address.
          </span>
        </label>
        <label class="label">
          <span>Amount</span>
          <div class="relative">
            <input
              class="peer input truncate rounded-xl border-gray/10 bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
              type="number"
              name="sendAmount"
              min="0"
              step="any"
              bind:value={sendAmount}
              placeholder="Amount"
              disabled={submitting}
              required
            />
            <button
              class="btn absolute right-0 top-0 outline-0"
              disabled={submitting}
              on:click={setMaxAmount}
            >
              <span>Max</span>
            </button>
          </div>
          <span class="invisible text-xs text-error-500 peer-invalid:visible">
            Enter a valid amount.
          </span>
        </label>
        <div>
          <p
            >Transaction Fee {tokenName == 'ckDOGE'
              ? '(billed to source)'
              : ''}</p
          >
          <p>{sendFee.full} {token.symbol}</p>
        </div>
      </form>
      <!-- prettier-ignore -->
      <footer class="flex flex-row items-center justify-center gap-4">
        <button class="btn w-36 border-[1px] border-black font-medium text-black" disabled={submitting} on:click={onClear}>Cancel</button>
        <button class="variant-filled btn w-36 bg-gray" disabled={submitting || !validating} on:click={onContinue}>
          {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Continue</span>
        {/if}
        </button>
      </footer>
    </div>
  {:else if stepN === 1}
    {@const tokenAmountDisplay = formatToken(tokenAmount)}
    <div class="flex w-full flex-col gap-4">
      <div class="flex flex-col gap-2 text-sm *:gap-2">
        <h4 class="h4 text-center">Review Transaction</h4>
        <div class="flex flex-row justify-between">
          <span>From</span>
          <span class="min-w-0 text-pretty break-words text-right">
            {sendFrom}
          </span>
        </div>
        <div class="flex flex-row justify-between">
          <span>Available Balance</span>
          <span class="text-pretty break-words text-right">
            {getTextAmount(availableBalance).full}
            {token.symbol}
          </span>
        </div>
        <div class="flex flex-row justify-between">
          <span>Sending Amount</span>
          <span class="text-right">
            {tokenAmountDisplay.full}
            {token.symbol}
          </span>
        </div>
        <div class="flex flex-row justify-between">
          <span>Transaction Fee</span>
          <span class="text-right">{sendFee.full} {token.symbol}</span>
        </div>
        <div class="flex flex-row justify-between">
          <span>Total Deducted</span>
          <span class="text-right">
            {tokenAmountDisplay.feeAndFull}
            {token.symbol}
          </span>
        </div>
        <div class="flex flex-row justify-end text-panda *:scale-110">
          <IconArrowDown />
        </div>
        <div class="flex flex-row justify-between">
          <span>Received Amount</span>
          <span class="text-right">
            {tokenAmountDisplay.full}
            {token.symbol}
          </span>
        </div>
        <div class="flex flex-row justify-between">
          <span>To</span>
          <p class="min-w-0 text-pretty break-words text-right">
            {sendTo}
          </p>
        </div>
      </div>
      <div class="flex flex-col items-center justify-center">
        {#if submitting}
          <span class="text-panda *:h-8 *:w-8"><Loading /></span>
        {:else if txOutput}
          <p class="text-lg text-success-500">Send transaction Success</p>
          <p class="min-w-0 max-w-full text-pretty break-words">
            {txOutput.txid}, check it on the
            <a
              class="font-bold text-secondary-500 underline underline-offset-4"
              href={txOutput.url}
              target="_blank">{txOutput.explorer}</a
            >.
          </p>
        {:else if sendError}
          <p class="text-lg text-error-500">Transfer failed</p>
          <p class="min-w-0 max-w-full text-pretty break-words">
            {sendError}
          </p>
        {/if}
      </div>
      <footer class="flex flex-row items-center justify-center gap-4">
        <button
          class="btn w-36 border-[1px] border-black text-black"
          disabled={submitting}
          on:click={onPrevStep}>Edit</button
        >
        <button
          class="variant-filled btn w-36 bg-gray"
          disabled={submitting || !validating}
          on:click={onFormSubmit}>Send Now</button
        >
      </footer>
    </div>
  {/if}
</ModalCard>
