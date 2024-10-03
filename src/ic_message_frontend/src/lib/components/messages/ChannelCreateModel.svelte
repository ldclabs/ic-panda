<script lang="ts">
  import { goto } from '$app/navigation'
  import { type StateInfo, type UserInfo } from '$lib/canisters/message'
  import { tokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextArea from '$lib/components/ui/TextAreaAutosize.svelte'
  import { MESSAGE_CANISTER_ID } from '$lib/constants'
  import { toastRun } from '$lib/stores/toast'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { MyMessageState } from '$lib/stores/message'
  import { Principal } from '@dfinity/principal'
  import {
    focusTrap,
    getModalStore,
    getToastStore
  } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import WalletDetailModal from '$lib/components/core/WalletDetailModal.svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let channelName: string = ''
  export let add_managers: [Principal, Uint8Array | null][] = []

  const modalStore = getModalStore()
  const toastStore = getToastStore()
  const messageCanisterPrincipal = Principal.fromText(MESSAGE_CANISTER_ID)
  const stateInfo = myState.api.stateStore as Readable<StateInfo>
  const myInfo = myState.agent.subscribeUser() as Readable<UserInfo>

  let nameInput = channelName
  let descriptionInput = ''

  let validating = nameInput.trim() !== ''
  let submitting = false
  let availablePandaBalance = 0n

  function checkInput() {
    const name = nameInput.trim()
    if (!name) {
      nameInput = ''
    }
    return ''
  }

  function onCreateChannel(e: Event) {
    submitting = true
    toastRun(async () => {
      if (channelPrice > availablePandaBalance) {
        throw new Error('Insufficient balance')
      }

      const name = nameInput.trim()
      if (!name) {
        throw new Error('Invalid channel name')
      }

      const mk = await myState.mustMasterKey()
      const { dek, kek, managers } = await mk.generateChannelKey([
        [$myInfo.id, null],
        ...add_managers
      ])

      if (channelPrice > 0n) {
        await tokenLedgerAPI.ensureAllowance(
          messageCanisterPrincipal,
          channelPrice
        )
      }
      const result = await myState.api.create_channel({
        dek,
        managers,
        name: nameInput.trim(),
        paid: channelPrice,
        description: descriptionInput.trim(),
        created_by: $myInfo.id,
        image: ''
      })

      await myState.saveChannelKEK(result.canister, result.id, kek)
      await myState.agent.setChannel(result)

      modalStore.close()
      goto(`/_/messages/${result.canister}/${result.id}`)
    }, toastStore).finally(() => {
      submitting = false
      validating = false
    })
  }

  function onOpenWallet() {
    modalStore.close()
    modalStore.trigger({
      type: 'component',
      component: {
        ref: WalletDetailModal,
        props: {}
      }
    })
  }

  function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    checkInput()
    validating = form.checkValidity()
  }

  onMount(() => {
    const { abort } = toastRun(async (signal: AbortSignal) => {
      const pandaBalance = tokenLedgerAPI.balance()
      availablePandaBalance = await pandaBalance
    }, toastStore)

    return abort
  })

  $: channelPrice = $stateInfo ? $stateInfo.price.channel : 100_000_000_000n
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Create a channel</div>

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    on:input|preventDefault|stopPropagation={onFormChange}
    use:focusTrap={true}
  >
    <div class="relative">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning hover:bg-white/90"
        type="text"
        name="nameInput"
        minlength="1"
        maxlength="32"
        data-1p-ignore
        bind:value={nameInput}
        disabled={submitting}
        placeholder="Channel name"
        data-focusindex="0"
        required
      />
    </div>
    <div class="relative mt-4">
      <TextArea
        bind:value={descriptionInput}
        minHeight="60"
        maxHeight="120"
        class="border-gray/10 textarea rounded-xl bg-white/20 hover:bg-white/90"
        name="descriptionInput"
        placeholder="Channel description (not encrypted)..."
        data-focusindex="1"
      />
    </div>
    <hr class="!border-t-1 !border-gray/20 mx-[-24px] !mt-4 !border-dashed" />
    <div class="!mt-4 space-y-2 rounded-xl bg-gray-500/5 p-4">
      <p class="text-neutral-600">
        <b>1.</b> Creating a message channel costs
        <span class="text-panda"
          >{formatNumber(Number(channelPrice) / Number(PANDAToken.one))}</span
        > PANDA tokens for gas; sending messages will consume gas.
      </p>
      <p class="text-neutral-600">
        <b>2.</b> A channel can have up to 5 managers and 100 members.
      </p>
      <p class="text-neutral-600">
        <b>3.</b> Managers can only remove regular members, not other managers. If
        the last manager leaves, the channel and all messages will be deleted.
      </p>
    </div>
    <div class="!mt-4 mb-2 text-sm">
      <div class="flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-2 py-1">
          <span class="*:size-6"><IconPanda /></span>
          <b>Your Wallet Balance:</b>
        </div>
        <div class="flex flex-row gap-1 text-neutral-600">
          <span
            >{formatNumber(
              Number(availablePandaBalance) / Number(PANDAToken.one)
            )}</span
          >
          <span>{PANDAToken.symbol}</span>
        </div>
        {#if availablePandaBalance < channelPrice}
          <button
            type="button"
            class="btn btn-sm hover:variant-soft-primary"
            on:click={onOpenWallet}
          >
            <span class="*:size-4"><IconAdd /></span>
            <span>Topup</span>
          </button>
        {/if}
      </div>
    </div>
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting ||
        !validating ||
        channelPrice > availablePandaBalance}
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
