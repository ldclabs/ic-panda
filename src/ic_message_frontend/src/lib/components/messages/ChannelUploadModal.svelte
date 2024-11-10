<script lang="ts">
  import { type ChannelInfo } from '$lib/canisters/messagechannel'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { getBytesString, getShortNumber } from '$lib/utils/helper'
  import { getModalStore } from '@skeletonlabs/skeleton'
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
  let uploadErr = $state('')
  let gas = $state(0)

  async function checkFile(b: Blob) {
    data = await encryptBlob(b)
    if (data.byteLength > Number(filesState.file_max_size)) {
      uploadErr = `File size exceeds ${getBytesString(filesState.file_max_size)}`
    } else {
      uploadErr = ''
    }

    // estimate gas
    gas =
      data.byteLength * MESSAGE_PER_BYTE_GAS +
      (channel.members.length + channel.managers.length) * MESSAGE_PER_USER_GAS

    if (
      uploadErr == '' &&
      gas + UPLOAD_FILE_GAS_THRESHOLD > Number(channel.gas)
    ) {
      uploadErr = `Insufficient gas balance ${getShortNumber(channel.gas)}, requires ${getShortNumber(gas + UPLOAD_FILE_GAS_THRESHOLD)} gas`
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
  <div class="!mt-0 text-center text-xl font-bold">Upload file</div>
  <p class="">{'File name: ' + file.name}</p>
  <p class="!mt-0">{'File size: ' + file.size + ' bytes'}</p>
  <p class="!mt-0">{'Encrypted size: ' + data.byteLength + ' bytes'}</p>
  <p class={uploadErr ? 'text-error-500' : 'text-panda'}
    >{uploadErr
      ? uploadErr
      : 'Consume ' +
        getShortNumber(gas) +
        ' gas, balance ' +
        getShortNumber(channel.gas)}</p
  >
  <button
    class="variant-filled-primary btn !mt-6 w-full"
    disabled={submitting || !data.byteLength || uploadErr != ''}
    onclick={uploadFile}
  >
    {#if submitting}
      <span class=""><IconCircleSpin /></span>
      <span>Processing...</span>
    {:else}
      <span>Upload</span>
    {/if}
  </button>
</ModalCard>
