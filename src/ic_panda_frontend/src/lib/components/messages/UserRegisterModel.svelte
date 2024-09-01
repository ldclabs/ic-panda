<script lang="ts">
  import { type StateInfo, type UserInfo } from '$lib/canisters/message'
  import {
    TokenLedgerAPI,
    tokenLedgerAPIAsync
  } from '$lib/canisters/tokenledger'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { MESSAGE_CANISTER_ID } from '$lib/constants'
  import { myMessageStateAsync, type MyMessageState } from '$lib/stores/user'
  import { errMessage } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'

  const usernameReg = /^[a-z0-9][a-z0-9_]{0,19}$/i

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let availablePandaBalance = 0n

  // const modalStore = getModalStore()
  const toastStore = getToastStore()
  const messageCanisterPrincipal = Principal.fromText(MESSAGE_CANISTER_ID)

  let tokenLedgerAPI: TokenLedgerAPI
  let myMessageState: MyMessageState

  let messageState: Readable<StateInfo | null>
  let userInfo: Readable<UserInfo | null>

  let validating = false
  let submitting = false

  let nameInput = ''
  let usernameInput = ''
  let usernameErr = ''
  let amount = 0n
  let result: UserInfo | null = null

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

  async function nameCopyPaste(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    if (nameInput == '') {
      nameInput = await navigator.clipboard.readText()
    } else {
      nameInput = ''
    }
    checkName()
  }

  async function usernameCopyPaste(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    if (usernameInput == '') {
      usernameInput = await navigator.clipboard.readText()
    } else {
      usernameInput = ''
    }
    checkUsername()
  }

  async function onRegister(e: Event) {
    submitting = true
    if (amount > availablePandaBalance) {
      usernameErr = 'Insufficient balance'
      submitting = false
      validating = true
      return
    }

    try {
      if (!usernameInput) {
        result = await myMessageState.api.update_my_name(nameInput)
      } else {
        try {
          const res = await myMessageState.api.get_by_username(usernameInput)
          if (res) {
            usernameErr = 'Try another one, this username is occupied'
            submitting = false
            validating = true
            return
          }
        } catch (e) {
          // ignore
        }

        await tokenLedgerAPI.ensureAllowance(messageCanisterPrincipal, amount)
        result = await myMessageState.api.register_username(
          usernameInput,
          nameInput
        )
      }
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

  onMount(async () => {
    myMessageState = await myMessageStateAsync()
    messageState = myMessageState.api.stateStore
    userInfo = myMessageState.info
    nameInput = $userInfo?.name || ''
    result = $userInfo

    tokenLedgerAPI = await tokenLedgerAPIAsync()
    const pandaBalance = tokenLedgerAPI.balance()
    availablePandaBalance = await pandaBalance
  })
</script>

<ModalCard {parent}>
  {#if result}
    <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
      <IconCheckbox />
    </div>
    <div class="text-center">
      <p class="mt-4">
        <span> You have successfully registered the name: </span>
      </p>
      <p class="my-2 text-center text-lg font-bold"
        >{result.username.length == 1
          ? result.name + ' (' + result.username[0] + ')'
          : result.name}</p
      >
    </div>
  {:else}
    <div class="!mt-0 text-center text-xl font-bold">Register Name</div>

    <form
      class="m-auto !mt-4 flex flex-col content-center"
      on:input|preventDefault|stopPropagation|stopImmediatePropagation={onFormChange}
    >
      <div class="relative">
        <input
          class="input truncate rounded-xl border-gray/10 bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
          type="text"
          name="nameInput"
          minlength="1"
          maxlength="32"
          bind:value={nameInput}
          disabled={submitting}
          placeholder="Display name"
          required
        />
        <a
          class="btn absolute right-0 top-0 outline-0"
          type="button"
          href="/"
          on:click={nameCopyPaste}
        >
          {#if nameInput == ''}
            <span>Paste</span>
          {:else}
            <span class="*:scale-90"><IconDeleteBin /></span>
          {/if}
        </a>
      </div>
      <hr class="!border-t-1 mx-[-24px] !mt-4 !border-dashed !border-gray/20" />
      <div class="!mt-4 space-y-2 rounded-xl bg-gray/5 p-4">
        <p class="text-gray/50">
          <b>1.</b> Username is optional. By registering a username, you will:
        </p>
        <p class="text-gray/50">
          <b>2.</b> Have your keys encrypted and stored on-chain, allowing sync across
          multiple devices (otherwise, the keys is stored only in the browser database,
          and clearing browser data or device issues may result in key loss, making
          messages undecryptable);
        </p>
        <p class="text-gray/50">
          <b>3.</b> Get a personal profile page;
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
      <div class="relative">
        <input
          class="input truncate rounded-xl border-gray/10 bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
          type="text"
          name="usernameInput"
          minlength="1"
          maxlength="20"
          bind:value={usernameInput}
          disabled={submitting}
          placeholder="https://panda.fans/[username]"
        />
        <a
          class="btn absolute right-0 top-0 outline-0"
          type="button"
          href="/"
          on:click={usernameCopyPaste}
        >
          {#if usernameInput == ''}
            <span>Paste</span>
          {:else}
            <span class="*:scale-90"><IconDeleteBin /></span>
          {/if}
        </a>
        <p
          class="h-5 pl-3 text-sm text-error-500 {usernameErr == ''
            ? 'invisible'
            : 'visiable'}">{usernameErr}</p
        >
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
          <span>Register Now</span>
        {/if}
      </button>
    </footer>
  {/if}
</ModalCard>
