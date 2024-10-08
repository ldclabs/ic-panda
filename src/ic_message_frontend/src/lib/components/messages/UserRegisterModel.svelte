<script lang="ts">
  import { type StateInfo, type UserInfo } from '$lib/canisters/message'
  import { tokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { MESSAGE_CANISTER_ID, APP_ORIGIN } from '$lib/constants'
  import { toastRun } from '$lib/stores/toast'
  import { unwrapOption } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { type MyMessageState } from '$lib/stores/message'
  import { Principal } from '@dfinity/principal'
  import {
    focusTrap,
    getToastStore,
    getModalStore
  } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onDestroy, onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import WalletDetailModal from '$lib/components/core/WalletDetailModal.svelte'

  const usernameReg = /^[a-z0-9][a-z0-9_]{0,19}$/i
  const toastStore = getToastStore()
  const modalStore = getModalStore()

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let onFinished: () => void = () => {}

  const myInfo: Readable<UserInfo | null> = myState.agent.subscribeUser()
  const messageState: Readable<StateInfo | null> = myState.api.stateStore
  const messageCanisterPrincipal = Principal.fromText(MESSAGE_CANISTER_ID)

  let validating = false
  let submitting = false
  let editMode = false
  let availablePandaBalance = 0n

  let username = ''
  let nameInput = ''
  let usernameInput = ''
  let usernameErr = ''
  let amount = 0n
  let existUsernames: string[] = []

  function checkName() {
    nameInput = nameInput.trim()
    return ''
  }

  function checkUsername() {
    amount = 0n
    usernameErr = ''
    usernameInput = usernameInput.trim()
    if (usernameInput && !usernameReg.test(usernameInput)) {
      usernameErr =
        'username must be 1-20 characters long and contain only letters, numbers, and underscores'
      return usernameErr
    }

    amount = getPrice(usernameInput)
    return ''
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

  async function onRegister() {
    submitting = true
    toastRun(async (signal: AbortSignal) => {
      if (amount > availablePandaBalance) {
        usernameErr = 'Insufficient balance'
        submitting = false
        validating = true
        return
      }

      if (!usernameInput) {
        await myState.api.update_my_name(nameInput)
      } else {
        await tokenLedgerAPI.ensureAllowance(messageCanisterPrincipal, amount)
        await myState.api.register_username(usernameInput, nameInput)
      }

      parent && parent['onClose']()
      await myState.agent.fetchUser()
      onFinished()
    }, toastStore).finally(() => {
      submitting = false
      validating = false
    })
  }

  function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    ;(form['nameInput'] as HTMLInputElement)?.setCustomValidity(checkName())
    ;(form['usernameInput'] as HTMLInputElement)?.setCustomValidity(
      checkUsername()
    )
    validating = form.checkValidity()
  }

  function getPrice(username: string): bigint {
    const price = $messageState?.price
    if (!username || !price) {
      return 0n
    }

    switch (username.length) {
      case 1:
        return price.name_l1
      case 2:
        return price.name_l2
      case 3:
      case 4:
        return price.name_l3
      case 5:
      case 6:
        return price.name_l5
      default:
        return price.name_l7
    }
  }

  const debouncedSearch = debounce(async () => {
    const name = usernameInput.trim()
    if (name && myState) {
      try {
        existUsernames = await myState.api.search_username(name)
      } catch (e) {
        // ignore
      }
    }
  }, 618)

  function onSearchUsername() {
    existUsernames.length = 0
    debouncedSearch()
  }

  onMount(() => {
    const { abort } = toastRun(async (signal: AbortSignal) => {
      if ($myInfo) {
        editMode = true
        nameInput = $myInfo.name || ''
        username = unwrapOption($myInfo.username) || ''
      }

      if (signal.aborted) {
        return
      }

      const pandaBalance = tokenLedgerAPI.balance()
      availablePandaBalance = await pandaBalance
    }, toastStore)

    return abort
  })

  onDestroy(() => {
    debouncedSearch.clear()
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold"
    >{editMode ? 'Edit' : 'Register'} Name</div
  >

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    on:input|preventDefault|stopPropagation={onFormChange}
    use:focusTrap={true}
  >
    <div class="relative">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning"
        type="text"
        name="nameInput"
        minlength="1"
        maxlength="32"
        data-1p-ignore
        bind:value={nameInput}
        disabled={submitting}
        placeholder="Display name"
        data-focusindex="1"
        required
      />
    </div>
    <hr class="!border-t-1 !border-gray/20 mx-[-24px] !mt-4 !border-dashed" />
    <div class="!mt-4 space-y-2 rounded-xl">
      <p class="">
        <b>1.</b> Username is optional. By registering a username, you will:
      </p>
      <p class="">
        <b>2.</b> Have your keys encrypted and stored on-chain, allowing sync
        across multiple devices.
        <span class="text-error-500"
          >Otherwise, the keys is stored only in the browser storage, and
          clearing browser data or device issues may result in key loss, making
          messages undecryptable.</span
        >
      </p>
      <p class="">
        <b>3.</b> Get a personal profile page.
      </p>
      <p class="">
        <b>4.</b> Usernames cannot be changed, but can be transferred to another
        user in the future, allowing you to set a new username after the transfer.
      </p>
    </div>
    <div class="!mt-4 mb-2 text-sm">
      <div class="flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-2 py-1">
          <span class="*:size-6"><IconPanda /></span>
          <b>Your Wallet Balance:</b>
        </div>
        <div class="flex flex-row gap-1 text-neutral-500">
          <span
            >{formatNumber(
              Number(availablePandaBalance) / Number(PANDAToken.one)
            )}</span
          >
          <span>{PANDAToken.symbol}</span>
        </div>
        {#if availablePandaBalance < amount}
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
    <div class="relative">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning"
        type="text"
        name="usernameInput"
        minlength="1"
        maxlength="20"
        data-1p-ignore
        bind:value={usernameInput}
        on:input={onSearchUsername}
        disabled={submitting || (editMode && username != '')}
        placeholder="{APP_ORIGIN}/{username || '[username]'}"
        data-focusindex="0"
      />
      <div class="absolute right-1 top-0 h-10 text-sm leading-10">
        {#if existUsernames.includes(usernameInput.trim())}
          <span class="text-error-500">occupied!</span>
        {:else}
          <span
            class={amount > availablePandaBalance
              ? 'text-error-500'
              : 'text-panda'}
            >{formatNumber(Number(amount) / Number(PANDAToken.one))}</span
          >
          <span>{PANDAToken.symbol}</span>
        {/if}
      </div>
      {#if !editMode}
        <p
          class="h-5 pl-3 text-sm text-error-500 {usernameErr == ''
            ? 'invisible'
            : 'visiable'}">{usernameErr}</p
        >
      {/if}
    </div>
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || !validating || amount > availablePandaBalance}
      on:click={onRegister}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>Processing...</span>
      {:else}
        <span>{editMode ? 'Save' : 'Register Now'}</span>
      {/if}
    </button>
  </footer>
</ModalCard>
