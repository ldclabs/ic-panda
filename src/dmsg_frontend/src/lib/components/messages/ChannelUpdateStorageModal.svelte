<script lang="ts">
  import { t, locale } from '$lib/i18n'
  import { autoFocus } from '$lib/actions/focus'
  import { type ChannelInfo } from '$lib/canisters/messagechannel'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { getBytesString, getShortNumber2 } from '$lib/utils/helper'
  import { unwrapOption } from '$src/lib/types/result'
  import { getToastStore } from '$lib/ui/stores'
  import { type SvelteComponent } from 'svelte'

  const toastStore = getToastStore()

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    myState: MyMessageState
    channel: ChannelInfo
    onCompleted: () => void
  }

  let { parent, myState, channel, onCompleted }: Props = $props()

  const fileMaxSize = Number(
    unwrapOption(channel.files_state)?.file_max_size || 0n
  )

  let maxSizeInput = $state(fileMaxSize || 1024 * 1024 * 10)
  let maxSizeErr = $state('')
  let validating = $state(fileMaxSize != 1024 * 1024 * 10)
  let submitting = $state(false)

  async function onTopup() {
    submitting = true
    toastRun(async (signal: AbortSignal) => {
      const file_max_size = BigInt(maxSizeInput)
      const api = myState.api.channelAPI(channel.canister)
      await api.update_storage({
        id: channel.id,
        file_max_size
      })

      onCompleted()
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
      input.setCustomValidity($t(maxSizeErr))
      return
    }

    if (maxSizeInput > 1024 * 1024 * 100) {
      maxSizeErr = 'Max file size should be less than 100MB'
      input.setCustomValidity($t(maxSizeErr))
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
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">{$t('Update storage')}</div>

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    oninput={onFormChange}
    use:autoFocus
  >
    <div class="relative">
      <input
        class="border-gray/10 input invalid:input-warning truncate rounded-xl bg-white/20 pr-20"
        type="number"
        name="maxSizeInput"
        min="0"
        step="any"
        bind:value={maxSizeInput}
        oninput={validateFileSize}
        disabled={submitting}
        placeholder={$t('Enter max file size')}
        data-focusindex="1"
        required
      />
      <div class="absolute top-2 right-2 text-neutral-500 outline-0"
        >{getBytesString(maxSizeInput, $locale)}</div
      >
      <p class="h-5 pl-3 text-sm {maxSizeErr ? 'text-error-500' : 'text-panda'}"
        >{$locale &&
          (maxSizeErr
            ? $t(maxSizeErr)
            : $t('Estimated gas: {amount}', {
                amount: getShortNumber2((maxSizeInput || 0) * 1000)
              }))}</p
      >
    </div>
    <hr class="!border-gray/20 mx-[-24px] !mt-4 !border-t-1 !border-dashed" />
    <div class="!mt-4 space-y-2 rounded-xl">
      <p class="">
        <b>1.</b>
        {$t(
          'Uploading files uses channel resources, costing 1000 gas per byte.'
        )}
      </p>
      <p class="">
        <b>2.</b>
        {$t(
          "If the channel's resource balance falls below 10M, file uploads will be temporarily disabled."
        )}
      </p>
    </div>
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || !validating || fileMaxSize == maxSizeInput}
      onclick={onTopup}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>{$t('Processing...')}</span>
      {:else}
        <span>{$t('Save')}</span>
      {/if}
    </button>
  </footer>
</ModalCard>
