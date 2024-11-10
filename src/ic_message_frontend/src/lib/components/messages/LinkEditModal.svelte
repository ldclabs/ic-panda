<script lang="ts">
  import { type Link } from '$lib/canisters/messageprofile'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { toastRun } from '$lib/stores/toast'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    link?: Link | null
    onSave: (link: Link) => Promise<void>
  }

  let { parent, link = null, onSave }: Props = $props()

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let submitting = $state(false)
  let uriInput = $state(link ? link.uri : '')
  let titleInput = $state(link ? link.title : '')
  let validating = $state(false)

  $effect(() => {
    validating =
      uriInput.length > 0 &&
      uriInput.trim() == uriInput &&
      titleInput.length > 0 &&
      titleInput.trim() == titleInput
  })

  function onSaveHandler() {
    submitting = true
    toastRun(async (signal: AbortSignal) => {
      await onSave({
        uri: uriInput.trim(),
        title: titleInput.trim(),
        image: []
      })

      modalStore.close()
    }, toastStore).finally(() => {})
  }
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold"
    >{link ? 'Update link' : 'Add link'}</div
  >
  <form class="m-auto !mt-4 flex flex-col content-center">
    <div class="relative mt-4">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning"
        type="text"
        name="titleInput"
        minlength="1"
        maxlength="120"
        data-1p-ignore
        bind:value={titleInput}
        disabled={submitting}
        placeholder="Enter title"
        required
      />
    </div>
    <div class="relative mt-4">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning"
        type="text"
        name="uriInput"
        minlength="1"
        maxlength="120"
        data-1p-ignore
        bind:value={uriInput}
        disabled={submitting}
        placeholder="Enter uri"
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
        <span>Save</span>
      {/if}
    </button>
  </footer>
</ModalCard>
