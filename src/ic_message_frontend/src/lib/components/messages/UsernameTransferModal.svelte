<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { Principal } from '@dfinity/principal'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    myState: MyMessageState
  }

  let { parent, myState }: Props = $props()

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const myInfo = myState.agent.subscribeUser() as Readable<UserInfo>

  let validating = $state(false)
  let submitting = $state(false)

  let usernameInput = $state('')
  let toInput = $state('')

  function checkUsername() {
    if (usernameInput.trim() !== $myInfo.username[0]) {
      return 'Username does not match'
    }
    return ''
  }

  function checkPrincipal() {
    try {
      Principal.fromText(toInput)
    } catch (_err) {
      return 'Invalid principal'
    }
    return ''
  }

  function onSaveHandler() {
    submitting = true
    toastRun(async (signal: AbortSignal) => {
      const to = Principal.fromText(toInput)
      await myState.api.transfer_username(to)
      await myState.agent.fetchUser()
      modalStore.close()
    }, toastStore).finally(() => {
      modalStore.close()
      submitting = false
      validating = false
    })
  }

  function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    ;(form['usernameInput'] as HTMLInputElement)?.setCustomValidity(
      checkUsername()
    )
    ;(form['toInput'] as HTMLInputElement)?.setCustomValidity(checkPrincipal())

    validating = form.checkValidity()
  }
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Transfer username</div>

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    oninput={onFormChange}
  >
    <div class="relative mt-4">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning"
        type="text"
        name="usernameInput"
        minlength="1"
        maxlength="20"
        data-1p-ignore
        bind:value={usernameInput}
        disabled={submitting}
        placeholder="Enter username"
        required
      />
    </div>
    <div class="relative mt-4">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning"
        type="text"
        name="toInput"
        minlength="27"
        maxlength="63"
        data-1p-ignore
        bind:value={toInput}
        disabled={submitting}
        placeholder="Enter principal to receive username"
        required
      />
    </div>
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || !validating}
      onclick={onSaveHandler}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>Processing...</span>
      {:else}
        <span>Transfer</span>
      {/if}
    </button>
  </footer>
</ModalCard>
