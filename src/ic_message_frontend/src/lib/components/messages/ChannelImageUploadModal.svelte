<script lang="ts">
  import { type ChannelInfo } from '$lib/canisters/messagechannel'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { agent } from '$lib/stores/auth'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { bytesToBase64Url } from '$lib/utils/crypto'
  import { imageUrl } from '$lib/utils/url'
  import ImageCrop from '$src/lib/components/ui/ImageCrop.svelte'
  import {
    BucketCanister,
    Uploader,
    toFixedChunkSizeReadable
  } from '@ldclabs/ic_oss_ts'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onDestroy, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let channel: ChannelInfo
  export let onFinished: () => void

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let blob: Blob | null = null
  let croppedUrl = ''
  let submitting = false

  function handleImageUpload(event: CustomEvent) {
    blob = event.detail.blob || null
    if (blob) {
      croppedUrl && URL.revokeObjectURL(croppedUrl)
      croppedUrl = URL.createObjectURL(blob)
    }
  }

  function uploadImage() {
    if (submitting) return
    submitting = true
    toastRun(async () => {
      if (!blob) return
      const api = await myState.api.channelAPI(channel.canister)
      const token = await api.upload_image_token({
        size: BigInt(blob.size),
        content_type: blob.type,
        channel: channel.id
      })
      const file = await toFixedChunkSizeReadable({
        content: blob,
        name: token.name,
        contentType: blob.type
      })
      const bucketClient = BucketCanister.create({
        agent,
        canisterId: token.storage[0],
        accessToken: new Uint8Array(token.access_token)
      })
      const uploader = new Uploader(bucketClient)
      const rt = await uploader.upload_chunks(file, token.id, blob.size)
      const url = imageUrl(
        token.storage[0],
        token.id,
        token.name,
        rt.hash ? bytesToBase64Url(rt.hash) : ''
      )
      await api.update_channel({
        id: channel.id,
        name: [],
        description: [],
        image: [url]
      })

      onFinished()
      modalStore.close()
    }, toastStore)
  }

  onDestroy(() => {
    croppedUrl && URL.revokeObjectURL(croppedUrl)
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Update Image</div>
  <div class="mx-auto !mt-6 space-y-4">
    <ImageCrop on:cropcomplete={handleImageUpload} />

    <div class="mx-auto size-[200px] rounded bg-surface-500/10">
      {#if croppedUrl}
        <Avatar
          src={croppedUrl}
          border="border border-panda/90"
          background=""
          fill=""
          width="w-[200px]"
        />
      {/if}
    </div>
  </div>
  <button
    class="variant-filled-primary btn !mt-6 w-full"
    disabled={submitting || !blob}
    on:click={uploadImage}
    >{#if submitting}
      <span class=""><IconCircleSpin /></span>
      <span>Processing...</span>
    {:else}
      <span>Update</span>
    {/if}</button
  >
</ModalCard>
