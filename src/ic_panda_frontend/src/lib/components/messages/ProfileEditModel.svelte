<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { type ProfileInfo } from '$lib/canisters/messageprofile'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextArea from '$lib/components/ui/TextAreaAutosize.svelte'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'
  import UsernameTransferModel from './UsernameTransferModel.svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let myInfo: Readable<UserInfo & ProfileInfo>
  export let onSave: (info: UserInfo & ProfileInfo) => Promise<void>

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let validating = false
  let submitting = false

  let nameInput = $myInfo.name || ''
  let descriptionInput = $myInfo.bio || ''

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
        ref: UsernameTransferModel,
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
    on:input|preventDefault|stopPropagation|stopImmediatePropagation={onFormChange}
  >
    <div class="relative">
      <div
        class="flex flex-row justify-between rounded-xl border-[1px] border-gray/10 px-3 py-2"
      >
        <p>
          <span class="text-gray/60">https://panda.fans/</span>
          <span>{$myInfo.username[0]}</span>
        </p>
        <button
          class="btn btn-sm p-0 text-gray/60 hover:text-panda"
          on:click={onTransferUsernameHandler}>Transfer</button
        >
      </div>
    </div>
    <div class="relative mt-4">
      <input
        class="input truncate rounded-xl border-gray/10 bg-white/20 invalid:input-warning hover:bg-white/90"
        type="text"
        name="nameInput"
        minlength="1"
        maxlength="32"
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
        class="textarea rounded-xl border-gray/10 bg-white/20 hover:bg-white/90"
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
      on:click={onSaveHandler}
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
