<script lang="ts">
  import { goto } from '$app/navigation'
  import { type StateInfo, type UserInfo } from '$lib/canisters/message'
  import {
    TokenLedgerAPI,
    tokenLedgerAPIAsync
  } from '$lib/canisters/tokenledger'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextArea from '$lib/components/ui/TextAreaAutosize.svelte'
  import { MESSAGE_CANISTER_ID } from '$lib/constants'
  import { errMessage } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import {
    myMessageStateAsync,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { Principal } from '@dfinity/principal'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let add_managers: [Principal, Uint8Array | null][] = []

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const messageCanisterPrincipal = Principal.fromText(MESSAGE_CANISTER_ID)

  let tokenLedgerAPI: TokenLedgerAPI
  let myState: MyMessageState
  let stateInfo: Readable<StateInfo>
  let myUserInfo: Readable<UserInfo>

  let validating = false
  let submitting = false
  let availablePandaBalance = 0n

  let nameInput = ''
  let descriptionInput = ''
  let amount = 0n

  function checkInput() {
    nameInput = nameInput.trim()
    return ''
  }

  async function onCreateChannel(e: Event) {
    submitting = true

    try {
      if (amount > availablePandaBalance) {
        throw new Error('Insufficient balance')
      }

      const mk = await myState.masterKey()
      if (!mk || !mk.isOpened()) {
        throw new Error('Invalid master key')
      }

      const { dek, kek, managers } = await mk.generateChannelKey([
        [$myUserInfo.id, null],
        ...add_managers
      ])

      await tokenLedgerAPI.ensureAllowance(messageCanisterPrincipal, amount)
      const result = await myState.api.create_channel({
        dek,
        managers,
        name: nameInput.trim(),
        paid: amount,
        description: descriptionInput.trim(),
        created_by: $myUserInfo.id,
        image: ''
      })

      await myState.saveChannelKEK(result.canister, result.id, kek)
      await myState.addMyChannel(result)

      modalStore.close()
      goto(`/_/messages/${result.canister}/${result.id}`)
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
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    checkInput()
    validating = form.checkValidity()
  }

  onMount(async () => {
    myState = await myMessageStateAsync()
    stateInfo = myState.api.stateStore as Readable<StateInfo>
    myUserInfo = myState.info
    amount = $stateInfo.price.channel

    tokenLedgerAPI = await tokenLedgerAPIAsync()
    const pandaBalance = tokenLedgerAPI.balance()
    availablePandaBalance = await pandaBalance
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Create Channel</div>

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    on:input|preventDefault|stopPropagation|stopImmediatePropagation={onFormChange}
  >
    <div class="relative">
      <input
        class="input truncate rounded-xl border-gray/10 bg-white/20 invalid:input-warning hover:bg-white/90"
        type="text"
        name="nameInput"
        minlength="1"
        maxlength="32"
        bind:value={nameInput}
        disabled={submitting}
        placeholder="Channel name"
        required
      />
    </div>
    <div class="relative mt-4">
      <TextArea
        bind:value={descriptionInput}
        minHeight="60"
        maxHeight="120"
        class="rounded-xl border-gray/10 bg-white/20 ring-0 invalid:input-warning hover:bg-white/90"
        name="descriptionInput"
        placeholder="Channel description..."
      />
    </div>
    <hr class="!border-t-1 mx-[-24px] !mt-4 !border-dashed !border-gray/20" />
    <div class="!mt-4 space-y-2 rounded-xl bg-gray/5 p-4">
      <p class="text-gray/50">
        <b>1.</b> Username is optional. By registering a username, you will:
      </p>
      <p class="text-gray/50">
        <b>2.</b> Have your keys encrypted and stored on-chain, allowing sync across
        multiple devices (otherwise, the keys is stored only in the browser storage,
        and clearing browser data or device issues may result in key loss, making
        messages undecryptable).
      </p>
      <p class="text-gray/50">
        <b>3.</b> Get a personal profile page.
      </p>
      <p class="text-gray/50">
        <b>4.</b> Usernames cannot be changed, but can be transferred to another
        user in the future, allowing you to set a new username after the transfer.
      </p>
    </div>
    <div class="!mt-4 mb-2 text-sm">
      <div class="flex flex-row items-center justify-between">
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
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || !validating || amount > availablePandaBalance}
      on:click={onCreateChannel}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>Processing...</span>
      {:else}
        <span>Create Now</span>
      {/if}
    </button>
  </footer>
</ModalCard>
