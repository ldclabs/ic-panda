<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { type ProfileInfo } from '$lib/canisters/messageprofile'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextArea from '$lib/components/ui/TextAreaAutosize.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'
  import UsernameTransferModal from './UsernameTransferModal.svelte'

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    myState: MyMessageState
    myInfo: Readable<UserInfo & ProfileInfo>
    onSave: (info: UserInfo & ProfileInfo) => Promise<void>
  }

  let { parent, myState, myInfo, onSave }: Props = $props()

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let validating = $state(false)
  let submitting = $state(false)

  let nameInput = $state($myInfo.name || '')
  let descriptionInput = $state($myInfo.bio || '')

  function checkName() {
    return ''
  }

  function onSaveHandler() {
    submitting = true
    toastRun(async (signal: AbortSignal) => {
      await onSave({
        ...$myInfo,
        name: nameInput.trim(),
        bio: descriptionInput.trim()
      })

      modalStore.close()
    }, toastStore).finally(() => {
      submitting = false
      validating = false
    })
  }

  function onTransferUsernameHandler() {
    modalStore.close()
    modalStore.trigger({
      type: 'component',
      component: {
        ref: UsernameTransferModal,
        props: {
          myState
        }
      }
    })
  }

  function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    ;(form['nameInput'] as HTMLInputElement)?.setCustomValidity(checkName())

    validating = form.checkValidity()
  }
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Edit profile</div>

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    onchange={onFormChange}
  >
    <div class="relative">
      <div
        class="border-gray/10 flex flex-row justify-between rounded-xl border-[1px] px-3 py-2"
      >
        <p>
          <span class="text-neutral-500">{APP_ORIGIN + '/'}</span>
          <span>{$myInfo.username[0]}</span>
        </p>
        <button
          class="btn btn-sm p-0 text-neutral-500 hover:text-panda"
          onclick={onTransferUsernameHandler}>Transfer</button
        >
      </div>
    </div>
    <div class="relative mt-4">
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
        required
      />
    </div>
    <div class="relative mt-4">
      <TextArea
        bind:value={descriptionInput}
        minHeight="60"
        maxHeight="120"
        class="border-gray/10 textarea rounded-xl bg-white/20"
        name="descriptionInput"
        disabled={submitting}
        placeholder="User bio..."
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
        <span>Save</span>
      {/if}
    </button>
  </footer>
</ModalCard>
