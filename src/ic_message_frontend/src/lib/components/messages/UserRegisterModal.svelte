<script lang="ts">
  import { type StateInfo, type UserInfo } from '$lib/canisters/message'
  import { pandaLedgerAPI } from '$lib/canisters/tokenledger'
  import WalletDetailModal from '$lib/components/core/WalletDetailModal.svelte'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { APP_ORIGIN, MESSAGE_CANISTER_ID } from '$lib/constants'
  import { getTokenPrice } from '$lib/stores/exchange'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { unwrapOption } from '$lib/types/result'
  import { getPriceNumber } from '$lib/utils/helper'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { authStore } from '$src/lib/stores/auth'
  import { Principal } from '@dfinity/principal'
  import {
    focusTrap,
    getModalStore,
    getToastStore
  } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onDestroy, onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'

  const usernameReg = /^[a-z0-9][a-z0-9_]{0,19}$/i
  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const usernameAccount = authStore.identity?.username || ''

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    myState: MyMessageState
    onFinished?: () => void
  }

  let { parent, myState, onFinished = () => {} }: Props = $props()

  const myInfo: Readable<UserInfo | null> = myState.agent.subscribeUser()
  const messageState: Readable<StateInfo | null> = myState.api.stateStore
  const messageCanisterPrincipal = Principal.fromText(MESSAGE_CANISTER_ID)
  const pandaPrice = getTokenPrice(PANDAToken.canisterId)

  let validating = $state(false)
  let submitting = $state(false)
  let editMode = $state(false)
  let availablePandaBalance = $state(0n)

  let username = $state('')
  let nameInput = $state('')
  let usernameInput = $state('')
  let usernameErr = $state('')
  let amount = $state(0n)
  let existUsernames: string[] = $state([])

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
        await pandaLedgerAPI.ensureAllowance(messageCanisterPrincipal, amount)
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

      const pandaBalance = pandaLedgerAPI.balance()
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
    >{editMode ? 'Edit' : 'Register'} name</div
  >

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    oninput={onFormChange}
    use:focusTrap={true}
  >
    <label class="label relative">
      <span>Display Name (Required)</span>
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
    </label>
    <hr class="!border-t-1 !border-gray/20 mx-[-24px] !mt-6 !border-dashed" />
    <label class="label relative mt-4">
      <div class="flex flex-row items-center justify-between">
        <span>Username</span>
        <div class="text-sm">
          {#if existUsernames.includes(usernameInput.trim())}
            <span class="text-error-500">occupied!</span>
          {:else}
            <span
              class={amount > availablePandaBalance
                ? 'text-error-500'
                : 'text-panda'}
              >{formatNumber(Number(amount) / Number(PANDAToken.one)) +
                ($pandaPrice && amount > 0n
                  ? ' ($' +
                    getPriceNumber(
                      $pandaPrice.priceUSD *
                        (Number(amount) / Number(PANDAToken.one))
                    ) +
                    ')'
                  : '')}</span
            >
            <span>{PANDAToken.symbol}</span>
          {/if}
        </div>
      </div>
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning"
        type="text"
        name="usernameInput"
        minlength="1"
        maxlength="20"
        data-1p-ignore
        bind:value={usernameInput}
        oninput={onSearchUsername}
        disabled={!!usernameAccount ||
          submitting ||
          (editMode && username != '')}
        placeholder="{APP_ORIGIN}/{username || '[username]'}"
        data-focusindex="0"
      />
    </label>
    {#if usernameErr}
      <div class="h-10 text-sm text-error-500">{usernameErr}</div>
    {:else}
      <div class="flex h-10 flex-row items-center justify-between text-sm">
        <div class="flex flex-row items-center gap-2 py-1">
          <span class="*:size-6"><IconPanda /></span>
          <span>Your Wallet Balance:</span>
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
            onclick={onOpenWallet}
          >
            <span class="*:size-4"><IconAdd /></span>
            <span>Topup</span>
          </button>
        {/if}
      </div>
    {/if}

    {#if usernameAccount}
      <div class="mt-2 space-y-1">
        <p class="">
          This is a <b>Username Permanent Account</b>, please transfer the
          username
          <span class="font-semibold text-primary-500">{usernameAccount}</span> to
          this account.
        </p>
      </div>
    {:else}
      <div class="mt-2 space-y-1">
        <p class="">
          <b>1.</b> Username is optional. By registering a username, you will:
        </p>
        <p class="">
          <b>2.</b> Have your keys encrypted and stored on-chain, allowing sync
          across multiple devices.
          <span class="text-error-500"
            >Otherwise, the keys is stored only in the browser storage, and
            clearing browser data or device issues may result in key loss,
            making messages undecryptable.</span
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
    {/if}
  </form>
  <footer class="m-auto !mt-4">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || !validating || amount > availablePandaBalance}
      onclick={onRegister}
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
