<script lang="ts">
  import {
    luckyPoolAPI,
    type AddPrizeInputV2 as AddPrizeInput,
    type NameOutput,
    type PrizeOutput
  } from '$lib/canisters/luckypool'
  import { tokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconDice from '$lib/components/icons/IconDice.svelte'
  import IconEqualizer from '$lib/components/icons/IconEqualizer.svelte'
  import IconGoldPanda2 from '$lib/components/icons/IconGoldPanda2.svelte'
  import IconLink from '$lib/components/icons/IconLink.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN, LUCKYPOOL_CANISTER_ID } from '$lib/constants'
  import { errMessage } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { encodeCBOR } from '@ldclabs/cose-ts/utils'
  import {
    focusTrap,
    getModalStore,
    getToastStore
  } from '@skeletonlabs/skeleton'
  import encodeQR from 'qr'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'
  import NameModal from './NameModal.svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  // prizeSubsidy: [u64, u16, u32, u8, u32, u16]
  // pub u64, // Prize fee in PANDA * TOKEN_1
  // pub u16, // Min quantity requirement for subsidy
  // pub u32, // Min total amount tokens requirement for subsidy
  // pub u8,  // Subsidy ratio, [0, 50]
  // pub u32, // Max subsidy tokens per prize
  // pub u16, // Subsidy count limit
  export let prizeSubsidy: [] | [bigint, number, number, number, number, number]

  let submitting = false
  let validating = false
  let nameState: Readable<NameOutput | null>
  let formThis: HTMLFormElement

  let pandaBalance = 0n
  let prizeInputKind = 1 // 0 | 1
  let prizeInputTotalAmount = 1000
  let prizeInputQuantity = 100
  let prizeInputExpire = 60 * 24
  let prizeInputRecipient = ''
  let prizeInputMessage = ''
  let prizeInputLink = ''
  let result: PrizeOutput | null = null

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const luckyPoolPrincipal = Principal.fromText(LUCKYPOOL_CANISTER_ID)

  const prizeInput: AddPrizeInput = {
    total_amount: prizeInputTotalAmount,
    kind: [],
    quantity: prizeInputQuantity,
    expire: prizeInputExpire,
    recipient: [],
    memo: []
  }

  function editName(mode: 0 | 1 | 2) {
    modalStore.close()
    modalStore.trigger({
      type: 'component',
      component: {
        ref: NameModal,
        props: {
          nameEditMode: mode,
          availablePandaBalance: pandaBalance,
          nameState: nameState
        }
      }
    })
  }

  function calcPayment(
    kind: number,
    quantity: number,
    amount: number
  ): [number, bigint] {
    if (prizeSubsidy.length == 6 && kind == 1) {
      if (
        prizeSubsidy[5] > 0 &&
        quantity >= prizeSubsidy[1] &&
        amount >= prizeSubsidy[2]
      ) {
        let subsidy = Math.floor((amount * prizeSubsidy[3]) / 100)
        subsidy = subsidy > prizeSubsidy[4] ? prizeSubsidy[4] : subsidy

        const payment =
          BigInt(amount - subsidy) * PANDAToken.one + prizeSubsidy[0]

        return [subsidy, payment]
      }
    }

    return [0, BigInt(amount) * PANDAToken.one + (prizeSubsidy[0] || 0n)]
  }

  function onPrizeInputKindChange(e: Event) {
    e.preventDefault()

    prizeInputKind = Number((e.currentTarget as HTMLInputElement).value) || 0
  }

  function onPrizeInputExpireChange(e: Event) {
    e.preventDefault()

    prizeInputExpire =
      Number((e.currentTarget as HTMLInputElement).value) || 60 * 24
  }

  async function recipientCopyPaste(e: Event) {
    e.preventDefault()

    if (prizeInputRecipient == '') {
      prizeInputRecipient = await navigator.clipboard.readText()
    } else {
      prizeInputRecipient = ''
    }
    onFormChange()
  }

  async function linkCopyPaste(e: Event) {
    e.preventDefault()

    if (prizeInputLink == '') {
      prizeInputLink = await navigator.clipboard.readText()
    } else {
      prizeInputLink = ''
    }
    onFormChange()
  }

  async function messageCopyPaste(e: Event) {
    e.preventDefault()

    if (prizeInputMessage == '') {
      prizeInputMessage = await navigator.clipboard.readText()
    } else {
      prizeInputMessage = ''
    }
    onFormChange()
  }

  let linkCopied = false
  let timeoutId: any
  async function copyPrizeLink(e: Event, link: string) {
    e.preventDefault()

    if (link != '') {
      await navigator.clipboard.writeText(link)
      linkCopied = true
      if (timeoutId != null) {
        clearTimeout(timeoutId)
      }
      timeoutId = setTimeout(() => {
        linkCopied = false
      }, 5000)
    }
  }

  async function createPrizeHandler(e: Event) {
    e.preventDefault()

    submitting = true
    try {
      prizeInput.total_amount = prizeInputTotalAmount
      prizeInput.kind = [prizeInputKind]
      prizeInput.quantity = prizeInputQuantity
      prizeInput.expire = prizeInputExpire
      if (prizeInputQuantity == 1 && prizeInputRecipient != '') {
        prizeInput.recipient = [Principal.fromText(prizeInputRecipient)]
      }

      const _prizeInputMessage = prizeInputMessage.trim()
      if (_prizeInputMessage != '') {
        prizeInput.memo = [
          encodeCBOR({ message: _prizeInputMessage, link: prizeInputLink })
        ]
      }

      const [_, payment] = calcPayment(
        prizeInputKind,
        prizeInputQuantity,
        prizeInputTotalAmount
      )

      await tokenLedgerAPI.ensureAllowance(
        luckyPoolPrincipal,
        payment + PANDAToken.fee
      )

      result = await luckyPoolAPI.createPrize(prizeInput)
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

  function onFormChange(e?: Event) {
    e?.preventDefault()

    const form = (e?.currentTarget as HTMLFormElement) || formThis

    const prizeInputRecipientEle = form[
      'prizeInputRecipient'
    ] as HTMLInputElement
    prizeInputRecipientEle?.setCustomValidity('')
    if (prizeInputQuantity == 1 && prizeInputRecipient != '') {
      try {
        Principal.fromText(prizeInputRecipient)
      } catch (_) {
        prizeInputRecipientEle?.setCustomValidity('invalid principal')
      }
    }

    const prizeInputMessageEle = form['prizeInputMessage'] as HTMLInputElement
    prizeInputMessageEle?.setCustomValidity('')
    const _prizeInputMessage = prizeInputMessage.trim()
    if (_prizeInputMessage.length > 120) {
      prizeInputMessageEle?.setCustomValidity('message is too long')
    }

    const prizeInputLinkEle = form['prizeInputLink'] as HTMLInputElement
    prizeInputLinkEle?.setCustomValidity('')
    if (prizeInputLink != '') {
      if (_prizeInputMessage.length == 0) {
        prizeInputMessageEle?.setCustomValidity('message is required for link')
      }

      try {
        new URL(prizeInputLink)
      } catch (_) {
        prizeInputLinkEle?.setCustomValidity('invalid link')
      }
    }

    validating = form.checkValidity()
  }

  onMount(async () => {
    pandaBalance = await tokenLedgerAPI.balance()

    await luckyPoolAPI.refreshNameState()
    nameState = luckyPoolAPI.nameStateStore

    onFormChange()
  })
</script>

<ModalCard {parent}>
  {#if result}
    {@const code = result.code[0]}
    {@const link = `${APP_ORIGIN}/?prize=${code}`}
    {@const qrcode = encodeQR(link, 'svg', { ecc: 'high' })}
    <div class="text-panda text-center *:m-auto *:size-12">
      <IconCheckbox />
    </div>
    <div class="text-center">
      <p class="mt-4">
        <span>Your prize has been successfully created.</span>
      </p>
      <p class="text-gray/50 mt-2 text-left">Prize Code:</p>
      <div class="flex flex-row items-center gap-1">
        <p class="truncate text-orange-500">{'PRIZE:' + code}</p>
        <TextClipboardButton textValue={'PRIZE:' + code} />
      </div>
      <p class="text-gray/50 mt-2 text-left">Prize QR Code:</p>
      <div class="relative items-center">
        {@html qrcode}
        <div
          class="absolute top-[calc(50%-26px)] left-[calc(50%-26px)] rounded-full border-4 border-white *:size-12"
          ><IconGoldPanda2 /></div
        >
      </div>
      <p class="text-gray/50 mt-2 text-left">Prize Link:</p>
      <div
        class="flex flex-row items-center gap-2 rounded-lg bg-gradient-to-r from-amber-50 to-red-50 p-2 text-orange-500"
      >
        <p class=""><IconLink /></p>
        <p class="w-full text-left text-balance break-all">
          {link}
        </p>
      </div>
    </div>
    <button
      class="variant-filled btn m-auto mt-6 flex w-52 flex-row items-center gap-2 outline-0 {linkCopied
        ? 'bg-panda'
        : ''}"
      disabled={linkCopied}
      on:click={(e) => copyPrizeLink(e, link)}
    >
      {#if linkCopied}
        <span>Prize Link Copied</span><IconCheckbox />
      {:else}
        <span>Copy the Prize Link</span>
      {/if}
    </button>
  {:else}
    <h3 class="h3 !mt-0 text-center *:m-auto *:size-10"><IconGoldPanda2 /></h3>
    <div class="!mt-0 text-center text-xl font-bold">Panda Prize</div>
    <div class="bg-gray/5 !mt-5 rounded-xl px-4 py-2 text-sm">
      <div class="flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-2">
          <span class="*:size-6"><IconPanda /></span>
          <b>Your Wallet Balance:</b>
        </div>
        <div class="flex flex-row gap-1 font-bold">
          <b>{formatNumber(Number(pandaBalance) / Number(PANDAToken.one))}</b>
          <b>{PANDAToken.symbol}</b>
        </div>
      </div>
    </div>
    <form
      class="flex flex-col gap-4"
      bind:this={formThis}
      on:input={onFormChange}
      use:focusTrap={true}
    >
      <label class="label">
        <span class="text-gray/50">Total prize amount:</span>
        <div class="relative">
          <input
            class="input border-gray/10 invalid:input-warning truncate rounded-xl bg-white/20 pr-16 hover:bg-white/90"
            type="number"
            name="prizeInputTotalAmount"
            min="1"
            max="1000000"
            step="1"
            bind:value={prizeInputTotalAmount}
            disabled={submitting}
            placeholder="Enter an amount between 1 and 1,000,000"
            required
          />
          <div class="absolute top-2 right-2 outline-0">{PANDAToken.symbol}</div
          >
        </div>
      </label>
      <label class="label">
        <span class="text-gray/50">Number of winners:</span>
        <div class="flex flex-row items-center">
          <input
            class="cursor-pointer accent-orange-500 outline-0"
            type="range"
            bind:value={prizeInputQuantity}
            min="1"
            max="10000"
            step="1"
            required
          />
          <input
            class="input border-gray/10 invalid:input-warning ml-4 w-28 truncate rounded-lg bg-white/20 px-2 py-1 hover:bg-white/90"
            type="number"
            name="prizeInputQuantity"
            min="1"
            max="10000"
            step="1"
            bind:value={prizeInputQuantity}
            disabled={submitting}
            required
          />
        </div>
      </label>
      {#if prizeInputQuantity == 1}
        <label class="label">
          <span class="text-gray/50">Designated recipient (Optional):</span>
          <div class="relative">
            <input
              class="input border-gray/10 invalid:input-warning truncate rounded-xl bg-white/20 pr-16 hover:bg-white/90"
              type="text"
              name="prizeInputRecipient"
              bind:value={prizeInputRecipient}
              placeholder="Enter recipient principal"
              disabled={submitting || prizeInputQuantity != 1}
            />
            <button
              class="btn absolute top-0 right-0 outline-0"
              disabled={submitting || prizeInputQuantity != 1}
              on:click={recipientCopyPaste}
            >
              {#if prizeInputRecipient == ''}
                <span>Paste</span>
              {:else}
                <span class="*:scale-90"><IconDeleteBin /></span>
              {/if}
            </button>
          </div>
        </label>
      {:else}
        <label class="label">
          <span class="text-gray/50">Prize distribution:</span>
          <div class="flex flex-row items-center justify-between gap-2">
            <label class="flex items-center space-x-2">
              <input
                class="radio border-gray/10 border-[2px] bg-transparent !outline-0 checked:!border-orange-500 checked:!bg-orange-500"
                type="radio"
                name="prizeInputKind"
                on:change={onPrizeInputKindChange}
                checked={prizeInputKind == 1}
                value="1"
              />
              <p class="flex flex-row items-center gap-1">
                <span class=""><IconDice /></span>
                <span class="">Random</span>
                {#if prizeSubsidy.length == 6 && prizeSubsidy[5] > 0}
                  <span class="text-orange-500">(+Subsidy)</span>
                {/if}
              </p>
            </label>
            <label class="flex items-center space-x-2">
              <input
                class="radio border-gray/10 border-[2px] bg-transparent !outline-0 checked:!border-orange-500 checked:!bg-orange-500"
                type="radio"
                name="prizeInputKind"
                on:change={onPrizeInputKindChange}
                checked={prizeInputKind == 0}
                value="0"
              />
              <p class="flex flex-row items-center gap-1">
                <span class=""><IconEqualizer /></span>
                <span class="">Equal</span>
              </p>
            </label>
          </div>
        </label>
      {/if}
      <label class="label">
        <span class="text-gray/50">Validity period:</span>
        <div class="flex flex-row items-center justify-between gap-2">
          <label class="flex items-center space-x-2">
            <input
              class="radio border-gray/10 border-[2px] bg-transparent !outline-0 checked:!border-orange-500 checked:!bg-orange-500"
              type="radio"
              name="prizeInputExpire"
              on:change={onPrizeInputExpireChange}
              checked={prizeInputExpire == 60}
              value="60"
            />
            <p>1 Hour</p>
          </label>
          <label class="flex items-center space-x-2">
            <input
              class="radio border-gray/10 border-[2px] bg-transparent !outline-0 checked:!border-orange-500 checked:!bg-orange-500"
              type="radio"
              name="prizeInputExpire"
              on:change={onPrizeInputExpireChange}
              checked={prizeInputExpire == 1440}
              value="1440"
            />
            <p>1 Day</p>
          </label>
          <label class="flex items-center space-x-2">
            <input
              class="radio border-gray/10 border-[2px] bg-transparent !outline-0 checked:!border-orange-500 checked:!bg-orange-500"
              type="radio"
              name="prizeInputExpire"
              on:change={onPrizeInputExpireChange}
              checked={prizeInputExpire == 10080}
              value="10080"
            />
            <p>1 Week</p>
          </label>
        </div>
      </label>
      <label class="label mt-2">
        {#if !$nameState?.name}
          <span class="text-gray/50">Your name (Optional):</span>
          <button
            class="btn text-panda !mt-0 !p-0 outline-0"
            on:click={() => editName(0)}
          >
            <span>Set up in Account</span>
          </button>
        {:else}
          <span class="text-gray/50 pr-2">Your name:</span>
          <span>{$nameState?.name}</span>
        {/if}
      </label>
      <label class="label">
        <span class="text-gray/50">Leave a message (Optional):</span>
        <div class="relative">
          <textarea
            class="textarea border-gray/10 invalid:input-warning rounded-xl bg-white/20 hover:bg-white/90"
            rows="2"
            name="prizeInputMessage"
            bind:value={prizeInputMessage}
            disabled={submitting}
          ></textarea>
          <button
            class="btn absolute top-6 right-0 outline-0"
            disabled={submitting}
            on:click={messageCopyPaste}
          >
            {#if prizeInputMessage == ''}
              <span>Paste</span>
            {:else}
              <span class="*:scale-90"><IconDeleteBin /></span>
            {/if}
          </button>
        </div>
      </label>
      <label class="label">
        <span class="text-gray/50">Leave a link (Optional):</span>
        <div class="relative">
          <input
            class="input border-gray/10 invalid:input-warning truncate rounded-xl bg-white/20 pr-16 hover:bg-white/90"
            type="text"
            name="prizeInputLink"
            bind:value={prizeInputLink}
            disabled={submitting}
          />
          <button
            class="btn absolute top-0 right-0 outline-0"
            disabled={submitting}
            on:click={linkCopyPaste}
          >
            {#if prizeInputLink == ''}
              <span>Paste</span>
            {:else}
              <span class="*:scale-90"><IconDeleteBin /></span>
            {/if}
          </button>
        </div>
      </label>
    </form>
    {#if prizeSubsidy.length == 6}
      {@const [subsidy, payment] = calcPayment(
        prizeInputKind,
        prizeInputQuantity,
        prizeInputTotalAmount
      )}
      <hr class="!border-gray/20 mx-[-24px] !mt-4 !border-t-1 !border-dashed" />
      <footer class="!mt-4">
        <div
          class="flex flex-row items-center justify-between text-sm font-medium"
        >
          <div class="flex flex-row items-center gap-2">
            <span>Total prize amount:</span>
          </div>
          <div class="flex flex-row gap-1">
            <span>{formatNumber(prizeInputTotalAmount)}</span>
            <span>{PANDAToken.symbol}</span>
          </div>
        </div>
        <div
          class="flex flex-row items-center justify-between text-sm font-medium"
        >
          <div class="flex flex-row items-center gap-2">
            <span>Prize service fee:</span>
          </div>
          <div class="flex flex-row gap-1">
            <span
              >{formatNumber(
                Number(prizeSubsidy[0]) / Number(PANDAToken.one)
              )}</span
            >
            <span>{PANDAToken.symbol}</span>
          </div>
        </div>
        {#if subsidy > 0}
          <div
            class="flex flex-row items-center justify-between text-sm font-medium text-orange-500"
          >
            <div class="flex flex-row items-center gap-2">
              <span>Subsidy from Lucky Pool:</span>
            </div>
            <div class="flex flex-row gap-1">
              <span>- {subsidy}</span>
              <span>{PANDAToken.symbol}</span>
            </div>
          </div>
        {/if}
        <div
          class="flex flex-row items-center justify-between text-sm font-medium"
        >
          <div class="flex flex-row items-center gap-2">
            <span>Final payment:</span>
          </div>
          <div class="flex flex-row gap-1 font-bold">
            <span>{formatNumber(Number(payment) / Number(PANDAToken.one))}</span
            >
            <span>{PANDAToken.symbol}</span>
          </div>
        </div>
        <button
          class="variant-filled-primary btn mt-6 w-full text-white"
          disabled={submitting ||
            !validating ||
            payment + PANDAToken.fee > pandaBalance}
          on:click={createPrizeHandler}
        >
          {#if submitting}
            <span class=""><IconCircleSpin /></span>
            <span>Processing...</span>
          {:else}
            <span>Create Now</span>
          {/if}
        </button>
      </footer>
    {/if}
  {/if}
</ModalCard>
