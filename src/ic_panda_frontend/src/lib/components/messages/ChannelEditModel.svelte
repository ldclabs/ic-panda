<script lang="ts">
  import {
    type ChannelInfo,
    type UpdateChannelInput
  } from '$lib/canisters/messagechannel'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextArea from '$lib/components/ui/TextAreaAutosize.svelte'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let channel: ChannelInfo
  export let onSave: (input: UpdateChannelInput) => Promise<void>

  const modalStore = getModalStore()

  let validating = false
  let submitting = false

  let nameInput = channel.name
  let descriptionInput = channel.description

  function checkInput() {
    const name = nameInput.trim()
    if (!name) {
      nameInput = ''
    }
    return ''
  }

  function onClickSave(e: Event) {
    submitting = true
    const input: UpdateChannelInput = {
      id: channel.id,
      name: [],
      description: [],
      image: []
    }
    const name = nameInput.trim()
    if (name && name !== channel.name) {
      input.name = [name]
    }
    const desc = descriptionInput.trim()
    if (desc && desc !== channel.description) {
      input.description = [desc]
    }

    if (input.name.length || input.description.length) {
      onSave(input)
        .then(() => {
          modalStore.close()
        })
        .catch((e) => {
          console.error(e)
          submitting = false
        })
    } else {
      modalStore.close()
    }
  }

  function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    checkInput()
    validating = form.checkValidity()
  }
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Edit channel</div>

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    on:input|preventDefault|stopPropagation={onFormChange}
  >
    <div class="relative">
      <input
        class="input truncate rounded-xl border-gray/10 bg-white/20 invalid:input-warning hover:bg-white/90"
        type="text"
        name="nameInput"
        minlength="1"
        maxlength="32"
        data-1p-ignore
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
        class="textarea rounded-xl border-gray/10 bg-white/20 hover:bg-white/90"
        name="descriptionInput"
        placeholder="Channel description..."
      />
    </div>
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || !validating}
      on:click={onClickSave}
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
