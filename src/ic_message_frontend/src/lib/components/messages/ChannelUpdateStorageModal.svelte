<script lang="ts">
  import { type ChannelInfo } from '$lib/canisters/messagechannel'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { getBytesString } from '$lib/utils/helper'
  import { PANDAToken, TokenDisplay } from '$lib/utils/token'
  import { unwrapOption } from '$src/lib/types/result'
  import { focusTrap, getToastStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  const toastStore = getToastStore()

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let channel: ChannelInfo
  export let onFinished: () => void

  const fileMaxSize = Number(
    unwrapOption(channel.files_state)?.file_max_size || 0n
  )

  let maxSizeInput = fileMaxSize || 1024 * 1024 * 10
  let maxSizeErr = ''
  let validating = maxSizeInput != fileMaxSize
  let submitting = false

  async function onTopup() {
    submitting = true
    toastRun(async (signal: AbortSignal) => {
      const file_max_size = BigInt(maxSizeInput)
      const api = myState.api.channelAPI(channel.canister)
      await api.update_storage({
        id: channel.id,
        file_max_size
      })

      onFinished()
      parent && parent['onClose']()
    }, toastStore).finally(() => {
      submitting = false
      validating = false
    })
  }

  function validateFileSize(e: Event) {
    const input = e.target as HTMLInputElement
    if (!Number.isSafeInteger(maxSizeInput) || maxSizeInput < 0) {
      maxSizeErr = 'Invalid max file size, should be a positive integer'
      input.setCustomValidity(maxSizeErr)
      return
    }

    if (maxSizeInput > 1024 * 1024 * 100) {
      maxSizeErr = 'Max file size should be less than 100MB'
      input.setCustomValidity(maxSizeErr)
      return
    }

    maxSizeErr = ''
    if (input.value.startsWith('0')) {
      input.value = maxSizeInput.toString()
    }

    input.setCustomValidity('')
  }

  function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    validating = form.checkValidity()
  }

  $: tokenDisplay = TokenDisplay.fromNumber(
    PANDAToken,
    (maxSizeInput || 0) * 1000,
    false
  )
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Update Storage</div>

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    on:input|preventDefault|stopPropagation={onFormChange}
    use:focusTrap={true}
  >
    <div class="relative">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 pr-20 invalid:input-warning"
        type="number"
        name="maxSizeInput"
        min="0"
        step="any"
        bind:value={maxSizeInput}
        on:input={validateFileSize}
        disabled={submitting}
        placeholder="Enter max file size"
        data-focusindex="1"
        required
      />
      <div class="absolute right-2 top-2 text-neutral-500 outline-0"
        >{getBytesString(maxSizeInput)}</div
      >
      <p class="h-5 pl-3 text-sm {maxSizeErr ? 'text-error-500' : 'text-panda'}"
        >{maxSizeErr
          ? maxSizeErr
          : 'Up to consume' + tokenDisplay.display() + ' Gas'}</p
      >
    </div>
    <hr class="!border-t-1 !border-gray/20 mx-[-24px] !mt-4 !border-dashed" />
    <div class="!mt-4 space-y-2 rounded-xl">
      <p class="">
        <b>1.</b> Uploading files consumes gas from the channel, at a rate of 1000
        gas per byte.
      </p>
      <p class="">
        <b>2.</b> When the channel's gas balance drops below 10,000,000, file uploads
        will be disabled.
      </p>
    </div>
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || !validating || fileMaxSize == maxSizeInput}
      on:click={onTopup}
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
