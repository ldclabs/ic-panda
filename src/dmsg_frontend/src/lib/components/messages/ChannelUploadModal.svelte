<script lang="ts">
  import { t, locale } from '$lib/i18n'
  import { type ChannelInfo } from '$lib/canisters/messagechannel'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { getBytesString, getShortNumber } from '$lib/utils/helper'
  import { getModalStore } from '$lib/ui/stores'
  import { type SvelteComponent } from 'svelte'

  interface Props {
    parent: SvelteComponent
    channel: ChannelInfo
    file: File
    encryptBlob: (blob: Blob) => Promise<Uint8Array>
    onReady: (data: Uint8Array, type: string) => void
  }

  let { parent, channel, file, encryptBlob, onReady }: Props = $props()

  const MESSAGE_PER_USER_GAS = 10000
  const MESSAGE_PER_BYTE_GAS = 1000
  const UPLOAD_FILE_GAS_THRESHOLD = 10_000_000
  const modalStore = getModalStore()
  const filesState = channel.files_state[0] || {
    file_max_size: 0n,
    file_storage: []
  }

  let data: Uint8Array = $state(new Uint8Array())
  let mime = file.type
  let submitting = $state(false)
  let gas = $state(0)
  let uploadError = $state<'' | 'size' | 'balance'>('')
  const uploadErr = $derived(
    uploadError === 'size'
      ? $t('File size exceeds {size}', {
          size: getBytesString(filesState.file_max_size)
        })
      : uploadError === 'balance'
        ? $t('Insufficient gas balance {balance}, requires {required} gas', {
            balance: getShortNumber(channel.gas),
            required: getShortNumber(gas + UPLOAD_FILE_GAS_THRESHOLD)
          })
        : ''
  )

  async function checkFile(b: Blob) {
    data = await encryptBlob(b)
    if (data.byteLength > Number(filesState.file_max_size)) {
      uploadError = 'size'
    } else {
      uploadError = ''
    }

    // estimate gas
    gas =
      data.byteLength * MESSAGE_PER_BYTE_GAS +
      (channel.members.length + channel.managers.length) * MESSAGE_PER_USER_GAS

    if (
      uploadError == '' &&
      gas + UPLOAD_FILE_GAS_THRESHOLD > Number(channel.gas)
    ) {
      uploadError = 'balance'
    }
  }

  function uploadFile() {
    if (submitting) return
    submitting = true
    if (data.byteLength > 0) onReady(data, mime)
    modalStore.close()
  }

  checkFile(file)
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">{$t('Upload file')}</div>
  <p class="">{$t('File name:') + ' ' + file.name}</p>
  <p class="!mt-0"
    >{$t('File size: {size} bytes', {
      size: file.size.toLocaleString($locale)
    })}</p
  >
  <p class="!mt-0"
    >{$t('Encrypted size: {size} bytes', {
      size: data.byteLength.toLocaleString($locale)
    })}</p
  >
  <p class={uploadErr ? 'text-error-500' : 'text-panda'}
    >{$locale &&
      (uploadErr
        ? uploadErr
        : $t('Consume {amount} gas; balance: {balance}.', {
            amount: getShortNumber(gas),
            balance: getShortNumber(channel.gas)
          }))}</p
  >
  <button
    class="variant-filled-primary btn !mt-6 w-full"
    disabled={submitting || !data.byteLength || uploadErr != ''}
    onclick={uploadFile}
  >
    {#if submitting}
      <span class=""><IconCircleSpin /></span>
      <span>{$t('Processing...')}</span>
    {:else}
      <span>{$t('Upload')}</span>
    {/if}
  </button>
</ModalCard>
