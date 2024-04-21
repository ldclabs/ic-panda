<script lang="ts">
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type NameOutput
  } from '$lib/canisters/luckypool'
  import {
    TokenLedgerAPI,
    tokenLedgerAPIAsync
  } from '$lib/canisters/tokenledger'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { LUCKYPOOL_CANISTER_ID } from '$lib/constants'
  import { errMessage } from '$lib/types/result'
  import { formatNumber, PANDAToken } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let nameEditMode: 0 | 1 | 2 = 0 // set, update, release
  export let availablePandaBalance = 0n
  export let nameState: Readable<NameOutput | null>

  const NamingDeposit = 3000n * PANDAToken.one

  // const modalStore = getModalStore()
  const toastStore = getToastStore()
  const luckyPoolPrincipal = Principal.fromText(LUCKYPOOL_CANISTER_ID)

  let validating = false
  let submitting = false

  let nameInput = $nameState?.name || ''
  let nameErr = ''
  let result: NameOutput | null = null
  let refund: bigint | null = null

  let luckyPoolAPI: LuckyPoolAPI
  let tokenLedgerAPI: TokenLedgerAPI

  function checkName() {
    nameErr = ''
    nameInput = nameInput.trim()

    if (
      nameInput == '' ||
      nameInput.includes('\r') ||
      nameInput.includes('\n') ||
      nameInput.includes('\t')
    ) {
      return 'Enter an name without line break'
    }

    if (nameEditMode == 1 && nameInput == $nameState?.name) {
      return 'The name is the same as the current one'
    }

    return ''
  }

  async function nameCopyPaste(e: Event) {
    e.preventDefault()

    if (nameInput == '') {
      nameInput = await navigator.clipboard.readText()
    } else {
      nameInput = ''
    }
    checkName()
  }

  async function onRegister(e: Event) {
    e.preventDefault()

    submitting = true

    try {
      const res = await luckyPoolAPI.nameLookup(nameInput)
      if (res) {
        nameErr = 'Try another one, this name is occupied'
        submitting = false
        validating = true
        return
      }

      await tokenLedgerAPI.ensureAllowance(
        luckyPoolPrincipal,
        NamingDeposit + PANDAToken.fee
      )
      result = await luckyPoolAPI.registerName(nameInput)
    } catch (err: any) {
      submitting = false
      validating = false
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  async function onUpdate(e: Event) {
    e.preventDefault()

    submitting = true

    try {
      if (
        nameInput.toLocaleLowerCase() != $nameState?.name?.toLocaleLowerCase()
      ) {
        const res = await luckyPoolAPI.nameLookup(nameInput)
        if (res) {
          nameErr = 'Try another one, this name is occupied'
          submitting = false
          validating = true
          return
        }
      }

      result = await luckyPoolAPI.updateName(nameInput, $nameState?.name || '')
    } catch (err: any) {
      submitting = false
      validating = false
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  async function onUnregister(e: Event) {
    e.preventDefault()

    submitting = true

    try {
      refund = await luckyPoolAPI.unregisterName($nameState?.name || '')
    } catch (err: any) {
      submitting = false
      validating = false
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  function onFormChange(e: Event) {
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    const input = form['pandaName'] as HTMLInputElement
    input?.setCustomValidity(checkName())
    validating = form.checkValidity()
  }

  onMount(async () => {
    tokenLedgerAPI = await tokenLedgerAPIAsync()
    luckyPoolAPI = await luckyPoolAPIAsync()
  })
</script>

<ModalCard {parent}>
  {#if result}
    <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
      <IconCheckbox />
    </div>
    <div class="text-center">
      <p class="mt-4">
        <span>
          You have successfully {nameEditMode == 0 ? 'registered' : 'updated'} a
          name:
        </span>
      </p>
      <p class="my-2 text-center text-lg font-bold">{result.name}</p>
      <p class="text-left">You can update the name for free at any time.</p>
    </div>
  {:else if refund !== null}
    <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
      <IconCheckbox />
    </div>
    <div class="text-center">
      <p class="mt-4">
        <span>You have successfully unregistered the name:</span>
      </p>
      <p class="my-2 text-center text-lg font-bold">{nameInput}</p>
      <p class="">
        <span class="font-bold">
          {formatNumber(Number(refund) / Number(PANDAToken.one))}
        </span>
        <span>tokens refunded, check your <b>Wallet Balance</b>.</span>
      </p>
    </div>
  {:else if nameEditMode == 0}
    <div class="!mt-0 text-center text-xl font-bold">Register Name</div>
    <div class="space-y-2 rounded-xl bg-gray/5 p-4">
      <p class="text-gray/50">
        <b>1.</b> To register a name, pay a
        <b
          >{formatNumber(Number(NamingDeposit) / Number(PANDAToken.one))} PANDA tokens
          deposit</b
        >. An
        <b
          >annual fee of {formatNumber(
            Number(NamingDeposit / 10n) / Number(PANDAToken.one)
          )} tokens</b
        >
        will be deducted from this deposit. After <b>10 years</b>, the name is
        yours permanently.
      </p>
      <p class="text-gray/50">
        <b>2.</b> You can update the name for free at any time.
      </p>
      <p class="text-gray/50">
        <b>3.</b> If you unregister the name early, the <b>remaining deposit</b>
        after fee deductions will be refunded to your lucky balance.
      </p>
    </div>
    <hr class="!border-t-1 mx-[-24px] !mt-6 !border-dashed !border-gray/20" />
    <div class="!mt-5 text-sm">
      <div class="mt-1 flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-2">
          <span class="*:size-6"><IconPanda /></span>
          <b>Your Wallet Balance:</b>
        </div>
        <div class="flex flex-row gap-1 text-gray/50">
          <span
            >{formatNumber(
              Number(availablePandaBalance) / Number(PANDAToken.one)
            )}</span
          >
          <span>{PANDAToken.symbol}</span>
        </div>
      </div>
    </div>
    <form
      class="m-auto !mt-4 flex flex-col content-center"
      on:input={onFormChange}
    >
      <div class="relative">
        <input
          class="input truncate rounded-xl border-gray/10 bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
          type="text"
          name="nameInput"
          minlength="2"
          maxlength="48"
          bind:value={nameInput}
          disabled={submitting ||
            availablePandaBalance < 1000n * PANDAToken.one}
          placeholder="Enter an name without line break"
          required
        />
        <button
          class="btn absolute right-0 top-0 outline-0"
          disabled={submitting}
          on:click={nameCopyPaste}
        >
          {#if nameInput == ''}
            <span>Paste</span>
          {:else}
            <span class="*:scale-90"><IconDeleteBin /></span>
          {/if}
        </button>
        <p
          class="h-5 pl-3 text-sm text-error-500 {nameErr == ''
            ? 'invisible'
            : 'visiable'}">{nameErr}</p
        >
      </div>
    </form>
    <footer class="m-auto !mt-2">
      <button
        class="variant-filled-primary btn w-full text-white"
        disabled={submitting || !validating}
        on:click={onRegister}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Register Now</span>
        {/if}
      </button>
    </footer>
  {:else if nameEditMode == 1}
    <div class="!mt-0 text-center text-xl font-bold">Update Name</div>
    <div class="space-y-2 rounded-xl bg-gray/5 p-4">
      <p class="mt-4 text-gray/50">
        <span>You are updating the name:</span>
      </p>
      <p class="my-2 text-center text-lg font-bold">{$nameState?.name || ''}</p>
      <p class="text-gray/50">You can update it for free at any time.</p>
    </div>
    <hr class="!border-t-1 mx-[-24px] !mt-6 !border-dashed !border-gray/20" />
    <form
      class="m-auto !mt-6 flex flex-col content-center"
      on:input={onFormChange}
    >
      <div class="relative">
        <input
          class="input truncate rounded-xl border-gray/10 bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
          type="text"
          name="nameInput"
          minlength="2"
          maxlength="48"
          bind:value={nameInput}
          disabled={submitting}
          placeholder="Enter an name without line break"
          required
        />
        <button
          class="btn absolute right-0 top-0 outline-0"
          disabled={submitting}
          on:click={nameCopyPaste}
        >
          {#if nameInput == ''}
            <span>Paste</span>
          {:else}
            <span class="*:scale-90"><IconDeleteBin /></span>
          {/if}
        </button>
        <p
          class="h-5 pl-3 text-sm text-error-500 {nameErr == ''
            ? 'invisible'
            : 'visiable'}">{nameErr}</p
        >
      </div>
    </form>
    <footer class="m-auto !mt-2">
      <button
        class="variant-filled-primary btn w-full text-white"
        disabled={submitting || !validating}
        on:click={onUpdate}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Update Now</span>
        {/if}
      </button>
    </footer>
  {:else if nameEditMode == 2}
    <div class="!mt-0 text-center text-xl font-bold">Unregister Name</div>
    <div class="space-y-2 rounded-xl bg-gray/5 p-4">
      <p class="mt-4 text-gray/50">
        <span>You are unregistering the name:</span>
      </p>
      <p class="my-2 text-center text-lg font-bold">{$nameState?.name || ''}</p>
      <p class="text-gray/50">
        <b>1.</b> If you unregister the name early, the <b>remaining deposit</b>
        after fee deductions will be refunded to your lucky balance.
      </p>
      <p class="text-gray/50">
        <b>2.</b> To register a name again, pay a
        <b
          >{formatNumber(Number(NamingDeposit) / Number(PANDAToken.one))} PANDA tokens
          deposit</b
        >. An
        <b
          >annual fee of {formatNumber(
            Number(NamingDeposit / 10n) / Number(PANDAToken.one)
          )} tokens</b
        >
        will be deducted from this deposit. After <b>10 years</b>, the name is
        yours permanently.
      </p>
    </div>
    <hr class="!border-t-1 mx-[-24px] !mt-6 !border-dashed !border-gray/20" />
    <footer class="m-auto !mt-6">
      <button
        class="variant-filled-warning btn w-full text-white"
        disabled={submitting}
        on:click={onUnregister}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Unregister It</span>
        {/if}
      </button>
    </footer>
  {/if}
</ModalCard>
